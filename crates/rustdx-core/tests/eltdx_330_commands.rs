//! 3.3.0's four added registry entries: independent bytes and local wire tests.
use rustdx_complete::tcp::{PACK1, TcpConfig, stock::*};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};
#[path = "common/ip_pool.rs"]
mod ip_pool;
#[path = "common/extended_protocol.rs"]
mod protocol;
use protocol::{response, varint};

const VERIFIED_052B: &[u8] = &[
    1, 0, 0x18, 0xb1, 0x34, 0x01, 0xb8, 0x33, 0xb4, 0x02, 0x88, 0x03, 0, 0x84, 0xe4, 0x9a, 0x04,
    0xdb, 0xb8, 0x67, 0x4b, 0, 0, 1, 0,
];
fn metadata(size: u32) -> Vec<u8> {
    let mut bytes = size.to_le_bytes().to_vec();
    bytes.push(0xab);
    bytes.extend_from_slice(b"0123456789abcdef0123456789abcdef");
    bytes.push(0);
    bytes
}
fn directory(length: u32) -> Vec<u8> {
    let mut bytes = vec![1, 0];
    bytes.extend_from_slice(&[0xb9, 0xab, 0xcb, 0xbe]); // 公司
    bytes.resize(66, 0);
    bytes.extend_from_slice(&[0xd7, 0xca, 0xc1, 0xcf]); // 资料
    bytes.extend_from_slice(b".txt");
    bytes.resize(146, 0);
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes
}
fn text_chunk(content: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xab; 10];
    bytes.extend_from_slice(&(content.len() as u16).to_le_bytes());
    bytes.extend_from_slice(content);
    bytes
}
fn bars(rows: &[(u32, i64)]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    let mut previous = 0;
    for &(day, price) in rows {
        bytes.extend_from_slice(&day.to_le_bytes());
        bytes.extend(varint(price - previous));
        for value in [0, 1, -1, 10] {
            bytes.extend(varint(value));
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 1, 0]);
        previous = price;
    }
    bytes
}
fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut header = [0; 10];
    stream.read_exact(&mut header)?;
    let mut bytes = header.to_vec();
    bytes.resize(
        10 + usize::from(u16::from_le_bytes([header[6], header[7]])),
        0,
    );
    stream.read_exact(&mut bytes[10..])?;
    Ok(bytes)
}
fn server(
    connections: usize,
    reply: impl Fn(&[u8]) -> Vec<u8> + Send + Sync + 'static,
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
        let mut lanes = Vec::new();
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();
            let reply = reply.clone();
            lanes.push(std::thread::spawn(move || {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                assert_eq!(read_request(&mut stream).unwrap(), PACK1);
                stream.write_all(&response(PACK1, &[0, 0])).unwrap();
                while let Ok(request) = read_request(&mut stream) {
                    let bytes = response(&request, &reply(&request));
                    for fragment in bytes.chunks(503) {
                        if stream.write_all(fragment).is_err() {
                            return;
                        }
                    }
                }
            }));
        }
        for lane in lanes {
            lane.join().unwrap();
        }
    });
    (config, handle)
}

#[test]
fn all_four_requests_match_complete_reference_bodies() {
    let meta = BlockMetaRequest::new(" block_zs.dat ")
        .unwrap()
        .encode(7)
        .unwrap();
    assert_eq!(&meta[1..5], &7u32.to_le_bytes());
    assert_eq!(&meta[10..12], &0x02c5u16.to_le_bytes());
    let mut expected = b"block_zs.dat".to_vec();
    expected.resize(40, 0);
    assert_eq!(&meta[12..], expected);
    let directory = CompanyInfoDirectoryRequest::new(2, "000001")
        .unwrap()
        .encode(9)
        .unwrap();
    assert_eq!(&directory[10..12], &0x02cfu16.to_le_bytes());
    assert_eq!(
        &directory[12..],
        &[2, 0, b'0', b'0', b'0', b'0', b'0', b'1', 0, 0, 0, 0]
    );
    let content = CompanyInfoTextRequest::new(0, "000001", "资料.txt", 1234, 70000).unwrap();
    let bytes = content.encode(11).unwrap();
    assert_eq!(&bytes[10..12], &0x02d0u16.to_le_bytes());
    let mut expected = vec![0, 0];
    expected.extend_from_slice(b"000001");
    expected.extend_from_slice(&[0, 0, 0xd7, 0xca, 0xc1, 0xcf]);
    expected.extend_from_slice(b".txt");
    expected.resize(90, 0);
    expected.extend_from_slice(&1234u32.to_le_bytes());
    expected.extend_from_slice(&70000u32.to_le_bytes());
    expected.extend_from_slice(&[0; 4]);
    assert_eq!(&bytes[12..], expected);
    let bars = DelistedBarsRequest::new(0, "000038", 800, 20)
        .unwrap()
        .encode(13)
        .unwrap();
    assert_eq!(&bars[10..12], &0x052bu16.to_le_bytes());
    assert_eq!(
        &bars[12..],
        &[0, 0, b'0', b'0', b'0', b'0', b'3', b'8', 0x20, 3, 20, 0]
    );
}

#[test]
fn constructors_reject_lossy_or_overflowing_parameters() {
    for name in ["", "  ", "中.dat", "x\0x", &"a".repeat(41)] {
        assert!(BlockMetaRequest::new(name).is_err());
    }
    for code in ["00001", "00000x", "０００００１", "T00038"] {
        assert!(CompanyInfoDirectoryRequest::new(0, code).is_err());
        assert!(DelistedBarsRequest::new(0, code, 0, 1).is_err());
    }
    assert!(CompanyInfoDirectoryRequest::new(3, "000001").is_err());
    assert!(DelistedBarsRequest::new(3, "000001", 0, 1).is_err());
    for name in ["", "x\0x", "😀.txt", &"资".repeat(41)] {
        assert!(CompanyInfoTextRequest::new(0, "000001", name, 0, 1).is_err());
    }
    assert!(CompanyInfoTextRequest::new(0, "000001", "x", u32::MAX, 1).is_err());
    assert!(CompanyInfoTextRequest::new(0, "000001", "x", 0, 0).is_err());
    assert!(DelistedBarsRequest::new(0, "000001", 0, 0).is_err());
    assert!(DelistedBarsRequest::new(0, "000001", 0, 801).is_err());
    assert!(CompanyInfoTextRequest::new(0, "000001", &"资".repeat(40), 0, 1).is_ok());
}

#[test]
fn metadata_and_directory_preserve_hash_offsets_and_legal_tail() {
    let bytes = metadata(12345);
    for end in 0..bytes.len() {
        assert!(decode_block_metadata(&bytes[..end]).is_err());
    }
    let mut extra = bytes.clone();
    extra.extend([0xde, 0xad]);
    let meta = decode_block_metadata(&extra).unwrap();
    assert_eq!(meta.size, 12345);
    assert_eq!(meta.reserved_raw, 0xab);
    assert_eq!(meta.md5, "0123456789abcdef0123456789abcdef");
    assert_eq!(meta.trailing_payload, [0xde, 0xad]);
    assert_eq!(meta.raw_payload.as_ref(), extra);
    let request = CompanyInfoDirectoryRequest::new(0, "000001").unwrap();
    let bytes = directory(65004);
    for end in 0..bytes.len() {
        assert!(decode_company_info_directory(&request, &bytes[..end]).is_err());
    }
    let mut extra = bytes;
    extra.extend([1, 2]);
    let result = decode_company_info_directory(&request, &extra).unwrap();
    assert_eq!(result.entries[0].name, "公司");
    assert_eq!(result.entries[0].filename, "资料.txt");
    assert_eq!(
        (result.entries[0].start, result.entries[0].length),
        (7, 65004)
    );
    assert_eq!(result.entries[0].filename_raw.len(), 80);
    assert_eq!(result.trailing_payload, [1, 2]);
    extra[..2].copy_from_slice(&500u16.to_le_bytes());
    assert!(decode_company_info_directory(&request, &extra).is_err());
}

#[test]
fn text_chunks_check_declared_lengths_and_keep_partial_gbk_and_tail() {
    let request = CompanyInfoTextRequest::new(0, "000001", "x", 0, 4).unwrap();
    let bytes = text_chunk(&[0xb9, 0xab, 0xcb, 0xbe]);
    for end in 0..bytes.len() {
        assert!(decode_company_info_text(&request, &bytes[..end]).is_err());
    }
    let mut extra = bytes.clone();
    extra.extend([0xde, 0xad]);
    let result = decode_company_info_text(&request, &extra).unwrap();
    assert_eq!(result.decode_text().unwrap(), "公司");
    assert_eq!(result.header_raw, [0xab; 10]);
    assert_eq!(result.trailing_payload, [0xde, 0xad]);
    assert!(decode_company_info_text(&request, &text_chunk(b"abcde")).is_err());
    let partial = decode_company_info_text(&request, &text_chunk(&[0xb9])).unwrap();
    assert!(partial.decode_text().is_err());
    assert_eq!(partial.content_raw, [0xb9]);
}

#[test]
fn delisted_verified_sample_has_integer_shares_milli_prices_and_raw_suffix() {
    let request = DelistedBarsRequest::new(0, "000038", 0, 1).unwrap();
    let page = decode_delisted_bars(&request, VERIFIED_052B).unwrap();
    let bar = &page.records[0];
    assert_eq!(bar.close_price_milli, 3500);
    assert_eq!(bar.volume_shares, 4_413_700);
    assert_eq!(bar.volume_lots, 44137.0);
    assert_eq!(bar.last_close_price_milli, None);
    assert_eq!(bar.unknown_suffix_raw, [0, 0, 1, 0]);
    assert_eq!(bar.time.offset().local_minus_utc(), 28800);
    assert_eq!(bar.time.time().to_string(), "00:00:00");
    assert_eq!(bar.record_raw, VERIFIED_052B[2..]);
    for end in 0..VERIFIED_052B.len() {
        assert!(decode_delisted_bars(&request, &VERIFIED_052B[..end]).is_err());
    }
    let mut extra = VERIFIED_052B.to_vec();
    extra.push(0);
    assert!(decode_delisted_bars(&request, &extra).is_err());
}

#[test]
fn delisted_decode_rejects_bad_dates_prices_and_volume_without_panicking() {
    let request = DelistedBarsRequest::new(0, "000038", 0, 2).unwrap();
    let mut bytes = bars(&[(20260101, 1000), (20260102, 1010)]);
    let result = decode_delisted_bars(&request, &bytes).unwrap();
    assert_eq!(result.records[1].last_close_price_milli, Some(1000));
    assert_eq!(result.records[1].open_price_milli, 1010);
    bytes[2..6].copy_from_slice(&20260230u32.to_le_bytes());
    assert!(decode_delisted_bars(&request, &bytes).is_err());
    for values in [
        [1000, 0, -1, -1, 10], // high below open
        [1000, 0, 1, 1, 10],   // low above open
        [1000, 0, 1, -1, -1],
        [1000, 0, 1, -1, i64::from(u32::MAX) + 1],
        [i64::MAX, 1, 1, 0, 0],
    ] {
        let mut bytes = vec![1, 0];
        bytes.extend_from_slice(&20260101u32.to_le_bytes());
        for value in values {
            bytes.extend(varint(value));
        }
        bytes.extend([0; 8]);
        assert!(decode_delisted_bars(&request, &bytes).is_err());
    }
    let mut excessive = bars(&[(20260101, 1000)]);
    excessive[..2].copy_from_slice(&801u16.to_le_bytes());
    assert!(decode_delisted_bars(&request, &excessive).is_err());
}

#[test]
fn daily_amount_preserves_binary32_leading_bit_zero_and_subnormals() {
    assert_eq!(decode_delisted_amount(0), 0.0);
    assert_eq!(decode_delisted_amount(0x4000_0000), 2.0);
    assert_eq!(decode_delisted_amount(0x4001_0000), 2.015625);
    assert_eq!(decode_delisted_amount(0x4080_0000), 4.0);
    assert_eq!(decode_delisted_amount(0x4081_0000), 4.03125);
    assert_eq!(decode_delisted_amount(0x0001_0000), 2f64.powi(-133));
    let request = DailyBarsRequest::new(0, "000038", 0, 1).unwrap();
    for raw in [0x7f80_0000u32, 0x7fc0_0000, 0xbf80_0000] {
        let mut bytes = bars(&[(20260101, 1000)]);
        let index = bytes.len() - 8;
        bytes[index..index + 4].copy_from_slice(&raw.to_le_bytes());
        assert!(decode_daily_bars(&request, &bytes).is_err());
    }
}

#[test]
fn f10_reads_multiple_frames_before_decoding_a_split_chinese_character() {
    let mut content = vec![b'a'; 29999];
    content.extend_from_slice(&[0xb9, 0xab, 0xcb, 0xbe]);
    content.extend(vec![b'b'; 35001]);
    let length = content.len() as u32;
    let starts = Arc::new(Mutex::new(Vec::new()));
    let captured = starts.clone();
    let (config, handle) = server(1, move |request| {
        match u16::from_le_bytes([request[10], request[11]]) {
            0x02cf => directory(length),
            0x02d0 => {
                assert_eq!(
                    &request[22..30],
                    &[0xd7, 0xca, 0xc1, 0xcf, b'.', b't', b'x', b't']
                );
                let offset = u32::from_le_bytes(request[102..106].try_into().unwrap());
                let length = u32::from_le_bytes(request[106..110].try_into().unwrap());
                captured.lock().unwrap().push((offset, length));
                let start = (offset - 7) as usize;
                text_chunk(&content[start..start + length as usize])
            }
            _ => panic!("unexpected command"),
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    let result = client.f10(0, "000001").unwrap();
    assert_eq!(result[0].0, "公司");
    assert_eq!(&result[0].1[29999..30005], "公司");
    assert_eq!(
        *starts.lock().unwrap(),
        [(7, 30000), (30007, 30000), (60007, 5004)]
    );
    assert!(client.f10(3, "000001").is_err());
    assert!(
        client
            .block_metadata_batch(&["ok.dat", "坏.dat"], 2)
            .is_err()
    );
    assert!(
        client
            .company_info_directory_batch(&[(0, "00001")], 2)
            .is_err()
    );
    drop(client);
    handle.join().unwrap();
}

#[test]
fn all_resource_and_full_archive_batches_use_the_bounded_pool_in_input_order() {
    // The query socket plus two fresh connections per batch: max_idle=0.
    let (config, handle) = server(9, |request| {
        match u16::from_le_bytes([request[10], request[11]]) {
            0x02c5 => metadata(u32::from(request[12] - b'0')),
            0x02cf => directory(u32::from(request[19] - b'0')),
            0x02d0 => text_chunk(&[request[22]; 4]),
            0x052b if request[20..22] == [0, 0] => bars(&[(20260101, i64::from(request[19]))]),
            0x052b => bars(&[]),
            _ => panic!("unexpected opcode"),
        }
    });
    let client = Client::with_config(&config).unwrap();
    let meta = client
        .block_metadata_batch(&["4.dat", "1.dat", "3.dat"], 2)
        .unwrap();
    assert_eq!(
        meta.into_iter()
            .map(|v| v.unwrap().size)
            .collect::<Vec<_>>(),
        [4, 1, 3]
    );
    let stocks = [(0, "000004"), (1, "000001"), (0, "000003")];
    let directories = client.company_info_directory_batch(&stocks, 2).unwrap();
    assert_eq!(
        directories
            .into_iter()
            .map(|v| v.unwrap().entries[0].length)
            .collect::<Vec<_>>(),
        [4, 1, 3]
    );
    let requests: Vec<_> = ["4.txt", "1.txt", "3.txt"]
        .iter()
        .map(|name| CompanyInfoTextRequest::new(0, "000001", name, 0, 4).unwrap())
        .collect();
    let texts = client
        .company_info_text_batch(&requests, CompanyInfoReadOptions::default(), 2)
        .unwrap();
    assert_eq!(
        texts
            .into_iter()
            .map(|v| v.unwrap().text)
            .collect::<Vec<_>>(),
        ["4444", "1111", "3333"]
    );
    let archives = client
        .delisted_bars_all_batch(&stocks, CommandScanOptions::default(), 2)
        .unwrap();
    for (archive, stock) in archives.into_iter().zip(stocks) {
        let archive = archive.unwrap();
        assert!(archive.complete);
        assert_eq!(archive.records.len(), 1);
        assert_eq!(archive.pages[0].request.code(), stock.1);
        assert_eq!(archive.pages[0].request.market(), stock.0);
    }
    assert!(
        client
            .company_info_text_batch(
                &[],
                CompanyInfoReadOptions {
                    max_chunks: 0,
                    ..Default::default()
                },
                2
            )
            .is_err()
    );
    assert!(
        client
            .delisted_bars_all_batch(&[(0, "bad")], CommandScanOptions::default(), 2)
            .is_err()
    );
    drop(client);
    handle.join().unwrap();
}

#[test]
fn zero_length_f10_sections_do_not_send_an_invalid_text_request() {
    let (config, handle) = server(1, |request| {
        assert_eq!(&request[10..12], &0x02cfu16.to_le_bytes());
        directory(0)
    });
    let mut client = Client::with_config(&config).unwrap();
    assert_eq!(
        client.f10(0, "000001").unwrap(),
        [("公司".into(), String::new())]
    );
    drop(client);
    handle.join().unwrap();
}

#[test]
fn f10_short_chunks_continue_but_empty_and_exhausted_budgets_fail() {
    for mode in 0..3 {
        let (config, handle) = server(1, move |request| {
            let offset = u32::from_le_bytes(request[102..106].try_into().unwrap());
            if offset == 0 || mode != 1 {
                text_chunk(b"ab")
            } else {
                text_chunk(b"")
            }
        });
        let mut client = Client::with_config(&config).unwrap();
        let result = client.company_info_text(
            CompanyInfoTextRequest::new(0, "000001", "x", 0, 4).unwrap(),
            CompanyInfoReadOptions {
                max_chunks: if mode == 2 { 1 } else { 3 },
                ..Default::default()
            },
        );
        if mode == 0 {
            let text = result.unwrap();
            assert_eq!(text.text, "abab");
            assert_eq!(text.chunks.len(), 2);
            assert!(text.complete);
        } else {
            assert!(result.is_err());
        }
        drop(client);
        handle.join().unwrap();
    }
}

#[test]
fn delisted_short_pages_overlap_and_budgets_have_explicit_completion() {
    for mode in 0..5 {
        let starts = Arc::new(Mutex::new(Vec::new()));
        let captured = starts.clone();
        let (config, handle) = server(1, move |request| {
            let start = u16::from_le_bytes([request[20], request[21]]);
            captured.lock().unwrap().push(start);
            match start {
                0 => bars(&[(20260102, 1010)]),
                1 if mode == 3 => bars(&[(20260102, 1010)]),
                1 if mode == 4 => bars(&[(20260102, 999)]),
                1 => bars(&[(20260101, 1000), (20260102, 1010)]),
                3 => bars(&[]),
                _ => panic!("bad start"),
            }
        });
        let mut client = Client::with_config(&config).unwrap();
        let result = client.delisted_bars_scan(
            DelistedBarsRequest::new(0, "000038", 0, 800).unwrap(),
            CommandScanOptions {
                max_pages: if mode == 1 { 1 } else { 5 },
                max_records: if mode == 2 { 1 } else { 10 },
                ..Default::default()
            },
        );
        if mode >= 3 {
            assert!(result.is_err());
        } else {
            let result = result.unwrap();
            assert_eq!(result.complete, mode == 0);
            if mode == 0 {
                assert_eq!(
                    result
                        .records
                        .iter()
                        .map(|b| b.date_raw)
                        .collect::<Vec<_>>(),
                    [20260101, 20260102]
                );
                assert_eq!(result.raw_record_count, 3);
                assert_eq!(result.termination, CommandTermination::EmptyPage);
                assert_eq!(*starts.lock().unwrap(), [0, 1, 3]);
            } else if mode == 1 {
                assert_eq!(result.termination, CommandTermination::PageBudget);
            } else {
                assert_eq!(result.termination, CommandTermination::RecordBudget);
            }
        }
        drop(client);
        handle.join().unwrap();
    }
}

#[test]
fn delisted_offset_overflow_is_incomplete_and_batch_keeps_each_error() {
    let (config, handle) = server(1, |_| bars(&[(20260101, 1000)]));
    let mut client = Client::with_config(&config).unwrap();
    let result = client
        .delisted_bars_scan(
            DelistedBarsRequest::new(0, "000038", u16::MAX, 1).unwrap(),
            CommandScanOptions::default(),
        )
        .unwrap();
    assert_eq!(result.termination, CommandTermination::OffsetLimit);
    assert!(!result.complete);
    drop(client);
    handle.join().unwrap();
    let (config, handle) = server(3, |request| {
        if &request[14..20] == b"000002" {
            vec![1, 0]
        } else {
            bars(&[(20260101, i64::from(request[19]))])
        }
    });
    let client = Client::with_config(&config).unwrap();
    let requests: Vec<_> = ["000004", "000002", "000001", "000003"]
        .iter()
        .map(|code| DelistedBarsRequest::new(0, code, 0, 1).unwrap())
        .collect();
    let result = client.delisted_bars_batch(&requests, 2).unwrap();
    assert_eq!(result.len(), 4);
    assert!(result[1].is_err());
    for index in [0, 2, 3] {
        assert_eq!(
            result[index].as_ref().unwrap().request.code(),
            requests[index].code()
        );
    }
    drop(client);
    handle.join().unwrap();
}

#[test]
fn replays_all_98_independently_verified_live_frames_without_network() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eltdx-330-replay");
    let mut total = 0;
    let mut bars_count = 0;
    for name in ["live-a", "live-b"] {
        let directory = root.join(name);
        let proof: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("independent-verification.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(proof["verified"], true);
        let read = |stem: &str, suffix: &str| {
            std::fs::read(directory.join(format!("{stem}.{suffix}.bin"))).unwrap()
        };
        for frame in proof["frames"].as_array().unwrap() {
            let stem = frame["stem"].as_str().unwrap();
            let request = read(stem, "request");
            let payload = read(stem, "payload");
            let header: [u8; 16] = read(stem, "header").try_into().unwrap();
            let wire = read(stem, "wire");
            assert_eq!(
                rustdx_complete::tcp::decode_frame(&header, &wire, true).unwrap(),
                payload
            );
            let opcode = u16::from_le_bytes([request[10], request[11]]);
            let market = || u16::from_le_bytes([request[12], request[13]]);
            let code = || std::str::from_utf8(&request[14..20]).unwrap();
            match opcode {
                0x000d => {}
                0x02c5 => {
                    let meta = decode_block_metadata(&payload).unwrap();
                    assert_eq!(meta.raw_payload.as_ref(), payload);
                    assert!(meta.size > 0);
                    assert_eq!(meta.md5.len(), 32);
                }
                0x02cf => {
                    let r = CompanyInfoDirectoryRequest::new(market(), code()).unwrap();
                    let result = decode_company_info_directory(&r, &payload).unwrap();
                    assert!(!result.entries.is_empty());
                    assert_eq!(result.raw_payload.as_ref(), payload);
                }
                0x02d0 => {
                    let field = &request[22..102];
                    let end = field.iter().position(|b| *b == 0).unwrap_or(80);
                    let (filename, _, errors) = encoding_rs::GBK.decode(&field[..end]);
                    assert!(!errors);
                    let offset = u32::from_le_bytes(request[102..106].try_into().unwrap());
                    let length = u32::from_le_bytes(request[106..110].try_into().unwrap());
                    let r =
                        CompanyInfoTextRequest::new(market(), code(), &filename, offset, length)
                            .unwrap();
                    let result = decode_company_info_text(&r, &payload).unwrap();
                    assert_eq!(result.raw_payload.as_ref(), payload);
                    assert_eq!(
                        result.content_raw.len(),
                        usize::from(result.declared_length)
                    );
                }
                0x052b => {
                    let start = u16::from_le_bytes([request[20], request[21]]);
                    let count = u16::from_le_bytes([request[22], request[23]]);
                    let r = DelistedBarsRequest::new(market(), code(), start, count).unwrap();
                    let page = decode_delisted_bars(&r, &payload).unwrap();
                    assert_eq!(
                        page.records.len() as u64,
                        frame["record_count"].as_u64().unwrap()
                    );
                    let kind = if count == 1 { "one" } else { "all" };
                    let expected: serde_json::Value = serde_json::from_slice(
                        &std::fs::read(directory.join(format!(
                            "delisted-{kind}-{}-{}.json",
                            market(),
                            code()
                        )))
                        .unwrap(),
                    )
                    .unwrap();
                    let records = if kind == "one" {
                        &expected["records"]
                    } else {
                        &expected["pages"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|p| {
                                p["request"]["start"] == start && p["request"]["count"] == count
                            })
                            .unwrap()["records"]
                    };
                    for (bar, expected) in page.records.iter().zip(records.as_array().unwrap()) {
                        assert_eq!(
                            i64::from(bar.volume_shares),
                            expected["volume_shares"].as_i64().unwrap()
                        );
                        assert_eq!(
                            bar.close_price_milli,
                            expected["close_price_milli"].as_i64().unwrap()
                        );
                        assert_eq!(bar.date.to_string(), expected["date"].as_str().unwrap());
                        assert_eq!(
                            bar.amount_raw,
                            expected["amount_raw"].as_u64().unwrap() as u32
                        );
                    }
                    bars_count += page.records.len();
                }
                _ => panic!("unexpected captured opcode"),
            }
            total += 1;
        }
    }
    assert_eq!(total, 98);
    assert_eq!(bars_count, 24084);
}
