use rustdx_complete::tcp::stock::{FinanceField as F, FinanceRequest, decode_finance};

fn sample() -> Vec<u8> {
    include_bytes!("fixtures/finance-q3/response.bin").to_vec()
}

#[test]
fn issuer_cash_difference_is_reconciled_as_exchange_rate_effect() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/finance-cash-basis/expected.json")).unwrap();
    let snapshot = decode_finance(&FinanceRequest::new(0, "000001").unwrap(), &sample())
        .unwrap()
        .remove(0);
    let comparison = snapshot.cash_flow_reconciliation().unwrap();
    let million = |name: &str| expected[name].as_i64().unwrap() as f64 * 1_000_000.0;
    assert_eq!(comparison.operating_yuan, million("operating"));
    assert_eq!(comparison.investing_yuan, million("investing"));
    assert_eq!(comparison.financing_yuan, million("financing"));
    assert_eq!(
        comparison.exchange_rate_effect_yuan,
        million("exchange_rate_effect")
    );
    assert_eq!(
        comparison.reported_net_change_yuan,
        million("reported_net_change")
    );
    assert_eq!(
        comparison.source_total_excluding_fx_yuan,
        comparison.operating_yuan + comparison.investing_yuan + comparison.financing_yuan
    );
    assert_eq!(
        comparison.source_total_excluding_fx_yuan + comparison.exchange_rate_effect_yuan,
        comparison.reported_net_change_yuan
    );
    assert_eq!(
        comparison.source_raw_bits,
        expected["source_raw_bits"].as_u64().unwrap() as u32
    );
    assert_eq!(
        comparison.evidence,
        expected["reference_url"].as_str().unwrap()
    );
    // The existing field continues to reject the different reported-net basis.
    assert_eq!(snapshot.field(F::CashFlowNetChange).value(), None);
    assert_eq!(snapshot.field(F::CashFlowNetChange).candidate_value(), None);
    assert!(snapshot.require_fields(&[F::CashFlowNetChange]).is_err());
}

#[test]
fn cash_basis_controls_do_not_qualify_changed_scope_or_raw_values() {
    for fault in 0..5 {
        let mut bytes = sample();
        let (market, code) = match fault {
            0 => {
                bytes[17..21].copy_from_slice(&20261025u32.to_le_bytes());
                (0, "000001")
            }
            1 => {
                bytes[3..9].copy_from_slice(b"000002");
                (0, "000002")
            }
            2 => {
                bytes[2] = 1;
                (1, "000001")
            }
            _ => {
                let field = if fault == 3 {
                    F::CashFlowNetChange
                } else {
                    F::OperatingCashFlow
                };
                let start = 9 + field.body_offset();
                bytes[start..start + 4].copy_from_slice(&1.0f32.to_le_bytes());
                (0, "000001")
            }
        };
        let snapshot = decode_finance(&FinanceRequest::new(market, code).unwrap(), &bytes)
            .unwrap()
            .remove(0);
        assert!(snapshot.cash_flow_reconciliation().is_err());
    }
    let snapshot = decode_finance(&FinanceRequest::new(0, "000001").unwrap(), &sample())
        .unwrap()
        .remove(0);
    for fault in 0..3 {
        let mut changed = snapshot.clone();
        match fault {
            0 => changed.semantic_schema_version += 1,
            1 => changed.profile_version = "unbound".into(),
            _ => changed.report_period = chrono::NaiveDate::from_ymd_opt(2026, 6, 30),
        }
        assert!(changed.cash_flow_reconciliation().is_err());
    }
}
