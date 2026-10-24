//! Synthetic resource faults; no fixture here represents a live trading window.
use rustdx_complete::tcp::{PACK1, Tcp, TcpConfig, stock::*};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};
#[path = "common/ip_pool.rs"]
mod ip_pool;
#[path = "common/extended_protocol.rs"]
mod protocol;

fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut head = [0; 10];
    stream.read_exact(&mut head)?;
    let length = usize::from(u16::from_le_bytes([head[6], head[7]]));
    if length > 1024 {
        return Err(io::Error::other("test request budget"));
    }
    let mut bytes = head.to_vec();
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
fn response(request: &[u8], content: &[u8]) -> Vec<u8> {
    assert_eq!(request.len(), 320);
    assert_eq!(&request[10..12], &0x06b9u16.to_le_bytes());
    let offset = u32::from_le_bytes(request[12..16].try_into().unwrap()) as usize;
    let size = u32::from_le_bytes(request[16..20].try_into().unwrap()) as usize;
    let data = &content[offset.min(content.len())..(offset + size).min(content.len())];
    let mut payload = (data.len() as u32).to_le_bytes().to_vec();
    payload.extend_from_slice(data);
    payload.extend_from_slice(&[0xaa, 0xbb]);
    payload
}
fn options() -> ResourceReadOptions {
    ResourceReadOptions {
        chunk_size: 5,
        page_delay: Duration::ZERO,
        ..Default::default()
    }
}

#[test]
fn path_ranges_and_declared_length_are_checked_and_tails_are_not_file_bytes() {
    let request = FileContentRequest::new(" T0002\\file.dat ", 7, 5).unwrap();
    let bytes = request.encode(42).unwrap();
    assert_eq!(bytes.len(), 320);
    assert_eq!(&bytes[20..34], b"T0002/file.dat");
    let chunk = decode_file_content(&request, &[3, 0, 0, 0, 1, 2, 3, 99]).unwrap();
    assert_eq!(chunk.content, [1, 2, 3]);
    assert_eq!(chunk.trailing_payload, [99]);
    assert!(chunk.is_last());
    for payload in [
        &[0, 0, 0][..],
        &[4, 0, 0, 0, 1, 2, 3],
        &[6, 0, 0, 0, 1, 2, 3, 4, 5, 6],
    ] {
        assert!(decode_file_content(&request, payload).is_err());
    }
    for (path, offset, size) in [
        ("", 0, 1),
        ("统计.zip", 0, 1),
        ("a\0b", 0, 1),
        ("a", u32::MAX, 1),
        ("a", 0, 0),
        ("a", 0, 60001),
    ] {
        assert_eq!(
            FileContentRequest::new(path, offset, size)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(FileContentRequest::new(&"a".repeat(300), 0, 60000).is_ok());
    assert!(FileContentRequest::new(&"a".repeat(301), 0, 1).is_err());
}
#[test]
fn full_download_observes_short_or_empty_chunk_and_retains_source_and_tail() {
    for content in [vec![], b"abcdefgh".to_vec(), b"abcdefghij".to_vec()] {
        let expected = content.clone();
        let (config, handle) = server(1, move |request| Some(response(request, &content)));
        let mut client = Client::with_config(&config).unwrap();
        let file = client.resource_file("zhb.zip", options()).unwrap();
        assert_eq!(file.require_complete().unwrap(), expected);
        assert_eq!(
            file.termination,
            if expected.len() % 5 == 0 {
                ResourceTermination::EmptyChunk
            } else {
                ResourceTermination::ShortChunk
            }
        );
        for chunk in &file.chunks {
            assert_eq!(chunk.trailing_payload, [0xaa, 0xbb]);
            assert_eq!(
                chunk.source.connection_id,
                file.chunks[0].source.connection_id
            );
        }
        drop(client);
        handle.join().unwrap();
    }
}
#[test]
fn byte_chunk_and_response_budgets_never_claim_a_complete_file() {
    for (opts, termination, length) in [
        (
            ResourceReadOptions {
                max_bytes: 10,
                ..options()
            },
            ResourceTermination::ByteBudget,
            10,
        ),
        (
            ResourceReadOptions {
                max_chunks: 1,
                ..options()
            },
            ResourceTermination::ChunkBudget,
            5,
        ),
        (
            ResourceReadOptions {
                max_response_bytes: 4,
                ..options()
            },
            ResourceTermination::ResponseBudget,
            0,
        ),
    ] {
        let (config, handle) = server(1, |request| Some(response(request, b"abcdefghij")));
        let mut tcp = Tcp::with_config(&config).unwrap();
        let file = download_resource_file(&mut tcp, "zhb.zip", opts).unwrap();
        assert_eq!(file.content.len(), length);
        assert_eq!(file.termination, termination);
        assert!(!file.complete && file.require_complete().is_err());
        drop(tcp);
        handle.join().unwrap();
    }
}
#[test]
fn reconnect_and_outer_deadline_cannot_deliver_a_successful_file() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let count = AtomicUsize::new(0);
    let (mut config, handle) = server(2, move |request| {
        if count.fetch_add(1, Ordering::SeqCst) == 1 {
            None
        } else {
            Some(response(request, b"abcdefgh"))
        }
    });
    config.auto_reconnect = 1;
    let mut tcp = Tcp::with_config(&config).unwrap();
    assert_eq!(
        download_resource_file(&mut tcp, "zhb.zip", options())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    drop(tcp);
    handle.join().unwrap();
    let (config, handle) = server(1, |request| {
        std::thread::sleep(Duration::from_millis(35));
        Some(response(request, b"abcdefghijklmnop"))
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    let op =
        rustdx_complete::operation::OperationContext::new(Duration::from_millis(60), None).unwrap();
    let _guard = op.enter();
    let start = std::time::Instant::now();
    assert!(matches!(
        download_resource_file(&mut tcp, "zhb.zip", options())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(tcp);
    handle.join().unwrap();
}
fn cfg(stat: bool) -> Vec<u8> {
    let mut fields = vec!["".to_owned(); if stat { 35 } else { 21 }];
    fields[0] = "0".into();
    fields[1] = "000001".into();
    if stat {
        fields[2] = "nan".into();
        fields[3] = "10.25".into();
        fields[4] = "20261023".into();
        fields[26] = "1.5".into();
        fields[31] = "20".into();
        fields[32] = "1e99".into();
    } else {
        fields[2] = "20261023".into();
        fields[3] = "100".into();
    }
    format!("header\n{}\n", fields.join("|")).into_bytes()
}
fn archive(stat: &[u8], stat2: &[u8], deflate: bool) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(if deflate {
        zip::CompressionMethod::Deflated
    } else {
        zip::CompressionMethod::Stored
    });
    for (name, bytes) in [("tdxstat.cfg", stat), ("tdxstat2.cfg", stat2)] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn stats_preserve_raw_slots_headers_and_missing_numbers_without_inventing_verified_units() {
    for deflate in [false, true] {
        let bytes = archive(&cfg(true), &cfg(false), deflate);
        let stats = parse_tdx_stats_archive(&bytes, "zhb.zip").unwrap();
        assert_eq!(stats.raw_archive.as_ref(), bytes);
        let row = stats.row("tdxstat.cfg", 0, "000001").unwrap();
        assert_eq!(row.fields_raw.len(), 35);
        let StatsCandidateValues::Stat(values) = &row.candidate_values else {
            panic!()
        };
        assert_eq!(values.pe_ttm, Some(10.25));
        assert_eq!(values.beta_60d, None);
        assert_eq!(values.year_limit_up_days, None);
        assert_eq!(values.limit_up_count_in_stat_days, None);
        assert_eq!(values.limit_stat_days, Some(20));
        assert_eq!(stats.members[0].unparsed_lines, [(1, "header".into())]);
    }
}
#[test]
fn stats_reject_crc_corruption_bad_gbk_duplicate_keys_and_hidden_duplicate_members() {
    let valid = archive(&cfg(true), &cfg(false), false);
    let mut faults = vec![
        valid[..valid.len() - 1].to_vec(),
        archive(b"\x81", &cfg(false), true),
    ];
    let mut duplicate = cfg(true);
    duplicate.extend(cfg(true));
    faults.push(archive(&duplicate, &cfg(false), true));
    let mut crc = valid.clone();
    let offset = crc.windows(7).position(|w| w == b"000001|").unwrap();
    crc[offset] = b'1';
    faults.push(crc);
    let end = valid.len() - 22;
    let start = u32::from_le_bytes(valid[end + 16..end + 20].try_into().unwrap()) as usize;
    let first_len = 46 + "tdxstat.cfg".len();
    let mut hidden = valid[..end].to_vec();
    hidden.extend_from_slice(&valid[start..start + first_len]);
    hidden.extend_from_slice(&valid[end..]);
    let new_end = hidden.len() - 22;
    hidden[new_end + 8..new_end + 12].copy_from_slice(&[3, 0, 3, 0]);
    hidden[new_end + 12..new_end + 16]
        .copy_from_slice(&((end - start + first_len) as u32).to_le_bytes());
    faults.push(hidden);
    let mut huge = valid.clone();
    huge[start + 24..start + 28].copy_from_slice(&(32 * 1024 * 1024u32 + 1).to_le_bytes());
    faults.push(huge);
    let mut encrypted = valid.clone();
    encrypted[start + 8] |= 1;
    faults.push(encrypted);
    for fault in faults {
        assert_eq!(
            parse_tdx_stats_archive(&fault, "zhb.zip")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
#[test]
fn stats_entry_refuses_a_budget_prefix_even_if_that_prefix_is_a_valid_zip() {
    let bytes = archive(&cfg(true), &cfg(false), true);
    let size = bytes.len() as u32;
    let (config, handle) = server(1, move |request| Some(response(request, &bytes)));
    let mut client = Client::with_config(&config).unwrap();
    assert_eq!(
        client
            .resource_stats(
                "zhb.zip",
                ResourceReadOptions {
                    max_bytes: size,
                    chunk_size: size,
                    ..options()
                }
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    drop(client);
    handle.join().unwrap();
}
