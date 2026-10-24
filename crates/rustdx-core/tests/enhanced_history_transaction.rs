//! 0fc6 profile fixtures, malformed payloads and local synchronous wire replay.
use rustdx_complete::{
    operation::OperationContext,
    tcp::stock::{
        EnhancedHistoryTransactionRequest, EnhancedTradeEventKind, EnhancedTradeSide,
        HistoryTransactionRequest, decode_enhanced_history_transactions,
        decode_history_transactions,
    },
};
use std::{io::ErrorKind, time::Duration};

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

fn request(start: u16, count: u16) -> EnhancedHistoryTransactionRequest<'static> {
    EnhancedHistoryTransactionRequest::new(0, "000001", start, count, 20261022).unwrap()
}

fn varint(value: i32, bytes: &mut Vec<u8>) {
    let mut magnitude = i64::from(value).unsigned_abs();
    let mut first = (magnitude & 0x3f) as u8 | if value < 0 { 0x40 } else { 0 };
    magnitude >>= 6;
    if magnitude != 0 {
        first |= 0x80;
    }
    bytes.push(first);
    while magnitude != 0 {
        let mut next = (magnitude & 0x7f) as u8;
        magnitude >>= 7;
        if magnitude != 0 {
            next |= 0x80;
        }
        bytes.push(next);
    }
}

fn payload(base: f32, rows: &[(u16, [i32; 5])]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    bytes.extend_from_slice(&base.to_le_bytes());
    for &(minutes, fields) in rows {
        bytes.extend_from_slice(&minutes.to_le_bytes());
        for value in fields {
            varint(value, &mut bytes);
        }
    }
    bytes
}

#[test]
fn pinned_eltdx_request_and_response_layouts_match() {
    // Frozen request body from eltdx 3.2.2 trades.rs, frame message id = 1.
    let history = EnhancedHistoryTransactionRequest::new(0, "300308", 0, 900, 20260511).unwrap();
    assert_eq!(
        history.encode(),
        [
            0x0c, 1, 0, 0, 0, 1, 0x12, 0, 0x12, 0, 0xc6, 0x0f, 0x9f, 0x26, 0x35, 1, 0, 0, b'3',
            b'0', b'0', b'3', b'0', b'8', 0, 0, 0x84, 3,
        ]
    );
    // Synthetic historical fixture in the pinned SDK, NOT a live-server capture.
    let frozen = [1, 0, 0, 0, 0x0e, 0x42, 0x50, 3, 0x0a, 0x14, 3, 5, 0];
    let page = decode_enhanced_history_transactions(&history, &frozen).unwrap();
    assert_eq!(page.price_base_raw_f32, 35.5);
    assert_eq!(page.price_base_raw_bytes, [0, 0, 0x0e, 0x42]);
    assert_eq!(page.raw_payload, frozen);
    assert_eq!(
        (page.market, page.code.as_str(), page.date),
        (0, "300308", 20260511)
    );
    let record = &page.records[0];
    assert_eq!((record.index, record.absolute_index), (0, 0));
    assert_eq!((record.time_minutes, record.time.as_str()), (848, "14:08"));
    assert_eq!(record.trade_datetime.to_string(), "2026-05-11 14:08:00");
    assert_eq!(
        (record.price_delta_raw, record.price_acc_raw, record.price),
        (10, 10, 0.1)
    );
    assert_eq!(
        (
            record.volume,
            record.order_count,
            record.status_raw,
            record.reserved_raw
        ),
        (20, 3, 5, 0)
    );
    assert_eq!(record.side(), EnhancedTradeSide::Status(5));
    assert_eq!(record.event_kind(), EnhancedTradeEventKind::Trade);
    assert!(!record.is_after_hours_fixed_price());
    assert_eq!(page.source_endpoint, None);
    assert_eq!(page.fetched_at, None);
}

#[test]
fn classification_preserves_wire_order_and_prioritizes_status() {
    let rows = [
        (565, [1000, 12, 3, 8, 0]), // 09:25 snapshot, not opening match.
        (565, [0, 14, 3, 0, 7]),    // Same minute/count, a legitimate separate row.
        (570, [-5, 10, 1, 1, 0]),
        (571, [5, 0, 0, 2, 0]),
        (900, [0, 10, 1, 0, 0]),
        (904, [0, 10, 1, 5, 0]), // Before after-hours boundary.
        (905, [0, 10, 1, 5, 0]),
        (930, [0, 10, 1, 5, 0]),
        (931, [0, 10, 1, 5, 0]),   // After after-hours boundary.
        (931, [0, 10, 1, 99, -1]), // Unknown status and reserved values stay raw.
        (932, [0, 10, 1, -1, 0]),
    ];
    let page =
        decode_enhanced_history_transactions(&request(20, 20), &payload(123.0, &rows)).unwrap();
    assert_eq!(page.raw_count(), 11);
    assert_eq!(page.next_start().unwrap(), Some(31));
    assert_eq!(page.auction_snapshots().count(), 1);
    assert_eq!(page.opening_matches().count(), 1);
    assert_eq!(page.after_hours_trades().count(), 2);
    assert_eq!(page.actual_trades().count(), 10);
    assert_eq!(page.records[1].reserved_raw, 7);
    assert_eq!(page.records[9].reserved_raw, -1);
    assert_eq!(page.records[9].side(), EnhancedTradeSide::Status(99));
    assert_eq!(page.records[10].side(), EnhancedTradeSide::Status(-1));
    assert_eq!(page.records[1].side(), EnhancedTradeSide::Buy);
    assert_eq!(page.records[2].side(), EnhancedTradeSide::Sell);
    assert_eq!(page.records[3].side(), EnhancedTradeSide::Neutral);
    for (index, record) in page.records.iter().enumerate() {
        assert_eq!(usize::from(record.index), index);
        assert_eq!(record.absolute_index, 20 + index as u32);
        assert_eq!(record.time_minutes, rows[index].0);
    }
    assert_eq!(page.records[2].price, 9.95);
    // The page header reference is not a price offset.
    assert_eq!(page.records[3].price, 10.0);
}

#[test]
fn offsets_advance_by_raw_count_and_do_not_wrap() {
    let data = payload(0.0, &[(565, [0, 0, 0, 8, 0])]);
    let page = decode_enhanced_history_transactions(&request(0, 1800), &data).unwrap();
    assert_eq!(page.actual_trades().count(), 0);
    assert_eq!(page.next_start().unwrap(), Some(1));
    assert_eq!(page.records[0].price, 0.0);
    let page = decode_enhanced_history_transactions(&request(u16::MAX - 1, 1), &data).unwrap();
    assert_eq!(page.next_start().unwrap(), Some(u16::MAX));
    let page = decode_enhanced_history_transactions(&request(u16::MAX, 1), &data).unwrap();
    assert_eq!(
        page.next_start().unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    let data = payload(0.0, &[(570, [1, 1, 1, 0, 0]), (571, [0, 1, 1, 0, 0])]);
    let page = decode_enhanced_history_transactions(&request(u16::MAX, 2), &data).unwrap();
    assert_eq!(page.records[1].absolute_index, 65536);
    assert!(page.next_start().is_err());
    let empty = decode_enhanced_history_transactions(&request(u16::MAX, 1), &[0; 6]).unwrap();
    assert_eq!(empty.next_start().unwrap(), None);
    assert_eq!(empty.raw_count(), 0);
}

#[test]
fn invalid_parameters_fail_before_io() {
    for code in ["", "12345", "0000010", "中文", "0000/1"] {
        assert!(EnhancedHistoryTransactionRequest::new(0, code, 0, 1, 20261022).is_err());
    }
    for market in [2, u16::MAX] {
        assert!(EnhancedHistoryTransactionRequest::new(market, "000001", 0, 1, 20261022).is_err());
    }
    for count in [0, 1801, 2000, u16::MAX] {
        assert!(EnhancedHistoryTransactionRequest::new(0, "000001", 0, count, 20261022).is_err());
    }
    for date in [0, 101, 20260229, 20260230, 100000101, u32::MAX] {
        assert!(EnhancedHistoryTransactionRequest::new(0, "000001", 0, 1, date).is_err());
    }
    let checked =
        EnhancedHistoryTransactionRequest::new(1, "600000", u16::MAX, 1800, 20240229).unwrap();
    assert_eq!(
        (
            checked.market(),
            checked.code(),
            checked.start(),
            checked.count(),
            checked.date()
        ),
        (1, "600000", u16::MAX, 1800, 20240229)
    );
}

#[test]
fn legacy_and_enhanced_layouts_cannot_be_interchanged() {
    // Legacy layout: time + four varints, with no order-count slot.
    let legacy = [1, 0, 0, 0, 0, 0, 0x3a, 2, 1, 2, 5, 0];
    let legacy_request = HistoryTransactionRequest::new(0, "000001", 0, 1, 20261022).unwrap();
    assert_eq!(
        decode_history_transactions(&legacy_request, &legacy).unwrap()[0].num,
        0
    );
    assert!(decode_enhanced_history_transactions(&request(0, 1), &legacy).is_err());
    let enhanced = payload(0.0, &[(570, [1, 2, 3, 5, 0])]);
    assert!(decode_history_transactions(&legacy_request, &enhanced).is_err());
    assert_eq!(&legacy_request.encode()[10..12], &[0xb5, 0x0f]);
    assert_eq!(&request(0, 1).encode()[10..12], &[0xc6, 0x0f]);
}

#[test]
fn malformed_payloads_fail_the_whole_page() {
    let checked = request(0, 2);
    let valid = payload(
        35.5,
        &[(570, [1000, 100, 10, 0, 0]), (571, [-1, 10, 1, 1, 0])],
    );
    for end in 0..valid.len() {
        assert!(
            decode_enhanced_history_transactions(&checked, &valid[..end]).is_err(),
            "prefix {end}"
        );
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(decode_enhanced_history_transactions(&checked, &trailing).is_err());
    assert!(decode_enhanced_history_transactions(&request(0, 1), &valid).is_err());
    for base in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(decode_enhanced_history_transactions(&checked, &payload(base, &[])).is_err());
    }
    for (minutes, fields) in [
        (1440, [1, 1, 1, 0, 0]),
        (570, [-1, 1, 1, 0, 0]),
        (570, [1, -1, 1, 0, 0]),
        (570, [1, 1, -1, 0, 0]),
    ] {
        assert!(
            decode_enhanced_history_transactions(&checked, &payload(0.0, &[(minutes, fields)]))
                .is_err()
        );
    }
    let mut overlong_varint = payload(0.0, &[]);
    overlong_varint[..2].copy_from_slice(&1u16.to_le_bytes());
    overlong_varint.extend_from_slice(&[0x3a, 2, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
    assert!(decode_enhanced_history_transactions(&checked, &overlong_varint).is_err());
    assert!(decode_enhanced_history_transactions(&checked, &vec![0; 65536]).is_err());
    // Wide accumulation is important even when each delta fits in i32.
    let wide = payload(
        0.0,
        &[(570, [i32::MAX, 1, 1, 0, 0]), (571, [i32::MAX, 1, 1, 0, 0])],
    );
    assert_eq!(
        decode_enhanced_history_transactions(&checked, &wide)
            .unwrap()
            .records[1]
            .price_acc_raw,
        4294967294
    );
    let full = payload(0.0, &vec![(570, [1, 1, 1, 0, 0]); 1800]);
    assert_eq!(
        decode_enhanced_history_transactions(&request(0, 1800), &full)
            .unwrap()
            .raw_count(),
        1800
    );
}

#[test]
fn offline_decoding_honors_cancellation() {
    let operation = OperationContext::new(Duration::from_secs(2), None).unwrap();
    let _guard = operation.enter();
    operation.cancel();
    assert_eq!(
        decode_enhanced_history_transactions(&request(0, 1), &[0; 6])
            .unwrap_err()
            .kind(),
        ErrorKind::Interrupted
    );
}

#[test]
fn real_frames_match_every_independently_decoded_field() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/testing/evidence/enhanced-history-0fc6-20261024");
    for (start, count, snapshots, opening, after_hours) in [
        (0, 1800, 19, 0, 45),
        (1800, 1800, 0, 0, 0),
        (3600, 1276, 68, 1, 0),
        (4876, 0, 0, 0, 0),
    ] {
        let directory = root.join(format!("live-start-{start}"));
        let read = |name| std::fs::read(directory.join(name)).unwrap();
        let checked =
            EnhancedHistoryTransactionRequest::new(0, "300308", start, 1800, 20261023).unwrap();
        assert_eq!(checked.encode().as_slice(), read("frame-01.request.bin"));
        let header: [u8; 16] = read("frame-01.header.bin").try_into().unwrap();
        let bytes =
            rustdx_complete::tcp::decode_frame(&header, &read("frame-01.wire.bin"), true).unwrap();
        assert_eq!(bytes, read("frame-01.payload.bin"));
        let page = decode_enhanced_history_transactions(&checked, &bytes).unwrap();
        let expected: serde_json::Value =
            serde_json::from_slice(&read("expected-records.json")).unwrap();
        assert_eq!(serde_json::to_value(&page.records).unwrap(), expected);
        assert_eq!(page.price_base_raw_bytes.as_slice(), &bytes[2..6]);
        assert_eq!(page.price_base_raw_f32, 590.11_f32);
        assert_eq!(page.raw_count(), count);
        assert_eq!(page.auction_snapshots().count(), snapshots);
        assert_eq!(page.opening_matches().count(), opening);
        assert_eq!(page.after_hours_trades().count(), after_hours);
        assert_eq!(page.actual_trades().count(), count - snapshots);
        assert_eq!(page.source_endpoint, None);
        if start == 3600 {
            let record = page.opening_matches().next().unwrap();
            assert_eq!(
                (record.time.as_str(), record.price, record.status_raw),
                ("09:25", 572.01, 2)
            );
            assert_eq!((record.volume, record.order_count), (8871, 1681));
            // A short nonempty page is still pageable; actual count drives it.
            assert_eq!(page.next_start().unwrap(), Some(4876));
        } else if count == 0 {
            assert_eq!(page.next_start().unwrap(), None);
        }
    }
}

#[test]
fn client_replays_compressed_empty_and_legacy_pages_without_fallback() {
    use rustdx_complete::tcp::{PACK1, TcpConfig, stock::Client};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
    };

    fn frame(stream: &mut TcpStream, request: &[u8], body: &[u8], compressed: bool) {
        let wire = if compressed {
            miniz_oxide::deflate::compress_to_vec_zlib(body, 6)
        } else {
            body.to_vec()
        };
        let mut header = [0; 16];
        header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
        header[5..9].copy_from_slice(&request[1..5]);
        header[10..12].copy_from_slice(&request[10..12]);
        header[12..14].copy_from_slice(&(wire.len() as u16).to_le_bytes());
        header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
        // Fragment the frame to exercise read_exact and decompression.
        for part in header.chunks(3) {
            stream.write_all(part).unwrap();
        }
        for part in wire.chunks(3) {
            stream.write_all(part).unwrap();
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };
    let valid = payload(11.3, &vec![(915, [1130, 1, 1, 5, 0]); 20]);
    let expected = valid.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut handshake = vec![0; PACK1.len()];
        stream.read_exact(&mut handshake).unwrap();
        assert_eq!(handshake, PACK1);
        frame(&mut stream, &handshake, &[0, 0], false);
        for (encoded, body, compressed) in [
            (request(0, 20).encode(), valid, true),
            (request(20, 20).encode(), vec![0; 6], false),
            (request(0, 1).encode(), vec![1, 0], false), // malformed 0fc6, no fallback.
            (
                HistoryTransactionRequest::new(0, "000001", 0, 1, 20261022)
                    .unwrap()
                    .encode(),
                vec![1, 0, 0, 0, 0, 0, 0x3a, 2, 1, 2, 5, 0],
                false,
            ),
        ] {
            let mut received = [0; 28];
            stream.read_exact(&mut received).unwrap();
            assert_eq!(received, encoded);
            frame(&mut stream, &received, &body, compressed);
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    // These must not emit requests; the server expects the first valid request.
    assert!(
        client
            .enhanced_history_transaction(0, "000001", 0, 1801, 20261022)
            .is_err()
    );
    assert!(
        client
            .enhanced_history_transaction(0, "000001", 0, 1, 20260230)
            .is_err()
    );
    let page = client
        .enhanced_history_transaction(0, "000001", 0, 20, 20261022)
        .unwrap();
    assert_eq!(page.raw_payload, expected);
    assert_eq!(page.source_endpoint, config.ip);
    assert!(page.fetched_at.is_some());
    assert_eq!(page.after_hours_trades().count(), 20);
    let empty = client
        .enhanced_history_transaction(0, "000001", 20, 20, 20261022)
        .unwrap();
    assert_eq!(empty.raw_count(), 0);
    let error = client
        .enhanced_history_transaction(0, "000001", 0, 1, 20261022)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Client::enhanced_history_transaction")
    );
    assert_eq!(
        client
            .history_transaction_legacy(0, "000001", 0, 1, 20261022)
            .unwrap()[0]
            .num,
        0
    );
    server.join().unwrap();
}

proptest::proptest! {
    #[test]
    fn arbitrary_payloads_are_bounded_and_never_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..4096)) {
        if let Ok(page) = decode_enhanced_history_transactions(&request(0, 1800), &bytes) {
            proptest::prop_assert!(page.raw_count() <= 1800);
            proptest::prop_assert_eq!(page.raw_payload, bytes);
        }
    }
}
