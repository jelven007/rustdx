//! Deterministic local transport tests; these are not live market captures.
use rustdx_complete::tcp::{PACK1, TcpConfig, stock::*};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
#[path = "common/extended_protocol.rs"]
mod common;
#[path = "common/ip_pool.rs"]
mod ip_pool;
use common::*;

fn request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut header = [0; 10];
    stream.read_exact(&mut header)?;
    let n = u16::from_le_bytes([header[6], header[7]]) as usize;
    let mut bytes = header.to_vec();
    bytes.resize(10 + n, 0);
    stream.read_exact(&mut bytes[10..])?;
    Ok(bytes)
}
fn config(listener: &TcpListener) -> TcpConfig {
    TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool::prepare(listener)),
        timeout: Duration::from_secs(1),
        ..Default::default()
    }
}
fn handshake(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let req = request(stream).unwrap();
    assert_eq!(req, PACK1);
    stream.write_all(&response(&req, &[0, 0])).unwrap();
}
#[test]
fn bounded_batch_preserves_input_order_and_individual_decode_errors() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = config(&listener);
    let lane_count = Arc::new(AtomicUsize::new(0));
    let lane_counter = lane_count.clone();
    let server = thread::spawn(move || {
        let mut lanes = Vec::new();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        // One client query socket and exactly two batch worker sockets.
        for index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let counter = lane_counter.clone();
            let barrier = barrier.clone();
            lanes.push(thread::spawn(move || {
                handshake(&mut stream);
                if index > 0 {
                    counter.fetch_add(1, Ordering::Relaxed);
                    barrier.wait();
                }
                while let Ok(req) = request(&mut stream) {
                    assert_eq!(&req[10..12], &0x051bu16.to_le_bytes());
                    let code = &req[14..20];
                    let payload = if code == b"000002" {
                        vec![1, 0]
                    } else {
                        let value = std::str::from_utf8(code).unwrap().parse::<i64>().unwrap();
                        let mut bytes = vec![1, 0];
                        bytes.extend(varint(value));
                        bytes.extend(varint(-value));
                        bytes
                    };
                    stream.write_all(&response(&req, &payload)).unwrap();
                }
            }));
        }
        for lane in lanes {
            lane.join().unwrap();
        }
    });
    let client = Client::with_config(&config).unwrap();
    let rows = client
        .intraday_aux_batch(
            &[(0, "000004"), (0, "000002"), (0, "000001"), (0, "000003")],
            IntradayAuxKind::BuySellStrength,
            2,
        )
        .unwrap();
    assert_eq!(rows.len(), 4);
    for (index, value) in [(0, 4), (2, 1), (3, 3)] {
        assert_eq!(
            rows[index].as_ref().unwrap().page.records[0],
            IntradayAuxPoint::IntegerPair {
                index: 0,
                series_a: value,
                series_b: -value
            }
        );
    }
    assert!(rows[1].is_err());
    assert_eq!(lane_count.load(Ordering::Relaxed), 2);
    assert!(
        client
            .intraday_aux_batch(
                &vec![(0, "000001"); 100_001],
                IntradayAuxKind::BuySellStrength,
                2
            )
            .is_err()
    );
    drop(client);
    server.join().unwrap();
}
#[test]
fn special_scan_advances_by_returned_count_and_reports_record_budget() {
    for budget in [1, 10] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = config(&listener);
        let starts = Arc::new(Mutex::new(Vec::new()));
        let captured = starts.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handshake(&mut stream);
            while let Ok(req) = request(&mut stream) {
                let start = u16::from_le_bytes([req[12], req[13]]);
                captured.lock().unwrap().push(start);
                let mut payload = if start < 2 { vec![1, 0] } else { vec![0, 0] };
                if start < 2 {
                    payload.push(0);
                    payload.extend((u32::from(start) + 1).to_le_bytes());
                    payload.extend(12f32.to_le_bytes());
                    payload.extend(10f32.to_le_bytes());
                }
                stream.write_all(&response(&req, &payload)).unwrap();
            }
        });
        let mut client = Client::with_config(&config).unwrap();
        let result = client
            .special_limits_all(CommandScanOptions {
                max_records: budget,
                ..Default::default()
            })
            .unwrap();
        if budget == 1 {
            assert_eq!(result.termination, CommandTermination::RecordBudget);
            assert!(!result.complete);
            assert_eq!(result.record_count, 1);
            assert_eq!(*starts.lock().unwrap(), [0, 1]);
        } else {
            assert_eq!(result.termination, CommandTermination::EmptyPage);
            assert!(result.complete);
            assert_eq!(result.record_count, 2);
            assert_eq!(*starts.lock().unwrap(), [0, 1, 2]);
        }
        drop(client);
        server.join().unwrap();
    }
}
#[test]
fn auction_scan_rejects_repeated_page_instead_of_claiming_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = config(&listener);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        handshake(&mut stream);
        for expected_start in [0u32, 1] {
            let req = request(&mut stream).unwrap();
            assert_eq!(&req[32..36], &expected_start.to_le_bytes());
            let mut payload = vec![1, 0];
            payload.extend(555u16.to_le_bytes());
            payload.extend(12f32.to_le_bytes());
            payload.extend(100u32.to_le_bytes());
            payload.extend(1i32.to_le_bytes());
            payload.extend([0, 0]);
            stream.write_all(&response(&req, &payload)).unwrap();
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    let error = client
        .auction_series_all(
            0,
            "000001",
            None,
            AuctionOptions {
                count: 1,
                ..Default::default()
            },
            CommandScanOptions::default(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("未推进"));
    drop(client);
    server.join().unwrap();
}

#[test]
fn scan_rejects_same_endpoint_reconnect_between_pages() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = config(&listener);
    config.auto_reconnect = 1;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        handshake(&mut stream);
        let req = request(&mut stream).unwrap();
        let mut payload = vec![1, 0, 0];
        payload.extend(1u32.to_le_bytes());
        payload.extend(12f32.to_le_bytes());
        payload.extend(10f32.to_le_bytes());
        stream.write_all(&response(&req, &payload)).unwrap();
        let req = request(&mut stream).unwrap();
        assert_eq!(&req[12..14], &1u16.to_le_bytes());
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        handshake(&mut stream);
        let req = request(&mut stream).unwrap();
        assert_eq!(&req[12..14], &1u16.to_le_bytes());
        stream.write_all(&response(&req, &[0, 0])).unwrap();
    });
    let mut client = Client::with_config(&config).unwrap();
    let error = client
        .special_limits_all(CommandScanOptions::default())
        .unwrap_err();
    assert!(error.to_string().contains("连接或观察日期改变"));
    drop(client);
    server.join().unwrap();
}
