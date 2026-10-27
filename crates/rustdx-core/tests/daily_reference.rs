use rustdx_complete::tcp::stock::{DailyBarsRequest, DayFileReference, decode_daily_bars};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/daily-reference")
}

fn manifest() -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(root().join("manifest.json")).unwrap()).unwrap()
}

#[test]
fn original_archive_reconciliation_preserves_all_forty_four_source_differences() {
    let manifest = manifest();
    let mut checked = 0;
    let mut differences = 0;
    for symbol in manifest["symbols"].as_array().unwrap() {
        let market = symbol["market"].as_u64().unwrap() as u16;
        let code = symbol["code"].as_str().unwrap();
        let bytes = std::fs::read(root().join(symbol["file"].as_str().unwrap())).unwrap();
        let reference = DayFileReference::new(
            market,
            code,
            &bytes,
            manifest["source_url"].as_str().unwrap(),
            symbol["sha256"].as_str().unwrap(),
        )
        .unwrap();
        let mut dates = Vec::new();
        for wire in symbol["pages"].as_array().unwrap() {
            let payload = std::fs::read(root().join(wire["file"].as_str().unwrap())).unwrap();
            assert_eq!(
                format!("{:x}", sha2::Sha256::digest(&payload)),
                wire["sha256"].as_str().unwrap()
            );
            let request = DailyBarsRequest::new(
                market,
                code,
                wire["start"].as_u64().unwrap() as u16,
                wire["count"].as_u64().unwrap() as u16,
            )
            .unwrap();
            let page = decode_daily_bars(&request, &payload).unwrap();
            let comparison = reference.compare_page(&page).unwrap();
            assert_eq!(comparison.records.len(), page.records.len());
            for row in comparison.records {
                checked += 1;
                assert_eq!(row.tcp.volume_raw, row.tcp.volume_shares);
                assert_eq!(row.reference.bar.symbol.to_string(), reference.symbol());
                if !row.differing_fields.is_empty() {
                    differences += 1;
                    dates.push(row.tcp.date_raw);
                    let expected = symbol["differences"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|d| d["date"].as_u64() == Some(row.tcp.date_raw as u64))
                        .unwrap();
                    if let Some(volume) = expected["tcp"]["vol"].as_u64() {
                        assert_eq!(row.tcp.volume_shares as u64, volume);
                        assert_eq!(
                            row.reference.bar.volume_shares,
                            expected["official"]["vol"].as_f64().unwrap()
                        );
                    } else {
                        assert_eq!(row.tcp.amount, expected["tcp"]["amount"].as_f64().unwrap());
                        assert_eq!(
                            row.reference.bar.amount,
                            expected["official"]["amount"].as_f64().unwrap()
                        );
                    }
                }
            }
        }
        dates.sort_unstable();
        let mut expected: Vec<_> = symbol["differences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["date"].as_u64().unwrap() as u32)
            .collect();
        expected.sort_unstable();
        assert_eq!(dates, expected);
    }
    assert_eq!(checked, 27237);
    assert_eq!(differences, 44);
}

#[test]
fn wrong_symbol_missing_date_corrupt_reference_and_mutated_dto_are_rejected() {
    let manifest = manifest();
    let symbol = &manifest["symbols"][0];
    let market = symbol["market"].as_u64().unwrap() as u16;
    let code = symbol["code"].as_str().unwrap();
    let bytes = std::fs::read(root().join(symbol["file"].as_str().unwrap())).unwrap();
    let source = manifest["source_url"].as_str().unwrap();
    let digest = symbol["sha256"].as_str().unwrap();
    let reference = DayFileReference::new(market, code, &bytes, source, digest).unwrap();
    let wire = &symbol["pages"][0];
    let payload = std::fs::read(root().join(wire["file"].as_str().unwrap())).unwrap();
    let request = DailyBarsRequest::new(market, code, 0, 800).unwrap();
    let page = decode_daily_bars(&request, &payload).unwrap();
    let wrong = DailyBarsRequest::new(0, "000002", 0, 800).unwrap();
    assert!(
        reference
            .compare_page(&decode_daily_bars(&wrong, &payload).unwrap())
            .is_err()
    );
    let mut bad = bytes.clone();
    bad[24] ^= 1;
    assert!(DayFileReference::new(market, code, &bad, source, digest).is_err());
    assert!(DayFileReference::new(market, code, &bytes[..31], source, digest).is_err());
    for fault in 0..4 {
        let mut bad = page.clone();
        match fault {
            0 => bad.records[0].volume_raw += 1,
            1 => bad.records[0].amount += 1.0,
            2 => bad.records[0].date_raw += 1,
            _ => bad.raw_payload = [0_u8].as_slice().into(),
        }
        assert!(reference.compare_page(&bad).is_err());
    }
    // A valid, correctly hashed partial reference must reject absent TCP dates.
    let partial = &bytes[..32];
    let digest = format!("{:x}", sha2::Sha256::digest(partial));
    let partial = DayFileReference::new(market, code, partial, source, &digest).unwrap();
    assert!(partial.compare_page(&page).is_err());
}

use sha2::Digest;
