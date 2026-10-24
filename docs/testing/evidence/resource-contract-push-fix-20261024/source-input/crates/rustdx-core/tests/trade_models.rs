use rustdx_complete::tcp::{
    PACK1, TcpConfig,
    stock::{
        Client, HistoryTransactionRequest, TradeCollection, TradeCoverage, TradeDirection,
        TradeEventKind, TradePage, TradePhase, TradeProtocol, TradeQualification, TradeQuery,
        TradeTermination, TradeWarning, TransactionRequest, decode_history_transactions,
        decode_trade_page, decode_transactions,
    },
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

// Signed TDX encoding, including negative price deltas / unknown raw statuses.
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

// minutes, price delta, raw volume, count slot, status, unknown tail
type Raw = (u16, i32, i32, i32, i32, i32);

fn payload(rows: &[Raw], historical: bool) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    if historical {
        bytes.extend_from_slice(&[0x5c, 0x8f, 0x36, 0x41]);
    }
    for &(time, delta, volume, count, status, tail) in rows {
        bytes.extend_from_slice(&time.to_le_bytes());
        for value in [delta, volume] {
            varint(value, &mut bytes);
        }
        if !historical {
            varint(count, &mut bytes);
        }
        for value in [status, tail] {
            varint(value, &mut bytes);
        }
    }
    bytes
}

fn page(query: &TradeQuery, start: u16, rows: &[Raw]) -> TradePage {
    decode_trade_page(query, start, 1800, &payload(rows, query.date().is_some())).unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/protocol")
        .join(format!("{name}.response.hex"));
    let hex = std::fs::read_to_string(path).unwrap();
    hex.trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn classification_prioritizes_status_and_preserves_unqualified_events() {
    let query = TradeQuery::today(0, "000001").unwrap();
    let cases = [
        (
            565,
            8,
            TradeEventKind::AuctionSnapshot,
            TradePhase::AuctionProcess,
            TradeDirection::Unknown,
            TradeQualification::ExcludedByProfile,
        ),
        (
            565,
            0,
            TradeEventKind::OpeningMatch,
            TradePhase::Opening,
            TradeDirection::Buy,
            TradeQualification::ConfirmedByProfile,
        ),
        (
            565,
            99,
            TradeEventKind::OpeningMatch,
            TradePhase::Opening,
            TradeDirection::Unknown,
            TradeQualification::Unknown,
        ),
        (
            900,
            8,
            TradeEventKind::AuctionSnapshot,
            TradePhase::AuctionProcess,
            TradeDirection::Unknown,
            TradeQualification::ExcludedByProfile,
        ),
        (
            900,
            1,
            TradeEventKind::Trade,
            TradePhase::ClosingMatch,
            TradeDirection::Sell,
            TradeQualification::ConfirmedByProfile,
        ),
        (
            905,
            5,
            TradeEventKind::Trade,
            TradePhase::AfterHoursFixedPrice,
            TradeDirection::Unknown,
            TradeQualification::ConfirmedByProfile,
        ),
        (
            930,
            5,
            TradeEventKind::Trade,
            TradePhase::AfterHoursFixedPrice,
            TradeDirection::Unknown,
            TradeQualification::ConfirmedByProfile,
        ),
        (
            904,
            5,
            TradeEventKind::Unknown,
            TradePhase::Unknown,
            TradeDirection::Unknown,
            TradeQualification::Candidate,
        ),
        (
            931,
            5,
            TradeEventKind::Unknown,
            TradePhase::Unknown,
            TradeDirection::Unknown,
            TradeQualification::Candidate,
        ),
        (
            600,
            2,
            TradeEventKind::Trade,
            TradePhase::Regular,
            TradeDirection::Neutral,
            TradeQualification::ConfirmedByProfile,
        ),
        (
            720,
            0,
            TradeEventKind::Trade,
            TradePhase::Unknown,
            TradeDirection::Buy,
            TradeQualification::Candidate,
        ),
        (
            600,
            -7,
            TradeEventKind::Unknown,
            TradePhase::Unknown,
            TradeDirection::Unknown,
            TradeQualification::Unknown,
        ),
    ];
    for (time, status, kind, phase, direction, qualification) in cases {
        let p = page(&query, 0, &[(time, 1130, 10, 2, status, -9)]);
        let record = &p.records()[0];
        assert_eq!(
            (
                record.event_kind(),
                record.phase(),
                record.direction(),
                record.qualification()
            ),
            (kind, phase, direction, qualification),
            "status={status} time={time}"
        );
        assert_eq!(record.status_raw(), status);
        assert_eq!(record.tail_raw(), -9);
        assert_eq!(
            record.is_confirmed_trade(),
            qualification == TradeQualification::ConfirmedByProfile
        );
    }
    let history = TradeQuery::history_legacy(0, "000001", 20261022).unwrap();
    let p = page(&history, 0, &[(905, 1130, 10, 2, 5, 0)]);
    assert_eq!(
        p.records()[0].qualification(),
        TradeQualification::Candidate
    );
    assert_eq!(p.confirmed_trades().count(), 0);
}

#[test]
fn real_fixtures_keep_raw_fields_and_legacy_output() {
    for historical in [false, true] {
        let query = if historical {
            TradeQuery::history_legacy(0, "000001", 20261022).unwrap()
        } else {
            TradeQuery::today(0, "000001").unwrap()
        };
        let bytes = fixture(if historical {
            "history-transaction"
        } else {
            "transaction"
        });
        let p = decode_trade_page(&query, 0, 3, &bytes).unwrap();
        assert_eq!(
            p.query().protocol(),
            if historical {
                TradeProtocol::LegacyHistory0fb5
            } else {
                TradeProtocol::Today0fc5
            }
        );
        assert_eq!(p.raw_count(), 3);
        assert_eq!(p.payload(), bytes);
        assert_eq!(p.records().iter().map(|r| r.volume_raw()).sum::<i32>(), 132);
        assert_eq!(
            p.records()
                .iter()
                .map(|r| r.price_delta_raw())
                .collect::<Vec<_>>(),
            [1130, 0, 0]
        );
        assert!(
            p.records()
                .iter()
                .all(|r| r.price_raw() == 1130 && r.status_raw() == 5 && r.tail_raw() == 0)
        );
        assert!(
            p.records()
                .iter()
                .all(|r| r.phase() == TradePhase::AfterHoursFixedPrice
                    && r.direction() == TradeDirection::Unknown)
        );
        assert_eq!(
            p.historical_prefix_raw(),
            historical.then_some([0x5c, 0x8f, 0x36, 0x41])
        );
        let counts = p
            .records()
            .iter()
            .map(|r| r.trade_count_raw())
            .collect::<Vec<_>>();
        assert_eq!(
            counts,
            if historical {
                vec![None; 3]
            } else {
                vec![Some(10), Some(1), Some(1)]
            }
        );
        let legacy = if historical {
            decode_history_transactions(
                &HistoryTransactionRequest::new(0, "000001", 0, 3, 20261022).unwrap(),
                &bytes,
            )
            .unwrap()
        } else {
            decode_transactions(&TransactionRequest::new(0, "000001", 0, 3).unwrap(), &bytes)
                .unwrap()
        };
        let converted: Vec<_> = p.records().iter().map(|r| r.to_legacy()).collect();
        assert_eq!(
            serde_json::to_value(converted).unwrap(),
            serde_json::to_value(legacy).unwrap()
        );
        let mut reconstructed = bytes[..if historical { 6 } else { 2 }].to_vec();
        for index in 0..3 {
            reconstructed.extend_from_slice(p.record_bytes(index).unwrap());
        }
        assert_eq!(reconstructed, bytes);
        assert!(p.record_bytes(3).is_none());
        assert!(p.source().is_none());
    }
}

#[test]
fn raw_decoder_is_atomic_and_offsets_are_checked() {
    for query in [
        TradeQuery::today(0, "000001").unwrap(),
        TradeQuery::history_legacy(0, "000001", 20261022).unwrap(),
    ] {
        let bytes = payload(
            &[(600, 1130, 1, 1, 99, -1), (600, -10, 2, 1, 8, 0)],
            query.date().is_some(),
        );
        for end in 0..bytes.len() {
            assert!(
                decode_trade_page(&query, 0, 2, &bytes[..end]).is_err(),
                "prefix={end}"
            );
        }
        let decoded = decode_trade_page(&query, u16::MAX, 2, &bytes).unwrap();
        assert_eq!(decoded.next_offset(), 65537);
        assert_eq!(decoded.records()[1].price_raw(), 1120);
        assert!(u16::try_from(decoded.next_offset()).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_trade_page(&query, 0, 2, &trailing).is_err());
        assert!(decode_trade_page(&query, 0, 1, &bytes).is_err());
        for count in [0, 2001] {
            assert!(decode_trade_page(&query, 0, count, &bytes).is_err());
        }
        for row in [
            (1440, 1, 1, 1, 0, 0),
            (600, 0, 1, 1, 0, 0),
            (600, 1, -1, 1, 0, 0),
        ] {
            assert!(
                decode_trade_page(&query, 0, 1, &payload(&[row], query.date().is_some())).is_err()
            );
        }
    }
    assert!(TradeQuery::today(2, "000001").is_err());
    assert!(TradeQuery::today(0, "0000/1").is_err());
    assert!(TradeQuery::history(0, "000001", 20260230).is_err());
}

#[test]
fn collection_advances_raw_count_and_preserves_identical_same_minute_rows() {
    let query = TradeQuery::today(0, "000001").unwrap();
    let auction = page(&query, 0, &[(565, 1130, 10, 1, 8, 0)]);
    assert_eq!(auction.confirmed_trades().count(), 0);
    assert_eq!(auction.next_offset(), 1); // Short and entirely excluded, but nonempty.
    let first = page(&query, 1, &[(600, 1130, 10, 1, 0, 0)]);
    let identical = page(&query, 2, &[(600, 1130, 10, 1, 0, 0)]);
    let empty = page(&query, 3, &[]);
    let collection = TradeCollection::new(
        query,
        vec![auction, first, identical, empty],
        TradeTermination::EmptyPageObserved,
    )
    .unwrap();
    assert_eq!(collection.counts().raw, 3);
    assert_eq!(collection.counts().auction_snapshots, 1);
    assert_eq!(collection.counts().confirmed, 2);
    assert_eq!(collection.confirmed_trades().count(), 2);
    assert_eq!(collection.next_offset(), 3);
    assert_eq!(collection.time_range(), Some((565, 600)));
    assert_eq!(collection.coverage(), TradeCoverage::Unknown);
    assert!(collection.warnings().contains(&TradeWarning::MissingSource));
    assert_eq!(
        collection.payload_bytes(),
        collection
            .pages()
            .iter()
            .map(|p| p.payload().len())
            .sum::<usize>()
    );
}

#[test]
fn collection_rejects_mixed_queries_gaps_and_false_termination() {
    let query = TradeQuery::history_legacy(0, "000001", 20261022).unwrap();
    let row = [(600, 1130, 10, 1, 0, 0)];
    let first = page(&query, 0, &row);
    for different in [
        TradeQuery::history_legacy(1, "000001", 20261022).unwrap(),
        TradeQuery::history_legacy(0, "000002", 20261022).unwrap(),
        TradeQuery::history_legacy(0, "000001", 20261023).unwrap(),
        TradeQuery::today(0, "000001").unwrap(),
    ] {
        assert!(
            TradeCollection::new(
                query.clone(),
                vec![first.clone(), page(&different, 1, &[])],
                TradeTermination::EmptyPageObserved
            )
            .is_err()
        );
    }
    for second in [page(&query, 0, &row), page(&query, 2, &row)] {
        assert!(
            TradeCollection::new(
                query.clone(),
                vec![first.clone(), second],
                TradeTermination::CallerStopped
            )
            .is_err()
        );
    }
    let empty = page(&query, 1, &[]);
    assert!(
        TradeCollection::new(
            query.clone(),
            vec![first.clone(), empty.clone(), page(&query, 1, &row)],
            TradeTermination::CallerStopped
        )
        .is_err()
    );
    assert!(
        TradeCollection::new(
            query.clone(),
            vec![first.clone()],
            TradeTermination::EmptyPageObserved
        )
        .is_err()
    );
    assert!(
        TradeCollection::new(
            query.clone(),
            vec![first.clone(), empty],
            TradeTermination::PageLimit
        )
        .is_err()
    );
    assert!(
        TradeCollection::new(query.clone(), vec![], TradeTermination::EmptyPageObserved).is_err()
    );
    assert!(
        TradeCollection::new(
            query.clone(),
            vec![first.clone()],
            TradeTermination::OffsetLimit
        )
        .is_err()
    );
    let different_size = decode_trade_page(&query, 1, 1, &payload(&row, true)).unwrap();
    assert!(
        TradeCollection::new(
            query,
            vec![first, different_size],
            TradeTermination::CallerStopped
        )
        .is_err()
    );
}

#[test]
fn every_budget_or_error_stop_retains_partial_coverage() {
    let query = TradeQuery::today(0, "000001").unwrap();
    for termination in [
        TradeTermination::PageLimit,
        TradeTermination::RecordLimit,
        TradeTermination::ByteLimit,
        TradeTermination::Deadline,
        TradeTermination::Cancelled,
        TradeTermination::ConnectionChanged,
        TradeTermination::DateChanged,
        TradeTermination::DecodeError {
            message: "truncated".into(),
        },
        TradeTermination::TransportError {
            message: "connection reset".into(),
        },
        TradeTermination::RepeatedPage,
        TradeTermination::CallerStopped,
    ] {
        let result = TradeCollection::new(
            query.clone(),
            vec![page(&query, 0, &[(905, 1130, 1, 1, 5, 0)])],
            termination.clone(),
        )
        .unwrap();
        assert_eq!(result.coverage(), TradeCoverage::Partial);
        assert_eq!(result.termination(), &termination);
        assert_eq!(result.counts().confirmed_unknown_direction, 1);
    }
    let result = TradeCollection::new(
        query.clone(),
        vec![page(&query, u16::MAX, &[(600, 1130, 1, 1, 0, 0)])],
        TradeTermination::OffsetLimit,
    )
    .unwrap();
    assert_eq!(result.next_offset(), 65536);
    assert_eq!(result.coverage(), TradeCoverage::Partial);
    let suffix = TradeCollection::new(
        query.clone(),
        vec![page(&query, 10, &[])],
        TradeTermination::EmptyPageObserved,
    )
    .unwrap();
    assert_eq!(suffix.coverage(), TradeCoverage::Partial);
}

fn frame(stream: &mut TcpStream, request: &[u8], body: &[u8]) {
    let mut header = [0; 16];
    header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
    header[5..9].copy_from_slice(&request[1..5]);
    header[10..12].copy_from_slice(&request[10..12]);
    header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
    header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
    stream.write_all(&header).unwrap();
    stream.write_all(body).unwrap();
}

fn respond(stream: &mut TcpStream, request: &[u8], body: &[u8]) {
    let mut received = vec![0; request.len()];
    stream.read_exact(&mut received).unwrap();
    assert_eq!(received, request);
    frame(stream, request, body);
}

#[test]
fn live_pages_capture_sources_and_detect_same_endpoint_reconnection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };
    let today = TradeQuery::today(0, "000001").unwrap();
    let history = TradeQuery::history_legacy(0, "000001", 20261022).unwrap();
    let server_today = today.clone();
    let server_history = history.clone();
    let server = std::thread::spawn(move || {
        for connection in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            respond(&mut stream, PACK1, &[0, 0]);
            if connection == 0 {
                respond(
                    &mut stream,
                    &server_today.encode(0, 3).unwrap(),
                    &fixture("transaction"),
                );
                respond(&mut stream, &server_today.encode(3, 3).unwrap(), &[0, 0]);
                respond(
                    &mut stream,
                    &server_history.encode(0, 3).unwrap(),
                    &fixture("history-transaction"),
                );
                respond(&mut stream, &server_history.encode(3, 3).unwrap(), &[0; 6]);
            } else {
                respond(&mut stream, &server_history.encode(3, 3).unwrap(), &[0; 6]);
            }
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    let initial_id = client.tcp.connection_id();
    let first = client.trade_page(0, "000001", 0, 3).unwrap();
    let end = client.trade_page(0, "000001", 3, 3).unwrap();
    assert_eq!(first.source().unwrap().endpoint(), config.ip.unwrap());
    assert_eq!(first.source().unwrap().connection_id(), initial_id);
    assert_eq!(
        first.source().unwrap().fetched_at().date_naive(),
        chrono::Utc::now().date_naive()
    );
    assert_eq!(
        TradeCollection::new(today, vec![first, end], TradeTermination::EmptyPageObserved)
            .unwrap()
            .coverage(),
        TradeCoverage::Unknown
    );
    let first = client
        .history_trade_page_legacy(0, "000001", 20261022, 0, 3)
        .unwrap();
    let end = client
        .history_trade_page_legacy(0, "000001", 20261022, 3, 3)
        .unwrap();
    let result = TradeCollection::new(
        history.clone(),
        vec![first.clone(), end],
        TradeTermination::EmptyPageObserved,
    )
    .unwrap();
    assert_eq!(result.coverage(), TradeCoverage::SourceEnumerated);
    assert_eq!(result.counts().confirmed, 0);
    assert_eq!(result.counts().candidates, 3);
    client.reconnect().unwrap();
    assert_ne!(client.tcp.connection_id(), initial_id);
    let new_end = client
        .history_trade_page_legacy(0, "000001", 20261022, 3, 3)
        .unwrap();
    assert_eq!(
        new_end.source().unwrap().endpoint(),
        first.source().unwrap().endpoint()
    );
    assert!(
        TradeCollection::new(
            history.clone(),
            vec![first.clone(), new_end],
            TradeTermination::EmptyPageObserved
        )
        .is_err()
    );
    let offline_end = decode_trade_page(&history, 3, 3, &[0; 6]).unwrap();
    assert_eq!(
        TradeCollection::new(
            history,
            vec![first, offline_end],
            TradeTermination::EmptyPageObserved
        )
        .unwrap()
        .coverage(),
        TradeCoverage::Unknown
    );
    drop(client);
    server.join().unwrap();
}
