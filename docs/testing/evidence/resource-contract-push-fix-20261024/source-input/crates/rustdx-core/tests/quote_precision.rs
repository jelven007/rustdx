//! Synthetic wire cases exercise arithmetic; real samples are labelled separately.
use rustdx_complete::{
    operation::OperationContext,
    tcp::{
        PACK1, TcpConfig, Tdx,
        stock::{
            Client, PrecisionSnapshot, QuoteMetadataStore, QuotesRequest, SecurityQuotes,
            decode_quotes, decode_quotes_legacy, decode_quotes_with_metadata,
        },
    },
};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

fn precision(entries: &[(u16, &str, u8)]) -> PrecisionSnapshot {
    PrecisionSnapshot::from_decimals(entries, chrono::Utc::now(), None).unwrap()
}

fn varint(bytes: &mut Vec<u8>, value: i32) {
    let mut magnitude = i64::from(value).unsigned_abs();
    let mut first = (magnitude & 0x3f) as u8 | if value < 0 { 0x40 } else { 0 };
    magnitude >>= 6;
    if magnitude != 0 {
        first |= 0x80;
    }
    bytes.push(first);
    while magnitude != 0 {
        let mut byte = (magnitude & 0x7f) as u8;
        magnitude >>= 7;
        if magnitude != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
    }
}

fn quote(market: u16, code: &str, base: i32, delta: i32) -> Vec<u8> {
    let mut bytes = vec![market as u8];
    bytes.extend(code.as_bytes());
    bytes.extend(0u16.to_le_bytes());
    for value in [base, delta, -10, 20, -20, 11298811, -base, 123, 4] {
        varint(&mut bytes, value);
    }
    bytes.extend(0x4e16310d_u32.to_le_bytes()); // independent amount encoding
    for value in [50, 73, 0, 0] {
        varint(&mut bytes, value);
    }
    for level in 0..5 {
        // Fifth bid is a legitimate absent level (absolute price zero).
        for value in [
            if level == 4 { -base } else { -level },
            level + 1,
            10 + level,
            20 + level,
        ] {
            varint(&mut bytes, value);
        }
    }
    bytes.extend(0u16.to_le_bytes());
    for _ in 0..4 {
        varint(&mut bytes, 0);
    }
    bytes.extend((-123_i16).to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes
}

fn response(rows: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![1, 20];
    bytes.extend((rows.len() as u16).to_le_bytes());
    for row in rows {
        bytes.extend(row);
    }
    bytes
}

#[test]
fn mixed_decimals_scale_every_price_once_and_preserve_nonprice_fields() {
    let request = QuotesRequest::new(vec![(0, "000001"), (1, "000001"), (1, "510300")]).unwrap();
    let snapshot = precision(&[(0, "000001", 2), (1, "000001", 3), (1, "510300", 4)]);
    let bytes = response(&[
        quote(0, "000001", 12345, -5),
        quote(1, "000001", 12345, -5),
        quote(1, "510300", 12345, -5),
    ]);
    let page = decode_quotes_with_metadata(&request, &bytes, &snapshot).unwrap();
    assert!(page.missing_codes.is_empty());
    assert_eq!(
        page.precision.iter().map(|p| p.decimal).collect::<Vec<_>>(),
        [2, 3, 4]
    );
    for (row, divisor) in page.records.iter().zip([100.0, 1000.0, 10000.0]) {
        assert_eq!(row.price, 12345.0 / divisor);
        assert_eq!(row.last_close, 12340.0 / divisor);
        assert_eq!(row.open, 12335.0 / divisor);
        assert_eq!(row.high, 12365.0 / divisor);
        assert_eq!(row.low, 12325.0 / divisor);
        for (level, bid) in [row.bid1, row.bid2, row.bid3, row.bid4]
            .into_iter()
            .enumerate()
        {
            assert_eq!(bid, (12345.0 - level as f64) / divisor);
        }
        assert_eq!(row.bid5, 0.0);
        for (level, ask) in [row.ask1, row.ask2, row.ask3, row.ask4, row.ask5]
            .into_iter()
            .enumerate()
        {
            assert_eq!(ask, (12346.0 + level as f64) / divisor);
        }
        assert!((row.change - 5.0 / divisor).abs() < 1e-12);
        assert!((row.change_percent - 5.0 / 12340.0 * 100.0).abs() < 1e-10);
        assert_eq!(
            (row.vol, row.cur_vol, row.s_vol, row.b_vol),
            (123.0, 4.0, 50.0, 73.0)
        );
        assert_eq!((row.bid1_vol, row.ask5_vol), (10.0, 24.0));
        assert_eq!(row.speed, -1.23);
        assert_eq!(row.servertime, "11:29:52.866");
        assert_eq!(row.amount, page.records[0].amount);
    }
}

#[test]
fn wide_integer_sums_zero_prices_and_missing_identity_are_preserved() {
    let request = QuotesRequest::new(vec![(0, "000001"), (1, "000001")]).unwrap();
    let snapshot = precision(&[(0, "000001", 4), (1, "000001", 2)]);
    let bytes = response(&[quote(0, "000001", i32::MAX, i32::MAX)]);
    let page = decode_quotes_with_metadata(&request, &bytes, &snapshot).unwrap();
    assert_eq!(page.records[0].last_close, 4294967294.0 / 10000.0);
    assert_eq!(page.missing_codes, [(1, "000001".into())]);
    assert_eq!(page.returned_codes, [(0, "000001".into())]);
    let mut zero = quote(0, "000001", 0, 0);
    // Build a completely zero valid record using the same layout.
    zero.clear();
    zero.extend([0]);
    zero.extend(b"000001");
    zero.extend([0; 2]);
    zero.extend([0; 9]);
    zero.extend([0; 4]);
    zero.extend([0; 24]);
    zero.extend([0; 2]);
    zero.extend([0; 8]);
    let rows = decode_quotes(&request, &response(&[zero]), &snapshot).unwrap();
    assert_eq!((rows[0].price, rows[0].bid1, rows[0].ask5), (0.0, 0.0, 0.0));
}

#[test]
fn missing_unsupported_duplicate_and_truncated_input_fail_without_fallback() {
    let request = QuotesRequest::new(vec![(0, "000001")]).unwrap();
    let bytes = response(&[quote(0, "000001", 12345, -5)]);
    assert_eq!(
        decode_quotes(&request, &bytes, &precision(&[(1, "000001", 2)]))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    for decimal in [0, 1, 5, 255] {
        assert_eq!(
            decode_quotes(&request, &bytes, &precision(&[(0, "000001", decimal)]))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
    }
    let snapshot = Arc::new(precision(&[(0, "000001", 3)]));
    let mut state = SecurityQuotes::default().with_precision(Arc::clone(&snapshot));
    for end in 0..bytes.len() {
        state.try_parse(bytes.clone()).unwrap();
        assert!(state.try_parse(bytes[..end].to_vec()).is_err());
        assert!(state.data.is_empty());
    }
    assert!(
        SecurityQuotes::default()
            .try_parse(bytes.clone())
            .unwrap_err()
            .to_string()
            .contains("MissingPrecision")
    );
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode_quotes(&request, &extra, &snapshot).is_err());
    let duplicate_request = QuotesRequest::new(vec![(0, "000001"), (1, "000001")]).unwrap();
    assert!(
        decode_quotes(
            &duplicate_request,
            &response(&[quote(0, "000001", 12345, 0), quote(0, "000001", 12345, 0)]),
            &snapshot
        )
        .is_err()
    );
}

#[test]
fn preserved_real_two_decimal_sample_matches_all_historical_fields() {
    let hex = include_str!("fixtures/protocol/quotes.response.hex").trim();
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    let request = QuotesRequest::new(vec![(0, "000001"), (1, "600000")]).unwrap();
    let metadata = precision(&[(0, "000001", 2), (1, "600000", 2)]);
    assert_eq!(
        serde_json::to_value(decode_quotes(&request, &bytes, &metadata).unwrap()).unwrap(),
        serde_json::to_value(decode_quotes_legacy(&request, &bytes).unwrap()).unwrap()
    );
    // Explicit offline replay accepts the capture's day, independent of wall clock.
    let historical = PrecisionSnapshot::from_decimals(
        &[(0, "000001", 2), (1, "600000", 2)],
        chrono::DateTime::parse_from_rfc3339("2026-10-22T23:31:59Z")
            .unwrap()
            .to_utc(),
        None,
    )
    .unwrap();
    assert_eq!(
        decode_quotes(&request, &bytes, &historical).unwrap().len(),
        2
    );
}

fn frame(stream: &mut TcpStream, request: &[u8], body: &[u8]) -> io::Result<()> {
    let mut header = [0; 16];
    header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
    header[5..9].copy_from_slice(&request[1..5]);
    header[10..12].copy_from_slice(&request[10..12]);
    header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
    header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(body)
}

fn server(
    reply: impl FnMut(&[u8]) -> Vec<u8> + Send + 'static,
) -> (TcpConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(1),
        ..Default::default()
    };
    let handle = std::thread::spawn(move || {
        let mut reply = reply;
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut handshake = vec![0; PACK1.len()];
        stream.read_exact(&mut handshake).unwrap();
        assert_eq!(handshake, PACK1);
        frame(&mut stream, &handshake, &[0, 0]).unwrap();
        loop {
            let mut header = [0; 10];
            if stream.read_exact(&mut header).is_err() {
                break;
            }
            let length = u16::from_le_bytes([header[6], header[7]]) as usize + 10;
            let mut request = header.to_vec();
            request.resize(length, 0);
            if stream.read_exact(&mut request[10..]).is_err() {
                break;
            }
            if frame(&mut stream, &request, &reply(&request)).is_err() {
                break;
            }
        }
    });
    (config, handle)
}

fn directory(entries: &[(&str, u8)]) -> Vec<u8> {
    let mut body = (entries.len() as u16).to_le_bytes().to_vec();
    for (code, decimal) in entries {
        let mut row = [0; 37];
        row[..6].copy_from_slice(code.as_bytes());
        row[6..8].copy_from_slice(&100u16.to_le_bytes());
        row[28] = *decimal;
        body.extend(row);
    }
    body
}

#[test]
fn client_cold_load_hot_cache_missing_precision_and_failed_refresh() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let fail = Arc::new(AtomicUsize::new(0));
    let (config, handle) = server({
        let calls = Arc::clone(&calls);
        let fail = Arc::clone(&fail);
        move |request| {
            let command = u16::from_le_bytes([request[10], request[11]]);
            calls.lock().unwrap().push(command);
            match command {
                0x044e => vec![2, 0],
                0x044d => {
                    if fail.load(Ordering::Relaxed) == 1 {
                        directory(&[])
                    } else {
                        directory(&[("000001", 3), ("000002", 0)])
                    }
                }
                0x053e => response(&[quote(0, "000001", 12345, -5)]),
                _ => panic!("unexpected command {command:x}"),
            }
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    assert!(client.quote_metadata_status().unwrap().is_empty());
    let page = client.quotes_with_metadata(&[(0, "000001")]).unwrap();
    assert_eq!(page.records[0].price, 12.345);
    assert_eq!(page.metadata_generation, 1);
    assert_eq!(page.source_endpoint, config.ip);
    assert_eq!(page.precision[0].source_endpoint, config.ip);
    assert_eq!(client.quote_metadata_status().unwrap()[0].unsupported, 1);
    for _ in 0..2 {
        assert_eq!(client.quotes(&[(0, "000001")]).unwrap()[0].price, 12.345);
        assert!(
            client
                .quotes(&[(0, "000003")])
                .unwrap_err()
                .to_string()
                .contains("MissingPrecision")
        );
        assert!(
            client
                .quotes(&[(0, "000002")])
                .unwrap_err()
                .to_string()
                .contains("UnsupportedPrecision")
        );
    }
    assert_eq!(
        *calls.lock().unwrap(),
        [0x044e, 0x044d, 0x053e, 0x053e, 0x053e]
    );
    fail.store(1, Ordering::Relaxed);
    assert!(client.refresh_quote_metadata(&[0]).is_err());
    assert_eq!(
        client
            .quote_metadata_store()
            .snapshot()
            .unwrap()
            .generation(),
        1
    );
    assert_eq!(client.quotes(&[(0, "000001")]).unwrap()[0].price, 12.345);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn multi_market_refresh_does_not_publish_successful_half_on_second_market_failure() {
    let (config, handle) = server(|request| match request[10] {
        0x4e => vec![1, 0],
        0x4d => {
            if request[12] == 0 {
                directory(&[("000001", 4)])
            } else {
                directory(&[])
            }
        }
        _ => panic!("unexpected request"),
    });
    let mut client = Client::with_config(&config).unwrap();
    let store = client.quote_metadata_store();
    store.replace(precision(&[(0, "000001", 2)])).unwrap();
    assert!(client.refresh_quote_metadata(&[0, 1]).is_err());
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.generation(), 1);
    assert_eq!(snapshot.scale(0, "000001").unwrap().decimal(), 2);
    assert_eq!(snapshot.status().len(), 1);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn nested_operation_deadline_bounds_cold_loading_and_does_not_publish_partial_cache() {
    let (config, handle) = server(|request| {
        assert_eq!(request[10], 0x4e);
        std::thread::sleep(Duration::from_millis(120));
        vec![1, 0]
    });
    let mut client = Client::with_config(&config).unwrap();
    let context = OperationContext::new(Duration::from_millis(40), None).unwrap();
    let start = Instant::now();
    let guard = context.enter();
    assert!(client.quotes(&[(0, "000001")]).is_err());
    assert!(start.elapsed() < Duration::from_millis(300));
    assert_eq!(
        client
            .quote_metadata_store()
            .snapshot()
            .unwrap()
            .generation(),
        0
    );
    drop(guard);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn immutable_generations_and_exchange_day_freshness_do_not_mix() {
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-24T15:59:59Z")
        .unwrap()
        .to_utc();
    let store = Arc::new(QuoteMetadataStore::default());
    let old = store
        .replace(
            PrecisionSnapshot::from_decimals(&[(0, "000001", 2), (1, "000001", 3)], at, None)
                .unwrap(),
        )
        .unwrap();
    assert!(old.is_fresh(0, at));
    assert!(!old.is_fresh(0, at - chrono::Duration::seconds(1)));
    assert!(!old.is_fresh(0, at + chrono::Duration::seconds(1))); // midnight Shanghai
    let worker = {
        let store = Arc::clone(&store);
        std::thread::spawn(move || {
            for _ in 0..50 {
                store
                    .replace(precision(&[(0, "000001", 4), (1, "000001", 4)]))
                    .unwrap();
            }
        })
    };
    let request = QuotesRequest::new(vec![(0, "000001"), (1, "000001")]).unwrap();
    for _ in 0..50 {
        let page = decode_quotes_with_metadata(
            &request,
            &response(&[quote(0, "000001", 12345, 0), quote(1, "000001", 12345, 0)]),
            &old,
        )
        .unwrap();
        assert_eq!(page.metadata_generation, 1);
        assert_eq!(
            (page.records[0].price, page.records[1].price),
            (123.45, 12.345)
        );
    }
    worker.join().unwrap();
    assert_eq!(store.snapshot().unwrap().generation(), 51);
    assert_eq!(
        QuoteMetadataStore::default()
            .snapshot()
            .unwrap()
            .generation(),
        0
    );
    assert!(
        PrecisionSnapshot::from_decimals(&[(0, "000001", 2), (0, "000001", 3)], at, None).is_err()
    );
}

#[test]
fn live_mixed_precision_frame_matches_independent_closed_session_prices() {
    let payload = include_bytes!("fixtures/protocol/quotes.precision.payload.bin");
    let entries = [
        (0, "000001", 2),
        (0, "159915", 3),
        (1, "510300", 3),
        (0, "101014", 4),
        (0, "101105", 4),
        (0, "101123", 4),
        (0, "148884", 4),
        (0, "127118", 4),
        (0, "127119", 4),
        (1, "018003", 4),
        (1, "018009", 4),
        (1, "018012", 4),
        (1, "240561", 4),
        (1, "118077", 4),
        (1, "118078", 4),
    ];
    let request = QuotesRequest::new(entries.iter().map(|(m, c, _)| (*m, *c)).collect()).unwrap();
    assert_eq!(
        request.encode(),
        include_bytes!("fixtures/protocol/quotes.precision.request.bin")
    );
    let page = decode_quotes_with_metadata(&request, payload, &precision(&entries)).unwrap();
    assert_eq!(page.records.len(), 15);
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/protocol/quotes.precision.expected.json"
    ))
    .unwrap();
    for row in expected["rows"].as_array().unwrap() {
        let index = page
            .returned_codes
            .iter()
            .position(|(market, code)| {
                Some(u64::from(*market)) == row["market"].as_u64()
                    && Some(code.as_str()) == row["code"].as_str()
            })
            .unwrap();
        let quote = &page.records[index];
        assert_eq!(
            u64::from(page.precision[index].decimal),
            row["decimal"].as_u64().unwrap()
        );
        for (actual, field) in [
            (
                [
                    quote.price,
                    quote.last_close,
                    quote.open,
                    quote.high,
                    quote.low,
                ],
                "ohlc",
            ),
            (
                [quote.bid1, quote.bid2, quote.bid3, quote.bid4, quote.bid5],
                "bids",
            ),
            (
                [quote.ask1, quote.ask2, quote.ask3, quote.ask4, quote.ask5],
                "asks",
            ),
        ] {
            for (value, expected) in actual.into_iter().zip(row[field].as_array().unwrap()) {
                assert!(
                    (value - expected.as_f64().unwrap()).abs()
                        <= row["tolerance"].as_f64().unwrap(),
                    "{} {field}: {value} != {expected}",
                    quote.code
                );
            }
        }
    }
    // The four-decimal active bond has actual trades; the other has 0.0001
    // resolution but no trades and a rounded independent display.
    assert_eq!(page.records[9].vol, 500.0);
    assert_eq!(page.records[5].price, 121.4613);
}

#[test]
fn live_request_rejects_stale_future_and_missing_precision_before_sending() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (config, handle) = server({
        let calls = Arc::clone(&calls);
        move |_| {
            calls.fetch_add(1, Ordering::Relaxed);
            vec![]
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    assert_eq!(
        client.refresh_quote_metadata(&[0, 2]).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let request = QuotesRequest::new(vec![(0, "000001")]).unwrap();
    for timestamp in [
        chrono::Utc::now() - chrono::Duration::days(1),
        chrono::Utc::now() + chrono::Duration::hours(1),
    ] {
        let snapshot =
            PrecisionSnapshot::from_decimals(&[(0, "000001", 2)], timestamp, None).unwrap();
        assert!(
            request
                .recv(&mut client.tcp, &snapshot)
                .unwrap_err()
                .to_string()
                .contains("StalePrecision")
        );
    }
    assert!(
        request
            .recv(&mut client.tcp, &precision(&[(0, "000002", 2)]))
            .unwrap_err()
            .to_string()
            .contains("MissingPrecision")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    drop(client);
    handle.join().unwrap();
}
