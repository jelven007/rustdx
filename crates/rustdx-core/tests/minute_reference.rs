use chrono::Utc;
use rustdx_complete::tcp::stock::*;

fn sample() -> (MinuteSeries, MinuteDateReference, Vec<i64>) {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/minute-boundary/expected.json")).unwrap();
    let metadata = PrecisionSnapshot::from_decimals(&[(0, "000001", 2)], Utc::now(), None).unwrap();
    let request = RecentMinutesRequest::new(0, "000001", 20261027).unwrap();
    let series = decode_recent_minutes(
        &request,
        include_bytes!("fixtures/minute-boundary/response.bin"),
        &metadata,
    )
    .unwrap();
    let controls = expected["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| MinuteDateControl {
            index: p["index"].as_u64().unwrap() as u16,
            price_raw: p["price_raw"].as_i64().unwrap(),
        })
        .collect();
    let reference = MinuteDateReference::new(
        CommandSymbol::new(0, "000001").unwrap(),
        20261027,
        series.scale,
        controls,
        expected["source"].as_str().unwrap(),
        expected["sha256"].as_str().unwrap(),
    )
    .unwrap();
    let volumes = expected["raw_cumulative_volumes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_i64().unwrap())
        .collect();
    (series, reference, volumes)
}

#[test]
fn real_morning_boundary_diff_is_explained_without_shifting_labels_or_qualifying_date() {
    let (series, reference, volumes) = sample();
    let comparison = series
        .compare_reference(&reference, Some(&volumes))
        .unwrap();
    assert!(!comparison.all_prices_match);
    assert_eq!(comparison.all_raw_cumulative_volumes_match, Some(false));
    let indexes: Vec<_> = comparison.differences.iter().map(|d| d.index).collect();
    assert_eq!(indexes, [119, 208]);
    let boundary = &comparison.differences[0];
    assert_eq!(boundary.position, MinuteReferencePosition::MorningClose);
    assert_eq!(boundary.synthetic_label, "11:30");
    assert_eq!(boundary.tcp_price_raw, 1146);
    assert_eq!(boundary.reference_price_raw, 1147);
    assert_eq!(boundary.tcp_cumulative_volume_raw, 913147);
    assert_eq!(boundary.reference_cumulative_volume_raw, Some(913222));
    assert_eq!(series.page.records[0].time_label, "09:31");
    assert_eq!(series.page.records[0].volume_raw, 39200);
    assert!(series.require_trading_date().is_err());
    assert!(series.with_date_reference(reference).is_err());
}

#[test]
fn diagnostics_reject_incomplete_controls_and_changed_decoded_values() {
    let (series, reference, volumes) = sample();
    assert!(
        series
            .compare_reference(&reference, Some(&volumes[..volumes.len() - 1]))
            .is_err()
    );
    let mut bad = series.clone();
    bad.page.records[119].price_raw = 1147;
    assert!(bad.compare_reference(&reference, Some(&volumes)).is_err());
    let mut bad = volumes;
    bad[1] = -1;
    assert!(series.compare_reference(&reference, Some(&bad)).is_err());
    let prices = series.compare_reference(&reference, None).unwrap();
    assert_eq!(prices.all_raw_cumulative_volumes_match, None);
    assert_eq!(prices.differences.len(), 1);
}
