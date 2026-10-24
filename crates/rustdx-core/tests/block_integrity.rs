//! Full-file integrity and strict board parsing, with bounded local wire faults.
use md5::{Digest, Md5};
use rustdx_complete::tcp::{PACK1, Tcp, TcpConfig, stock::*};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[path = "common/ip_pool.rs"]
mod ip_pool;
#[path = "common/extended_protocol.rs"]
mod protocol;

fn metadata(content: &[u8]) -> Vec<u8> {
    metadata_with(
        content.len() as u32,
        format!("{:x}", Md5::digest(content)).as_bytes(),
    )
}
fn metadata_with(size: u32, digest: &[u8]) -> Vec<u8> {
    let mut bytes = size.to_le_bytes().to_vec();
    bytes.push(0xab);
    bytes.extend_from_slice(digest);
    bytes.push(0);
    bytes
}
fn file(blocks: u16) -> Vec<u8> {
    let mut data = vec![0; 386 + usize::from(blocks) * 2813];
    data[384..386].copy_from_slice(&blocks.to_le_bytes());
    for slot in data[386..].chunks_exact_mut(2813) {
        slot[..6].copy_from_slice(&[0xef, 0xae, 0xb5, 0xe7, 0xb3, 0xd8]); // 锂电池
        slot[9..11].copy_from_slice(&1u16.to_le_bytes());
        slot[11..13].copy_from_slice(&7u16.to_le_bytes());
        slot[13..19].copy_from_slice(b"300750");
        slot[19] = 0xa7; // Unknown seventh byte is not interpreted as market.
    }
    data
}
fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut header = [0; 10];
    stream.read_exact(&mut header)?;
    let mut bytes = header.to_vec();
    let length = usize::from(u16::from_le_bytes([header[6], header[7]]));
    if length > 1024 {
        return Err(io::Error::other("test request budget"));
    }
    bytes.resize(10 + length, 0);
    stream.read_exact(&mut bytes[10..])?;
    Ok(bytes)
}
fn server(
    connections: usize,
    reply: impl Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static,
) -> (TcpConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool::prepare(&listener)),
        timeout: Duration::from_secs(1),
        ..Default::default()
    };
    let reply = Arc::new(reply);
    let handle = std::thread::spawn(move || {
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            assert_eq!(read_request(&mut stream).unwrap(), PACK1);
            stream
                .write_all(&protocol::response(PACK1, &[0, 0]))
                .unwrap();
            while let Ok(request) = read_request(&mut stream) {
                let Some(payload) = reply(&request) else {
                    break;
                };
                if stream
                    .write_all(&protocol::response(&request, &payload))
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    (config, handle)
}
fn opcode(request: &[u8]) -> u16 {
    u16::from_le_bytes([request[10], request[11]])
}
fn chunk(request: &[u8], content: &[u8]) -> Vec<u8> {
    assert_eq!(opcode(request), 0x06b9);
    assert_eq!(&request[20..32], b"block_gn.dat");
    let start = u32::from_le_bytes(request[12..16].try_into().unwrap()) as usize;
    let length = u32::from_le_bytes(request[16..20].try_into().unwrap()) as usize;
    assert!(length <= 30000);
    let mut bytes = vec![0xde, 0xad, 0xbe, 0xef];
    bytes.extend_from_slice(&content[start..start + length]);
    bytes
}

#[test]
fn metadata_verifies_known_md5_and_rejects_length_and_same_length_corruption() {
    let bytes = metadata_with(3, b"900150983CD24FB0D6963F7D28E17F72");
    let meta = decode_block_metadata(&bytes).unwrap();
    assert_eq!(
        meta.verify_content(b"abc").unwrap(),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(meta.raw_payload.as_ref(), bytes);
    for content in [b"abd".as_slice(), b"ab".as_slice(), b"abcd".as_slice()] {
        assert_eq!(
            meta.verify_content(content).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    for invalid in [
        b"g00150983cd24fb0d6963f7d28e17f72".as_slice(),
        b" 00150983cd24fb0d6963f7d28e17f72".as_slice(),
        b"\x0000150983cd24fb0d6963f7d28e17f72".as_slice(),
    ] {
        let meta = decode_block_metadata(&metadata_with(3, invalid)).unwrap();
        assert!(meta.verify_content(b"abc").is_err());
    }
    let empty =
        decode_block_metadata(&metadata_with(0, b"d41d8cd98f00b204e9800998ecf8427e")).unwrap();
    assert!(empty.verify_content(&[]).is_ok());
}

#[test]
fn strict_decoder_rejects_malformed_layout_and_member_fields_without_partial_success() {
    let valid = file(2);
    let records = decode_block_content(&valid).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].blockname, "锂电池");
    assert_eq!(records[0].code, "300750");
    assert_eq!(records[0].block_type, 7);
    let mut faults = Vec::new();
    faults.push(valid[..385].to_vec());
    faults.push(valid[..valid.len() - 1].to_vec()); // Uneven slots.
    let mut tiny = vec![0; 386 + 12];
    tiny[384..386].copy_from_slice(&1u16.to_le_bytes());
    faults.push(tiny);
    for offset in [386, 386 + 2813] {
        let mut overflow = valid.clone();
        overflow[offset + 9..offset + 11].copy_from_slice(&401u16.to_le_bytes());
        faults.push(overflow);
        let mut bad_gbk = valid.clone();
        bad_gbk[offset..offset + 9].fill(0);
        bad_gbk[offset] = 0x81;
        faults.push(bad_gbk);
        let mut bad_code = valid.clone();
        bad_code[offset + 13] = b'x';
        faults.push(bad_code);
        let mut missing_name = valid.clone();
        missing_name[offset..offset + 9].fill(0);
        faults.push(missing_name);
    }
    let mut zero_with_tail = valid.clone();
    zero_with_tail[384..386].fill(0);
    faults.push(zero_with_tail);
    for fault in faults {
        assert!(decode_block_content(&fault).is_err());
    }
    assert!(decode_block_content(&[]).unwrap().is_empty());
    assert!(decode_block_content(&[0; 386]).unwrap().is_empty());
    assert!(parse_block_content(&[0; 100]).is_empty()); // Legacy remains compatible.
}

#[test]
fn full_download_checks_multichunk_ranges_digest_raw_headers_and_sources() {
    let content = file(22);
    let expected = content.clone();
    let (config, handle) = server(1, move |request| {
        Some(if opcode(request) == 0x02c5 {
            metadata(&content)
        } else {
            chunk(request, &content)
        })
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(None).unwrap();
    let downloaded = download_block_file(&mut tcp, " block_gn.dat ").unwrap();
    assert_eq!(downloaded.content, expected);
    assert_eq!(downloaded.filename, "block_gn.dat");
    assert_eq!(downloaded.decode_records().unwrap().len(), 22);
    assert_eq!(downloaded.chunks.len(), 3);
    for (index, chunk) in downloaded.chunks.iter().enumerate() {
        assert_eq!(chunk.start, index as u32 * 30000);
        assert_eq!(chunk.header_raw, [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(
            chunk.source.connection_id,
            downloaded.metadata.source.connection_id
        );
        assert_eq!(chunk.source.endpoint, downloaded.metadata.source.endpoint);
    }
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn captured_full_file_matches_independent_md5_and_all_record_fields() {
    let content = include_bytes!("fixtures/block-integrity/block_zs.dat");
    let meta = decode_block_metadata(include_bytes!(
        "fixtures/block-integrity/block_zs.metadata.bin"
    ))
    .unwrap();
    assert_eq!(
        meta.verify_content(content).unwrap(),
        "deddfa997800581cf5cff4ea5cf200ca"
    );
    let records = decode_block_content(content).unwrap();
    assert_eq!(records.len(), 11554);
    assert_eq!(
        format!("{:x}", Md5::digest(serde_json::to_vec(&records).unwrap())),
        "235661e9cc49825c68de63e8bbfd7463"
    );
}

#[test]
fn whole_download_respects_outer_deadline_across_chunks() {
    let content = file(22);
    let (config, handle) = server(1, move |request| {
        Some(if opcode(request) == 0x02c5 {
            metadata(&content)
        } else {
            std::thread::sleep(Duration::from_millis(40));
            chunk(request, &content)
        })
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(None).unwrap();
    let operation =
        rustdx_complete::operation::OperationContext::new(Duration::from_millis(90), None).unwrap();
    let _guard = operation.enter();
    let started = std::time::Instant::now();
    let error = download_block_file(&mut tcp, "block_gn.dat").unwrap_err();
    // macOS may surface the socket's deadline as EWOULDBLOCK.
    assert!(matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn download_rejects_correct_length_content_with_wrong_md5() {
    let content = file(1);
    let (config, handle) = server(1, move |request| {
        Some(if opcode(request) == 0x02c5 {
            metadata_with(content.len() as u32, b"d41d8cd98f00b204e9800998ecf8427e")
        } else {
            chunk(request, &content)
        })
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    assert_eq!(
        download_block_file(&mut tcp, "block_gn.dat")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn download_rejects_short_chunk() {
    let content = file(1);
    let (config, handle) = server(1, move |request| {
        Some(if opcode(request) == 0x02c5 {
            metadata(&content)
        } else {
            let mut bytes = chunk(request, &content);
            bytes.pop();
            bytes
        })
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    assert_eq!(
        download_block_file(&mut tcp, "block_gn.dat")
            .unwrap_err()
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn empty_and_invalid_metadata_never_trigger_chunk_download() {
    for (size, digest, succeeds) in [
        (0, "d41d8cd98f00b204e9800998ecf8427e", true),
        (0, "00000000000000000000000000000000", false),
        (3, "g00150983cd24fb0d6963f7d28e17f72", false),
        (
            64 * 1024 * 1024 + 1,
            "d41d8cd98f00b204e9800998ecf8427e",
            false,
        ),
    ] {
        let (config, handle) = server(1, move |request| {
            assert_eq!(opcode(request), 0x02c5);
            Some(metadata_with(size, digest.as_bytes()))
        });
        let mut tcp = Tcp::with_config(&config).unwrap();
        let result = download_block_file(&mut tcp, "block_gn.dat");
        if succeeds {
            let file = result.unwrap();
            assert!(file.content.is_empty());
            assert!(file.chunks.is_empty());
            assert!(file.decode_records().unwrap().is_empty());
        } else {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        drop(tcp);
        handle.join().unwrap();
    }
}

#[test]
fn high_level_entries_reject_md5_valid_but_malformed_file_and_preserve_error_kind() {
    let mut content = file(1);
    content[386 + 13] = b'x';
    for client_entry in [false, true] {
        let bytes = content.clone();
        let (config, handle) = server(1, move |request| {
            Some(if opcode(request) == 0x02c5 {
                metadata(&bytes)
            } else {
                chunk(request, &bytes)
            })
        });
        let error = if client_entry {
            let mut client = Client::with_config(&config).unwrap();
            client.block("block_gn.dat").unwrap_err()
        } else {
            let mut tcp = Tcp::with_config(&config).unwrap();
            let result = get_block_info(&mut tcp, "block_gn.dat").unwrap_err();
            drop(tcp);
            result
        };
        let rustdx_complete::Error::Io(error) = error else {
            panic!("expected IO error")
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        handle.join().unwrap();
    }
}

#[test]
fn download_rejects_reconnect_between_metadata_and_content() {
    let content = file(1);
    let calls = AtomicUsize::new(0);
    let (mut config, handle) = server(2, move |request| {
        match calls.fetch_add(1, Ordering::SeqCst) {
            0 => Some(metadata(&content)),
            1 => None, // Close before replying; automatic retry opens a new connection.
            2 => Some(chunk(request, &content)),
            _ => panic!("unexpected extra request"),
        }
    });
    config.auto_reconnect = 1;
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(None).unwrap();
    let error = download_block_file(&mut tcp, "block_gn.dat").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("连接或观察日期"));
    drop(tcp);
    handle.join().unwrap();
}
