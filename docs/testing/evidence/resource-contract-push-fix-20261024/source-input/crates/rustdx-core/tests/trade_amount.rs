use rustdx_complete::tcp::stock::*;
#[path = "common/trade_amount.rs"]
mod support;
use support::*;

#[test]
fn frozen_day_preserves_records_duplicates_and_estimates() {
    let trades = collection(false);
    let ctx = context(trades.query().clone());
    let before = serde_json::to_value(&trades).unwrap();
    let summary = trades
        .amount_estimate_summary(&ctx, TradeEstimateOptions::default())
        .unwrap();
    assert_eq!(summary.raw_records(), 4876);
    assert_eq!(summary.excluded_records(), 87);
    assert_eq!(summary.regular_with_auctions().records(), 4744);
    assert_eq!(
        summary.regular_with_auctions().quantity_shares(),
        47_881_800
    );
    assert_eq!(
        summary.regular_with_auctions().estimated_amount_fen(),
        Some(2_791_801_083_900)
    );
    assert_eq!(summary.after_hours().records(), 45);
    assert_eq!(summary.after_hours().quantity_shares(), 18_100);
    assert_eq!(
        summary.after_hours().estimated_amount_yuan(),
        Some(11_088_603.0)
    );
    assert_eq!(summary.trade_coverage(), TradeCoverage::Unknown);
    assert_eq!(
        summary.regular_with_auctions().availability(),
        AmountAvailability::Partial
    );
    assert_eq!(before, serde_json::to_value(&trades).unwrap());
    let json = serde_json::to_value(&summary).unwrap();
    assert_eq!(
        json["regular_with_auctions"]["estimated_amount_fen"],
        "2791801083900"
    );
    let closing: Vec<_> = trades
        .records()
        .filter(|r| r.phase() == TradePhase::ClosingMatch)
        .collect();
    assert_eq!(closing.len(), 1);
    assert_eq!(
        (closing[0].price_raw(), closing[0].volume_raw()),
        (61263, 4094)
    );

    let legacy = collection(true);
    let legacy_ctx = context(legacy.query().clone());
    let excluded = legacy
        .amount_estimate_summary(&legacy_ctx, TradeEstimateOptions::default())
        .unwrap();
    assert_eq!(
        excluded.regular_with_auctions().estimated_amount_fen(),
        None
    );
    assert_eq!(excluded.candidate_records(), 4789);
    let included = legacy
        .amount_estimate_summary(
            &legacy_ctx,
            TradeEstimateOptions {
                include_candidates: true,
            },
        )
        .unwrap();
    assert_eq!(included.candidates().records(), 4789);
    assert_eq!(
        included.candidates().estimated_amount_yuan(),
        Some(27_929_099_442.0)
    );
    assert_eq!(
        included.candidates().availability(),
        AmountAvailability::Partial
    );
    let fields = |t: &TradeCollection| {
        t.pages()
            .iter()
            .rev()
            .flat_map(|p| p.records())
            .map(|r| {
                (
                    r.time_minutes(),
                    r.price_raw(),
                    r.volume_raw(),
                    r.status_raw(),
                    r.tail_raw(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(fields(&trades), fields(&legacy));
}

#[test]
fn unknown_units_nonexecutions_and_candidates_do_not_become_zero_totals() {
    let trades = simple_collection(&[
        (565, 1000, 10, 8),
        (600, 0, 0, 0),
        (720, 0, 2, 0),
        (601, 0, 1, 99),
    ]);
    let query = trades.query().clone();
    let ctx = context(query.clone());
    let outcomes = trades.pages()[0].amount_estimates(&ctx).unwrap();
    assert_eq!(outcomes.len(), 4);
    assert_eq!(
        outcomes[0].unavailable_reason(),
        Some(AmountUnavailableReason::NotExecution)
    );
    assert_eq!(outcomes[1].estimate().unwrap().estimated_amount_fen(), 0);
    assert_eq!(outcomes[2].qualification(), AmountQualification::Candidate);
    assert_eq!(
        outcomes[3].unavailable_reason(),
        Some(AmountUnavailableReason::UnqualifiedEvent)
    );
    let summary = trades
        .amount_estimate_summary(&ctx, TradeEstimateOptions::default())
        .unwrap();
    assert_eq!(
        summary.regular_with_auctions().estimated_amount_fen(),
        Some(0)
    );
    assert_eq!(
        summary.regular_with_auctions().availability(),
        AmountAvailability::Partial
    );
    let unknown = TradeAmountContext::unknown(query);
    let outcomes = trades.pages()[0].amount_estimates(&unknown).unwrap();
    assert_eq!(
        outcomes[1].unavailable_reason(),
        Some(AmountUnavailableReason::UnknownQuantityUnit)
    );
    assert_eq!(
        trades
            .amount_estimate_summary(&unknown, TradeEstimateOptions::default())
            .unwrap()
            .regular_with_auctions()
            .estimated_amount_fen(),
        None
    );
    for rows in [
        vec![],
        vec![(565, 1000, 10, 8)],
        vec![(720, 1000, 10, 0)],
        vec![(600, 1000, 10, 99)],
    ] {
        let trades = simple_collection(&rows);
        assert_eq!(
            trades
                .amount_estimate_summary(&ctx, TradeEstimateOptions::default())
                .unwrap()
                .regular_with_auctions()
                .estimated_amount_fen(),
            None
        );
    }
}

#[test]
fn identity_and_explicit_share_units_are_checked() {
    let trades = simple_collection(&[(600, 12345, 2, 0), (600, 0, 2, 0)]);
    let shares = TradeAmountContext::a_share(
        trades.query().clone(),
        TradeQuantityUnit::Shares,
        "explicit shares",
    )
    .unwrap();
    let summary = trades
        .amount_estimate_summary(&shares, TradeEstimateOptions::default())
        .unwrap();
    assert_eq!(
        summary.regular_with_auctions().estimated_amount_fen(),
        Some(49_380)
    );
    assert_eq!(summary.regular_with_auctions().records(), 2);
    for query in [
        TradeQuery::history(1, "600000", DATE).unwrap(),
        TradeQuery::history(0, "300308", DATE - 1).unwrap(),
        TradeQuery::history_legacy(0, "300308", DATE).unwrap(),
    ] {
        assert!(trades.pages()[0].amount_estimates(&context(query)).is_err());
    }
    assert!(
        TradeAmountContext::a_share(
            trades.query().clone(),
            TradeQuantityUnit::Unknown,
            "evidence"
        )
        .is_err()
    );
    assert!(
        TradeAmountContext::a_share(trades.query().clone(), TradeQuantityUnit::Shares, "").is_err()
    );
    let unsupported = TradeQuery::history(0, "159001", DATE).unwrap();
    assert!(
        TradeAmountContext::a_share(unsupported, TradeQuantityUnit::Shares, "evidence").is_err()
    );
}

#[test]
fn enhanced_dto_mutation_cannot_change_authoritative_amount() {
    let (request_bytes, bytes) = frozen("enhanced-00");
    let request = EnhancedHistoryTransactionRequest::new(0, "300308", 0, 1800, DATE).unwrap();
    assert_eq!(request.encode().as_slice(), request_bytes);
    let original = decode_enhanced_history_transactions(&request, &bytes).unwrap();
    let ctx = context(TradeQuery::history(0, "300308", DATE).unwrap());
    assert_eq!(original.amount_estimates(&ctx).unwrap().len(), 1800);
    for edit in 0..5 {
        let mut page = original.clone();
        match edit {
            0 => page.records[0].price_acc_raw += 1,
            1 => page.records[0].price = f64::NAN,
            2 => page.records[0].volume = -1,
            3 => page.records[0].status_raw = 8,
            _ => page.price_base_raw_f32 = 1.0,
        }
        assert!(page.amount_estimates(&ctx).is_err());
    }
}
