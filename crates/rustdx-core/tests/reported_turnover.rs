use rustdx_complete::tcp::stock::*;
#[path = "common/trade_amount.rs"]
mod support;
use support::*;
#[path = "common/ip_pool.rs"]
mod ip_pool_test;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

#[test]
fn frozen_source_amounts_keep_codes_scope_units_and_fractional_precision() {
    for (category, count, expected) in [
        (8, 240, 27_918_479_958.875),
        (0, 48, 27_918_479_888.0),
        (9, 1, 27_918_479_360.0),
        (4, 1, 27_918_479_360.0),
    ] {
        let report = report(category);
        assert_eq!(report.bars().len(), count);
        assert_eq!(report.availability(), TurnoverAvailability::Available);
        assert_eq!(report.reported_amount_yuan(), Some(expected));
        assert_eq!(report.query().category(), category);
        assert_eq!(
            report.quantity_unit(),
            Some(if category == 4 {
                TradeQuantityUnit::LotsOf100Shares
            } else {
                TradeQuantityUnit::Shares
            })
        );
        for bar in report.bars() {
            assert_eq!(
                bar.reported_amount_yuan(),
                f64::from(f32::from_bits(bar.raw_amount_u32()))
            );
            assert_eq!(
                bar.reported_volume_raw(),
                f64::from(f32::from_bits(bar.raw_volume_u32()))
            );
            let bytes = &report.pages()[0].payload()[bar.byte_range()];
            assert_eq!(
                &bytes[bytes.len() - 4..],
                &bar.raw_amount_u32().to_le_bytes()
            );
        }
    }
}

#[test]
fn amount_leaf_and_compatibility_mapping_handle_zero_low_and_invalid_codes() {
    // The retained binary32 value is exactly 1_243_902.875 yuan.
    for amount in [0.0f32, 0.0001, 1.5, f32::from_bits(0x4997_d7f7)] {
        assert_eq!(
            decode_nonnegative_binary32(amount.to_bits()).unwrap(),
            f64::from(amount)
        );
        let request = KlineRequest::new(0, "300308", 9, 0, 1).unwrap();
        let bytes = kline_payload(9, &[(DATE, 900, 0.0, amount)]);
        assert_eq!(
            decode_stock_bars(&request, &bytes).unwrap()[0].amount,
            f64::from(amount)
        );
        let source = decode_reported_turnover(&request, &bytes, None).unwrap();
        assert_eq!(source.bars()[0].reported_volume_raw(), 0.0);
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
        assert!(decode_nonnegative_binary32(value.to_bits()).is_err());
        let request = KlineRequest::new(0, "300308", 9, 0, 1).unwrap();
        assert!(
            decode_reported_turnover(
                &request,
                &kline_payload(9, &[(DATE, 900, 1.0, value)]),
                None
            )
            .is_err()
        );
        assert!(
            decode_reported_turnover(
                &request,
                &kline_payload(9, &[(DATE, 900, value, 1.0)]),
                None
            )
            .is_err()
        );
    }
    let request = KlineRequest::new(0, "300308", 9, 0, 1).unwrap();
    let bytes = kline_payload(9, &[(DATE, 900, 1.0, 1.0)]);
    for end in 0..bytes.len() {
        assert!(decode_reported_turnover(&request, &bytes[..end], None).is_err());
    }
}

#[test]
fn missing_profiles_dates_and_after_hours_are_unavailable_not_zero() {
    let valid = report(9);
    let query = valid.query().clone();
    let no_profile =
        ReportedTurnover::from_pages(query.clone(), valid.pages().to_vec(), None).unwrap();
    assert_eq!(no_profile.reported_amount_yuan(), None);
    assert_eq!(
        no_profile.unavailable_reason(),
        Some(TurnoverUnavailableReason::UnconfirmedProfile)
    );
    let missing_query =
        TurnoverQuery::new(0, "300308", 20200102, TurnoverScope::RegularWithAuctions, 9).unwrap();
    let missing_profile =
        TurnoverProfile::a_share(0, "300308", 20200102, source().endpoint(), "dated evidence")
            .unwrap();
    let missing = ReportedTurnover::from_pages(
        missing_query,
        valid.pages().to_vec(),
        Some(&missing_profile),
    )
    .unwrap();
    assert_eq!(missing.reported_amount_yuan(), None);
    assert_eq!(
        missing.unavailable_reason(),
        Some(TurnoverUnavailableReason::NoBarsForDate)
    );
    let partial = small_report(&[(DATE, 571, 0.0, 0.0)]);
    assert_eq!(partial.reported_amount_yuan(), Some(0.0));
    assert_eq!(partial.availability(), TurnoverAvailability::Partial);
    for scope in [TurnoverScope::AfterHours, TurnoverScope::Combined] {
        let result = ReportedTurnover::from_pages(
            TurnoverQuery::new(0, "300308", DATE, scope, 9).unwrap(),
            valid.pages().to_vec(),
            Some(&profile()),
        )
        .unwrap();
        assert_eq!(result.reported_amount_yuan(), None);
        assert_eq!(
            result.regular_component_reported_amount_yuan(),
            Some(27_918_479_360.0)
        );
        assert_eq!(
            result.unavailable_reason(),
            Some(TurnoverUnavailableReason::NoIndependentAfterHoursSource)
        );
    }
    assert!(TurnoverQuery::new(0, "300308", DATE, TurnoverScope::Minute1, 9).is_err());
}

#[test]
fn mixed_sources_duplicate_bars_and_intraday_windows_are_checked() {
    let request = KlineRequest::new(0, "300308", 8, 0, 1).unwrap();
    let first = decode_reported_turnover(
        &request,
        &kline_payload(8, &[(DATE, 571, 1.0, 1.0)]),
        Some(source()),
    )
    .unwrap();
    let query = TurnoverQuery::new(0, "300308", DATE, TurnoverScope::Minute1, 8).unwrap();
    let second_request = KlineRequest::new(0, "300308", 8, 1, 1).unwrap();
    let other_source = TurnoverSource::archived(
        source().endpoint(),
        std::num::NonZeroU64::new(2).unwrap(),
        source().fetched_at(),
        "enhanced-history-amount-20261024",
    )
    .unwrap();
    let second = decode_reported_turnover(
        &second_request,
        &kline_payload(8, &[(DATE, 572, 1.0, 1.0)]),
        Some(other_source),
    )
    .unwrap();
    assert!(
        ReportedTurnover::from_pages(query.clone(), vec![first.clone(), second], Some(&profile()))
            .is_err()
    );
    let duplicate = decode_reported_turnover(
        &second_request,
        &kline_payload(8, &[(DATE, 571, 1.0, 1.0)]),
        Some(source()),
    )
    .unwrap();
    assert!(ReportedTurnover::from_pages(query, vec![first, duplicate], Some(&profile())).is_err());
    let (request_bytes, payload) = frozen("kline-8");
    let request = KlineRequest::new(
        0,
        "300308",
        8,
        0,
        u16::from_le_bytes(request_bytes[26..28].try_into().unwrap()),
    )
    .unwrap();
    let intraday = TurnoverSource::archived(
        source().endpoint(),
        source().connection_id(),
        "2026-10-23T03:00:00Z".parse().unwrap(),
        "intraday",
    )
    .unwrap();
    let page = decode_reported_turnover(&request, &payload, Some(intraday)).unwrap();
    let result = ReportedTurnover::from_pages(
        TurnoverQuery::new(0, "300308", DATE, TurnoverScope::Minute1, 8).unwrap(),
        vec![page],
        Some(&profile()),
    )
    .unwrap();
    assert_eq!(result.availability(), TurnoverAvailability::Partial);
    assert_eq!(result.sampling(), TurnoverSampling::IntradayOrUnobserved);
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
fn start_server(
    responses: Vec<Vec<u8>>,
) -> (rustdx_complete::tcp::TcpConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = rustdx_complete::tcp::TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut handshake = vec![0; rustdx_complete::tcp::PACK1.len()];
        stream.read_exact(&mut handshake).unwrap();
        assert_eq!(handshake, rustdx_complete::tcp::PACK1);
        frame(&mut stream, &handshake, &[0, 0]);
        for (index, payload) in responses.into_iter().enumerate() {
            let mut request = [0; 38];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request[10..12], &[0x2d, 5]);
            assert_eq!(
                u16::from_le_bytes(request[24..26].try_into().unwrap()),
                index as u16
            );
            frame(&mut stream, &request, &payload);
        }
    });
    (config, server)
}

#[test]
fn client_finds_requested_historical_day_and_enforces_budgets() {
    let (config, server) = start_server(vec![
        kline_payload(9, &[(20261024, 900, 2.0, 999.0)]),
        kline_payload(9, &[(DATE, 900, 1.0, 123.0)]),
    ]);
    let mut client = Client::with_config(&config).unwrap();
    let options = TurnoverOptions {
        page_size: 1,
        page_delay: Duration::ZERO,
        profile: Some(
            TurnoverProfile::a_share(0, "300308", DATE, config.ip.unwrap(), "mock scope").unwrap(),
        ),
        ..Default::default()
    };
    let result = client
        .reported_turnover(
            0,
            "300308",
            DATE,
            TurnoverScope::RegularWithAuctions,
            options,
        )
        .unwrap();
    assert_eq!(result.reported_amount_yuan(), Some(123.0));
    assert_eq!(result.pages().len(), 2);
    assert_eq!(
        result.pages()[0].source().unwrap().connection_id(),
        client.tcp.connection_id()
    );
    drop(client);
    server.join().unwrap();
    for limit in 0..3 {
        let (config, server) = start_server(vec![kline_payload(9, &[(20261024, 900, 2.0, 999.0)])]);
        let mut client = Client::with_config(&config).unwrap();
        let mut options = TurnoverOptions {
            page_size: 1,
            page_delay: Duration::ZERO,
            ..Default::default()
        };
        match limit {
            0 => options.max_pages = 1,
            1 => options.max_records = 1,
            _ => options.max_bytes = 1,
        }
        assert!(
            client
                .reported_turnover(
                    0,
                    "300308",
                    DATE,
                    TurnoverScope::RegularWithAuctions,
                    options
                )
                .is_err()
        );
        drop(client);
        server.join().unwrap();
    }
    let (config, server) = start_server(vec![kline_payload(9, &[(DATE - 1, 900, 2.0, 999.0)])]);
    let mut client = Client::with_config(&config).unwrap();
    let result = client
        .reported_turnover(
            0,
            "300308",
            DATE,
            TurnoverScope::RegularWithAuctions,
            TurnoverOptions {
                page_size: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.reported_amount_yuan(), None);
    drop(client);
    server.join().unwrap();
}

#[test]
fn client_reconnect_deadline_and_cancellation_never_return_partial_success() {
    use rustdx_complete::operation::OperationContext;
    for mode in 0..3 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = rustdx_complete::tcp::TcpConfig {
            ip: Some(listener.local_addr().unwrap()),
            ip_pool: Some(ip_pool_test::prepare(&listener)),
            timeout: Duration::from_secs(2),
            auto_reconnect: if mode == 0 { 1 } else { 0 },
            ..Default::default()
        };
        let context = OperationContext::new(Duration::from_secs(2), None).unwrap();
        let signal = context.clone();
        let server = std::thread::spawn(move || {
            for connection in 0..if mode == 0 { 2 } else { 1 } {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut handshake = vec![0; rustdx_complete::tcp::PACK1.len()];
                stream.read_exact(&mut handshake).unwrap();
                frame(&mut stream, &handshake, &[0, 0]);
                loop {
                    let mut request = [0; 38];
                    if stream.read_exact(&mut request).is_err() {
                        break;
                    }
                    let offset = u16::from_le_bytes(request[24..26].try_into().unwrap());
                    if mode == 0 && connection == 0 && offset == 1 {
                        break;
                    }
                    if mode == 2 && offset == 1 {
                        signal.cancel();
                    }
                    if mode == 1 {
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    // Descending but all newer than the requested date.
                    let payload =
                        kline_payload(9, &[(20261024 - u32::from(offset), 900, 1.0, 1.0)]);
                    let mut header = [0; 16];
                    header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                    header[5..9].copy_from_slice(&request[1..5]);
                    header[10..12].copy_from_slice(&request[10..12]);
                    header[12..14].copy_from_slice(&(payload.len() as u16).to_le_bytes());
                    header[14..16].copy_from_slice(&(payload.len() as u16).to_le_bytes());
                    if stream
                        .write_all(&header)
                        .and_then(|_| stream.write_all(&payload))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        let mut client = Client::with_config(&config).unwrap();
        let guard = context.enter();
        let result = client.reported_turnover(
            0,
            "300308",
            20200102,
            TurnoverScope::RegularWithAuctions,
            TurnoverOptions {
                page_size: 1,
                page_delay: Duration::ZERO,
                timeout: if mode == 1 {
                    Duration::from_millis(60)
                } else {
                    Duration::from_secs(2)
                },
                ..Default::default()
            },
        );
        let error = result.unwrap_err();
        match mode {
            0 => assert_eq!(error.kind(), std::io::ErrorKind::ConnectionAborted),
            1 => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            )),
            _ => assert_eq!(error.kind(), std::io::ErrorKind::Interrupted),
        }
        drop(guard);
        drop(client);
        server.join().unwrap();
    }
}
