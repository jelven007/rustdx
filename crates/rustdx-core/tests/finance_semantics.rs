use chrono::NaiveDate;
use rustdx_complete::tcp::{
    Tdx,
    stock::{
        FinanceField as F, FinanceInfo, FinanceProfile, FinanceRequest, FinanceSemanticStatus as S,
        FinanceUnit as U, Validatable, ValidationLevel, decode_finance, decode_finance_legacy,
        decode_finance_raw, normalize_finance,
    },
};

fn sample() -> Vec<u8> {
    include_str!("fixtures/protocol/finance.response.hex")
        .trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn decode(bytes: &[u8]) -> rustdx_complete::tcp::stock::FinanceSnapshot {
    decode_finance(&FinanceRequest::new(0, "000001").unwrap(), bytes)
        .unwrap()
        .remove(0)
}

#[test]
fn issuer_evidence_verifies_ten_fields_with_float32_and_disclosure_tolerances() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/protocol/finance.semantic.expected.json"
    ))
    .unwrap();
    let snapshot = decode(&sample());
    assert_eq!(snapshot.semantic_schema_version, 1);
    assert_eq!(snapshot.raw().updated_date_raw(), 20260815);
    assert_eq!(snapshot.report_period, NaiveDate::from_ymd_opt(2026, 6, 30));
    assert_eq!(snapshot.eps_basis, "basic_and_diluted_interim");
    assert_eq!(
        snapshot.net_profit_basis,
        "attributable_to_bank_shareholders"
    );
    assert_eq!(snapshot.net_assets_basis, "consolidated_total_equity");
    let verified: Vec<_> = snapshot
        .fields()
        .iter()
        .filter(|f| f.semantic_status() == S::Verified)
        .collect();
    assert_eq!(verified.len(), 10);
    for row in expected["fields"].as_array().unwrap() {
        let field = verified
            .iter()
            .find(|f| serde_json::to_value(f.field()).unwrap() == row["field"])
            .unwrap();
        assert_eq!(
            field.field().body_offset(),
            row["offset"].as_u64().unwrap() as usize
        );
        let raw = row["raw"].as_f64().unwrap() as f32;
        assert_eq!(field.raw_bits(), raw.to_bits());
        let value = field.value().unwrap();
        assert_eq!(value, f64::from(raw) * row["multiplier"].as_f64().unwrap());
        assert!(
            (value - row["official"].as_f64().unwrap()).abs() <= row["tolerance"].as_f64().unwrap()
        );
        assert!(field.evidence().unwrap().contains("pingan"));
    }
    let legacy = decode_finance_legacy(&FinanceRequest::new(0, "000001").unwrap(), &sample())
        .unwrap()
        .remove(0);
    assert_eq!(
        legacy.zongzichan / 10.0,
        snapshot.total_assets_yuan().unwrap()
    );
    assert_eq!(legacy.zhigonggu / 10000.0, snapshot.reported_eps().unwrap());
    assert_eq!(legacy.zongguben, snapshot.total_shares().unwrap());
    // Financial bases differ; these equalities must not be a validity condition.
    assert!(
        (snapshot.net_profit_yuan().unwrap() / snapshot.total_shares().unwrap()
            - snapshot.reported_eps().unwrap())
        .abs()
            > 0.01
    );
    assert!(!matches!(
        snapshot.validate().level,
        ValidationLevel::Error(_)
    ));
    assert!(snapshot.fetched_at.is_none());
}

#[test]
fn every_slot_is_lossless_and_candidates_require_explicit_access() {
    let bytes = sample();
    let snapshot = decode(&bytes);
    assert_eq!(snapshot.raw().body().as_slice(), &bytes[9..]);
    for (index, field) in F::ALL.into_iter().enumerate() {
        let offset = if index == 0 { 0 } else { 16 + (index - 1) * 4 };
        let expected = u32::from_le_bytes(bytes[9 + offset..13 + offset].try_into().unwrap());
        assert_eq!(snapshot.field(field).raw_bits(), expected);
        assert_eq!(snapshot.raw().raw_bits(offset).unwrap(), expected);
        let item = snapshot.field(field);
        let multiplier = match item.unit() {
            U::Shares => Some(10000.0),
            U::Yuan => Some(1000.0),
            U::YuanPerShare | U::Count => Some(1.0),
            U::Unknown => None,
        };
        assert_eq!(
            item.candidate_value(),
            multiplier.map(|m| f64::from(f32::from_bits(expected)) * m)
        );
        if item.semantic_status() != S::Verified {
            assert_eq!(item.value(), None);
            assert_eq!(item.evidence(), None);
        }
    }
    for field in [
        F::Slot01,
        F::Slot02,
        F::Slot03,
        F::Slot04,
        F::Slot05,
        F::Slot17,
        F::Slot29,
    ] {
        assert_eq!(snapshot.field(field).semantic_status(), S::Unknown);
        assert_eq!(snapshot.field(field).candidate_value(), None);
    }
    assert_eq!(
        snapshot.field(F::ShareholderCount).candidate_value(),
        Some(450712.0)
    );
    assert_eq!(
        snapshot.field(F::CashFlowNetChange).candidate_value(),
        Some(-51495000000.0)
    );
    for offset in [1, 4, 8, 12, 15, 17, 136, usize::MAX] {
        assert!(snapshot.raw().raw_bits(offset).is_err());
    }
    let serialized = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(serialized["raw"]["body"].as_array().unwrap().len(), 136);
}

#[test]
fn evidence_is_scoped_to_symbol_update_and_each_field() {
    for (market, code) in [(1, "000001"), (0, "000002"), (1, "600000")] {
        let mut bytes = sample();
        bytes[2] = market as u8;
        bytes[3..9].copy_from_slice(code.as_bytes());
        let snapshot = decode_finance(&FinanceRequest::new(market, code).unwrap(), &bytes)
            .unwrap()
            .remove(0);
        assert!(snapshot.fields().iter().all(|f| f.value().is_none()));
        assert_eq!(snapshot.report_period, None);
    }
    let mut bytes = sample();
    bytes[17..21].copy_from_slice(&20260816u32.to_le_bytes());
    assert!(decode(&bytes).fields().iter().all(|f| f.value().is_none()));
    for (field, expected_basis) in [
        (F::ReportedEps, "reported_unspecified"),
        (F::NetProfit, "unspecified"),
        (F::NetAssets, "unspecified"),
    ] {
        let mut bytes = sample();
        let offset = 9 + field.body_offset();
        bytes[offset..offset + 4].copy_from_slice(&(-1.0_f32).to_le_bytes());
        let snapshot = decode(&bytes);
        assert_eq!(snapshot.field(field).value(), None);
        assert_eq!(
            snapshot.field(field).candidate_value().unwrap(),
            if field == F::ReportedEps {
                -1.0
            } else {
                -1000.0
            }
        );
        let basis = match field {
            F::ReportedEps => snapshot.eps_basis,
            F::NetProfit => snapshot.net_profit_basis,
            _ => snapshot.net_assets_basis,
        };
        assert_eq!(basis, expected_basis);
    }
}

#[test]
fn consumption_requires_every_declared_field_and_the_bound_report_period() {
    let snapshot = decode(&sample());
    snapshot
        .require_fields(&[F::TotalShares, F::NetProfit])
        .unwrap();
    assert!(snapshot.require_fields(&[]).is_err());
    assert!(
        snapshot
            .require_fields(&[F::NetProfit, F::NetProfit])
            .is_err()
    );
    assert!(
        snapshot
            .require_fields(&[F::NetProfit, F::ShareholderCount])
            .is_err()
    );
    assert!(snapshot.require_fields(&[F::Slot29]).is_err());
    let mut changed = snapshot.clone();
    changed.report_period = NaiveDate::from_ymd_opt(2026, 9, 30);
    assert!(changed.require_fields(&[F::NetProfit]).is_err());
    changed = snapshot.clone();
    changed.semantic_schema_version += 1;
    assert!(changed.require_fields(&[F::NetProfit]).is_err());
    let mut bytes = sample();
    let offset = 9 + F::TotalShares.body_offset();
    bytes[offset..offset + 4].copy_from_slice(&1_f32.to_le_bytes());
    let changed = decode(&bytes);
    assert!(
        changed
            .require_fields(&[F::TotalShares, F::NetProfit])
            .is_err()
    );
    changed.require_fields(&[F::NetProfit]).unwrap();
}

#[test]
fn caller_evidence_can_verify_other_symbols_and_negative_financial_values() {
    for (market, code) in [(0, "300001"), (1, "600001")] {
        let mut bytes = sample();
        bytes[2] = market as u8;
        bytes[3..9].copy_from_slice(code.as_bytes());
        let period = NaiveDate::from_ymd_opt(2026, 6, 30).unwrap();
        let mut profile =
            FinanceProfile::new(market, code, 20260815, period, "synthetic test evidence").unwrap();
        for (field, raw) in [
            (F::CurrentAssets, 101.0_f32),
            (F::CurrentLiabilities, 52.0),
            (F::NetProfit, -3.0),
            (F::NetAssets, -2.0),
            (F::ReportedEps, -0.5),
            (F::OperatingCashFlow, -7.0),
            (F::CashFlowNetChange, -8.0),
        ] {
            let offset = 9 + field.body_offset();
            bytes[offset..offset + 4].copy_from_slice(&raw.to_le_bytes());
            profile = profile.verify(field, raw.to_bits()).unwrap();
        }
        let raw = decode_finance_raw(&FinanceRequest::new(market, code).unwrap(), &bytes)
            .unwrap()
            .remove(0);
        let snapshot = normalize_finance(raw, Some(&profile));
        assert_eq!(snapshot.net_profit_yuan(), Some(-3000.0));
        assert_eq!(snapshot.reported_eps(), Some(-0.5));
        assert!(matches!(
            snapshot.validate().level,
            ValidationLevel::Warning(_)
        ));
        assert_eq!(snapshot.eps_basis, "reported_unspecified");
        assert_eq!(snapshot.report_period, Some(period));
    }
    let period = NaiveDate::from_ymd_opt(2026, 6, 30).unwrap();
    assert!(FinanceProfile::new(0, "000001", 20260230, period, "test").is_err());
    assert!(FinanceProfile::new(0, "000001", 20260815, period, " ").is_err());
    let profile = FinanceProfile::new(0, "000001", 20260815, period, "test").unwrap();
    assert!(profile.clone().verify(F::Slot29, 6_f32.to_bits()).is_err());
    assert!(profile.verify(F::NetProfit, f32::NAN.to_bits()).is_err());
}

#[test]
fn malformed_records_fail_and_clear_state_while_zero_dates_remain_unknown() {
    let bytes = sample();
    let request = FinanceRequest::new(0, "000001").unwrap();
    let mut state = FinanceInfo::new(0, "000001");
    for end in 0..bytes.len() {
        assert!(decode_finance(&request, &bytes[..end]).is_err());
    }
    for field in F::ALL {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad = bytes.clone();
            let offset = 9 + field.body_offset();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            state.try_parse(bytes.clone()).unwrap();
            assert!(state.try_parse(bad).is_err());
            assert!(state.data.is_empty());
        }
    }
    let mut no_dates = bytes.clone();
    no_dates[17..25].fill(0);
    let snapshot = decode(&no_dates);
    assert_eq!(snapshot.raw().updated_date(), None);
    assert_eq!(snapshot.raw().ipo_date(), None);
    assert_eq!(snapshot.report_period, None);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_finance_raw(&request, &trailing).is_err());
    assert!(decode_finance(&request, &[0, 0]).unwrap().is_empty());
}
