use rustdx_complete::tcp::stock::{
    FinanceField as F, FinanceRequest, FinanceSemanticStatus as S, decode_finance,
};

fn sample() -> Vec<u8> {
    include_bytes!("fixtures/finance-q3/response.bin").to_vec()
}

#[test]
fn q3_disclosure_qualifies_seventeen_fields_and_rejects_cash_conflict() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/finance-q3/expected.json")).unwrap();
    let request = FinanceRequest::new(0, "000001").unwrap();
    let snapshot = decode_finance(&request, &sample()).unwrap().remove(0);
    assert_eq!(snapshot.report_period.unwrap().to_string(), "2026-09-30");
    assert_eq!(snapshot.eps_basis, "basic_and_diluted_year_to_date");
    assert_eq!(
        snapshot
            .fields()
            .iter()
            .filter(|f| f.value().is_some())
            .count(),
        17
    );
    for row in expected["fields"].as_array().unwrap() {
        let field = snapshot
            .fields()
            .iter()
            .find(|f| serde_json::to_value(f.field()).unwrap() == row["field"])
            .unwrap();
        assert_eq!(field.raw_bits(), row["raw_bits"].as_u64().unwrap() as u32);
        let official = row["official"].as_f64().unwrap();
        let multiplier = row["multiplier"].as_f64().unwrap();
        assert_eq!(
            (official / multiplier) as f32,
            f32::from_bits(row["expected_bits"].as_u64().unwrap() as u32)
        );
        if row["verified"].as_bool().unwrap() {
            assert_eq!(field.semantic_status(), S::Verified);
            assert_eq!(
                field.value(),
                Some(f64::from((official / multiplier) as f32) * multiplier)
            );
            assert!(field.evidence().unwrap().contains("1225621667.PDF"));
        } else {
            assert_eq!(field.semantic_status(), S::BaselineCandidate);
            assert_eq!(field.value(), None);
            assert_eq!(field.candidate_value(), None);
            assert!(serde_json::to_value(field).unwrap()["candidate_rejection"].is_object());
        }
    }
    snapshot
        .require_fields(&[F::TotalShares, F::ShareholderCount, F::OperatingCashFlow])
        .unwrap();
    assert!(snapshot.require_fields(&[F::CashFlowNetChange]).is_err());
    assert!(snapshot.require_fields(&[F::CirculatingShares]).is_err());
    assert_eq!(
        snapshot.field(F::CashFlowNetChange).raw_value(),
        8_515_000.0
    );
}

#[test]
fn q3_evidence_is_not_reused_for_another_update_symbol_or_changed_value() {
    for fault in 0..4 {
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
                let offset = 9 + F::ReportedEps.body_offset();
                bytes[offset..offset + 4].copy_from_slice(&1.0f32.to_le_bytes());
                (0, "000001")
            }
        };
        let snapshot = decode_finance(&FinanceRequest::new(market, code).unwrap(), &bytes)
            .unwrap()
            .remove(0);
        if fault < 3 {
            assert!(snapshot.fields().iter().all(|f| f.value().is_none()));
            assert_eq!(snapshot.report_period, None);
        } else {
            assert_eq!(snapshot.reported_eps(), None);
            assert_eq!(snapshot.field(F::ReportedEps).candidate_value(), None);
            snapshot.require_fields(&[F::TotalShares]).unwrap();
        }
    }
}
