use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

use rustdx_cli::tcp_evidence::{FrameArchive, WireRecord, load_response, replay};
use rustdx_complete::tcp::{SecurityCount, Tcp, TcpConfig, Tdx, decode_frame};
use tempfile::TempDir;

#[path = "../../../crates/rustdx-core/tests/common/ip_pool.rs"]
mod ip_pool_test;

#[test]
fn replay_rejects_mismatched_identity_before_mutating_the_parser() {
    for offset in [0, 5, 10] {
        let mut parser = SecurityCount::new(0);
        parser.try_parse(42u16.to_le_bytes().to_vec()).unwrap();
        let request = parser.send().to_vec();
        let body = miniz_oxide::deflate::compress_to_vec_zlib(&123u16.to_le_bytes(), 6);
        let mut header = [0; 16];
        header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
        header[5..9].copy_from_slice(&request[1..5]);
        header[10..12].copy_from_slice(&request[10..12]);
        header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
        header[14..16].copy_from_slice(&2u16.to_le_bytes());
        header[offset] ^= 1;
        let record = WireRecord {
            schema_version: 1,
            phase: "response".into(),
            captured_at: chrono::Utc::now().to_rfc3339(),
            endpoint: "127.0.0.1:7709".parse().unwrap(),
            request,
            header: Some(header),
            body,
            error_kind: None,
        };
        assert_eq!(
            replay(&record, &mut parser).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert_eq!(*parser.result(), 42);
    }
}

#[test]
fn compressed_wire_archive_survives_reconnect_and_replays_checked_parser() {
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("wire");
    let archive = Arc::new(FrameArchive::create(&directory, 1024 * 1024, 32).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut handshake = [0; 13];
            stream.read_exact(&mut handshake).unwrap();
            let mut header = [0; 16];
            header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
            header[5..9].copy_from_slice(&handshake[1..5]);
            header[10..12].copy_from_slice(&handshake[10..12]);
            header[12..14].copy_from_slice(&2_u16.to_le_bytes());
            header[14..16].copy_from_slice(&2_u16.to_le_bytes());
            stream.write_all(&header).unwrap();
            stream.write_all(&[0, 0]).unwrap();
            let mut request = [0; 18];
            stream.read_exact(&mut request).unwrap();
            header[5..9].copy_from_slice(&request[1..5]);
            header[10..12].copy_from_slice(&request[10..12]);
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&42_u16.to_le_bytes(), 6);
            header[12..14].copy_from_slice(&(compressed.len() as u16).to_le_bytes());
            header[14..16].copy_from_slice(&2_u16.to_le_bytes());
            // Deliberately split the header and body across writes.
            for bytes in [
                &header[..7],
                &header[7..],
                &compressed[..3],
                &compressed[3..],
            ] {
                stream.write_all(bytes).unwrap();
            }
        }
    });
    let mut tcp = Tcp::with_config_and_observer(&config, archive.clone()).unwrap();
    assert_eq!(tcp.heartbeat_legacy().unwrap(), 42);
    tcp.reconnect().unwrap();
    assert_eq!(tcp.heartbeat_legacy().unwrap(), 42);
    drop(tcp);
    server.join().unwrap();
    archive.finish().unwrap();
    for sequence in [3, 7] {
        let record = load_response(&directory, sequence).unwrap();
        assert_ne!(record.body, 42_u16.to_le_bytes());
        let mut parser = SecurityCount::new(0);
        replay(&record, &mut parser).unwrap();
        assert_eq!(*parser.result(), 42);
        let mut wrong = SecurityCount::new(1);
        assert!(replay(&record, &mut wrong).is_err());
        let mut header = record.header.unwrap();
        header[14..16].copy_from_slice(&1_u16.to_le_bytes());
        assert!(decode_frame(&header, &record.body, true).is_err());
    }
    std::fs::write(directory.join("000003.json"), b"tampered").unwrap();
    assert!(load_response(&directory, 3).is_err());
}

#[test]
fn exhausted_archive_refuses_handshake_before_any_request_transmission() {
    let temp = TempDir::new().unwrap();
    let archive = Arc::new(FrameArchive::create(&temp.path().join("wire"), 1, 1).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    });
    assert!(Tcp::with_config_and_observer(&config, archive).is_err());
    server.join().unwrap();
}

#[test]
fn invalid_compression_is_archived_and_truncated_response_is_marked_failed() {
    for truncated in [false, true] {
        let temp = TempDir::new().unwrap();
        let directory = temp.path().join("wire");
        let archive = Arc::new(FrameArchive::create(&directory, 1024 * 1024, 16).unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = TcpConfig {
            ip: Some(listener.local_addr().unwrap()),
            ip_pool: Some(ip_pool_test::prepare(&listener)),
            ..Default::default()
        };
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut handshake = [0; 13];
            stream.read_exact(&mut handshake).unwrap();
            let mut header = [0; 16];
            header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
            header[5..9].copy_from_slice(&handshake[1..5]);
            header[10..12].copy_from_slice(&handshake[10..12]);
            header[12..14].copy_from_slice(&2_u16.to_le_bytes());
            header[14..16].copy_from_slice(&2_u16.to_le_bytes());
            stream.write_all(&header).unwrap();
            stream.write_all(&[0, 0]).unwrap();
            let mut request = [0; 18];
            stream.read_exact(&mut request).unwrap();
            header[5..9].copy_from_slice(&request[1..5]);
            header[10..12].copy_from_slice(&request[10..12]);
            header[12..14].copy_from_slice(&(if truncated { 2_u16 } else { 1_u16 }).to_le_bytes());
            stream.write_all(&header).unwrap();
            stream.write_all(&[255]).unwrap();
        });
        let mut tcp = Tcp::with_config_and_observer(&config, archive.clone()).unwrap();
        assert!(tcp.heartbeat_legacy().is_err());
        assert!(!tcp.is_reusable());
        drop(tcp);
        server.join().unwrap();
        archive.finish().unwrap();
        if truncated {
            assert!(load_response(&directory, 3).is_err());
            assert!(
                std::fs::read_to_string(directory.join("000003.json"))
                    .unwrap()
                    .contains("failed")
            );
        } else {
            let record = load_response(&directory, 3).unwrap();
            assert_eq!(record.body, [255]);
            assert!(replay(&record, &mut SecurityCount::new(0)).is_err());
        }
    }
}
