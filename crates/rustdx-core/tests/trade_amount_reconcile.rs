use rustdx_complete::tcp::stock::*;
#[path = "common/trade_amount.rs"]
mod support;
use support::*;
#[path = "common/ip_pool.rs"]
mod ip_pool_test;

#[test]
fn frozen_day_aligns_240_and_48_intervals_without_repairing_amounts() {
    let trades = collection(false);
    let ctx = context(trades.query().clone());
    let original = serde_json::to_value(&trades).unwrap();
    let daily = report(9);
    for (category, count, difference) in [(8, 240, -469_119.875), (0, 48, -469_049.0)] {
        let minute = report(category);
        let result = reconcile_trade_amounts(&trades, &ctx, &minute, Some(&daily)).unwrap();
        assert_eq!(result.alignment(), TradeAmountAlignment::Exact);
        assert_eq!(result.intervals().len(), count);
        assert_eq!(result.estimate_minus_minute_yuan(), Some(difference));
        assert_eq!(result.estimate_minus_day_yuan(), Some(-468_521.0));
        assert_eq!(
            result
                .intervals()
                .iter()
                .map(|i| i.quantity_shares())
                .sum::<i128>(),
            47_881_800
        );
        assert_eq!(result.intervals().last().unwrap().event_range().end, 4744);
        assert!(!result.full_scope_available()); // Offline trade pages have no live provenance.
        assert_eq!(
            result.source_status(),
            ReconciliationSourceStatus::MissingOrArchived
        );
        if category == 8 {
            assert_eq!(result.minute_minus_day_yuan(), Some(598.875));
            assert_eq!(result.minute_encoding_step_sum_yuan(), 2416.125);
            assert_eq!(
                result.intervals().last().unwrap().quantity_shares(),
                409_400
            );
        }
    }
    assert_eq!(original, serde_json::to_value(&trades).unwrap());
}

#[test]
fn quantity_mismatch_and_duplicate_zero_prefix_report_diagnostics() {
    let trades = simple_collection(&[(600, 1000, 10, 0)]);
    let ctx = context(trades.query().clone());
    let mismatch = small_report(&[(DATE, 600, 1001.0, 10_000.0)]);
    let result = reconcile_trade_amounts(&trades, &ctx, &mismatch, None).unwrap();
    assert_eq!(result.alignment(), TradeAmountAlignment::QuantityMismatch);
    assert_eq!(result.failed_bar_index(), Some(0));
    assert!(result.intervals().is_empty());
    let zero_trades = simple_collection(&[(600, 1000, 0, 0)]);
    let zero_bar = small_report(&[(DATE, 600, 0.0, 0.0)]);
    let result = reconcile_trade_amounts(&zero_trades, &ctx, &zero_bar, None).unwrap();
    assert_eq!(result.alignment(), TradeAmountAlignment::AmbiguousBoundary);
    let fractional = small_report(&[(DATE, 600, 1000.5, 10_000.0)]);
    assert_eq!(
        reconcile_trade_amounts(&trades, &ctx, &fractional, None)
            .unwrap()
            .alignment(),
        TradeAmountAlignment::QuantityMismatch
    );
}

#[test]
fn partial_unknown_candidate_date_and_scope_are_not_promoted() {
    let complete = simple_collection(&[(600, 1000, 10, 0)]);
    let ctx = context(complete.query().clone());
    let minute = small_report(&[(DATE, 600, 1000.0, 10_000.0)]);
    let partial = TradeCollection::new(
        complete.query().clone(),
        vec![complete.pages()[0].clone()],
        TradeTermination::CallerStopped,
    )
    .unwrap();
    assert_eq!(
        reconcile_trade_amounts(&partial, &ctx, &minute, None)
            .unwrap()
            .alignment(),
        TradeAmountAlignment::CoverageInsufficient
    );
    for rows in [vec![(600, 1000, 10, 99)], vec![(720, 1000, 10, 0)]] {
        let trades = simple_collection(&rows);
        assert_eq!(
            reconcile_trade_amounts(&trades, &ctx, &minute, None)
                .unwrap()
                .alignment(),
            TradeAmountAlignment::CoverageInsufficient
        );
    }
    assert_eq!(
        reconcile_trade_amounts(&complete, &ctx, &report(9), None)
            .unwrap()
            .alignment(),
        TradeAmountAlignment::UnsupportedScope
    );
    let wrong_date = ReportedTurnover::from_pages(
        TurnoverQuery::new(0, "300308", DATE - 1, TurnoverScope::Minute1, 8).unwrap(),
        minute.pages().to_vec(),
        None,
    )
    .unwrap();
    assert!(reconcile_trade_amounts(&complete, &ctx, &wrong_date, None).is_err());
    let wrong_endpoint = TurnoverSource::archived(
        "127.0.0.1:7709".parse().unwrap(),
        source().connection_id(),
        source().fetched_at(),
        "other",
    )
    .unwrap();
    let req = KlineRequest::new(0, "300308", 9, 0, 1).unwrap();
    let page = decode_reported_turnover(
        &req,
        &kline_payload(9, &[(DATE, 900, 1000.0, 10_000.0)]),
        Some(wrong_endpoint),
    )
    .unwrap();
    let day = ReportedTurnover::from_pages(
        TurnoverQuery::new(0, "300308", DATE, TurnoverScope::RegularWithAuctions, 9).unwrap(),
        vec![page],
        None,
    )
    .unwrap();
    assert!(reconcile_trade_amounts(&complete, &ctx, &minute, Some(&day)).is_err());
}

#[test]
fn page_request_order_is_reversed_but_duplicates_are_preserved() {
    let query = TradeQuery::history(0, "300308", DATE).unwrap();
    let trades = TradeCollection::new(
        query.clone(),
        vec![
            page(&query, 0, &[(601, 2000, 3, 0), (601, 0, 3, 0)]),
            page(&query, 2, &[(600, 1000, 2, 0)]),
            page(&query, 3, &[]),
        ],
        TradeTermination::EmptyPageObserved,
    )
    .unwrap();
    let result = reconcile_trade_amounts(
        &trades,
        &context(query),
        &small_report(&[(DATE, 600, 200.0, 2000.0), (DATE, 601, 600.0, 12000.0)]),
        None,
    )
    .unwrap();
    assert_eq!(result.alignment(), TradeAmountAlignment::Exact);
    assert_eq!(result.intervals()[0].event_range(), 0..1);
    assert_eq!(result.intervals()[1].event_range(), 1..3);
}

#[test]
fn live_replay_checks_compatibility_adapter_and_same_connection_reconciliation() {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        time::Duration,
    };
    fn respond(stream: &mut TcpStream, request: &[u8], payload: &[u8]) {
        let mut received = vec![0; request.len()];
        stream.read_exact(&mut received).unwrap();
        assert_eq!(received, request);
        let mut header = [0; 16];
        header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
        header[5..9].copy_from_slice(&request[1..5]);
        header[10..12].copy_from_slice(&request[10..12]);
        header[12..14].copy_from_slice(&(payload.len() as u16).to_le_bytes());
        header[14..16].copy_from_slice(&(payload.len() as u16).to_le_bytes());
        stream.write_all(&header).unwrap();
        stream.write_all(payload).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = rustdx_complete::tcp::TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        for connection in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            respond(&mut stream, rustdx_complete::tcp::PACK1, &[0, 0]);
            if connection == 0 {
                for index in 0..4 {
                    let (request, payload) = frozen(&format!("enhanced-{index:02}"));
                    respond(&mut stream, &request, &payload);
                }
            }
            for category in [8, 9] {
                let (_, payload) = frozen(&format!("kline-{category}"));
                let request = KlineRequest::new(0, "300308", category, 0, 800).unwrap();
                respond(&mut stream, &request.encode(), &payload);
            }
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    let raw = client
        .history_transaction_all(
            0,
            "300308",
            DATE,
            TransactionPaginationOptions {
                page_delay: Duration::ZERO,
                ..Default::default()
            },
        )
        .unwrap();
    let ctx = context(raw.event_pages[0].query().clone());
    let summary = raw
        .amount_estimate_summary(&ctx, TradeEstimateOptions::default())
        .unwrap();
    assert_eq!(summary.trade_coverage(), TradeCoverage::SourceEnumerated);
    assert_eq!(
        summary.regular_with_auctions().availability(),
        AmountAvailability::Available
    );
    for mutation in 0..4 {
        let mut changed = raw.clone();
        match mutation {
            0 => changed.records[0].price += 0.01,
            1 => changed.command = 0x0fb5,
            2 => changed.pages[0].record_range = 0..1,
            _ => changed.requested_date = Some(DATE - 1),
        }
        assert!(
            changed
                .amount_estimate_summary(&ctx, TradeEstimateOptions::default())
                .is_err()
        );
    }
    let trades = TradeCollection::new(
        ctx.query().clone(),
        raw.event_pages,
        TradeTermination::EmptyPageObserved,
    )
    .unwrap();
    let options = TurnoverOptions {
        page_delay: Duration::ZERO,
        profile: Some(
            TurnoverProfile::a_share(
                0,
                "300308",
                DATE,
                config.ip.unwrap(),
                "frozen replay profile",
            )
            .unwrap(),
        ),
        ..Default::default()
    };
    for connection in 0..2 {
        if connection == 1 {
            client.reconnect().unwrap();
        }
        let minute = client
            .reported_turnover(0, "300308", DATE, TurnoverScope::Minute1, options.clone())
            .unwrap();
        let day = client
            .reported_turnover(
                0,
                "300308",
                DATE,
                TurnoverScope::RegularWithAuctions,
                options.clone(),
            )
            .unwrap();
        let result = reconcile_trade_amounts(&trades, &ctx, &minute, Some(&day));
        if connection == 0 {
            let result = result.unwrap();
            assert_eq!(result.alignment(), TradeAmountAlignment::Exact);
            assert!(result.full_scope_available());
            assert_eq!(result.estimate_minus_day_yuan(), Some(-468_521.0));
        } else {
            assert!(result.is_err());
        }
    }
    drop(client);
    server.join().unwrap();
}
