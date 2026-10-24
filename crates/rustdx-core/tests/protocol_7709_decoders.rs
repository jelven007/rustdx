use rustdx_complete::tcp::stock::*;
#[path = "common/extended_protocol.rs"]
mod common;
use common::*;

fn truncations(bytes: &[u8], decode: impl Fn(&[u8]) -> bool) {
    for end in 0..bytes.len() {
        assert!(!decode(&bytes[..end]), "accepted truncation at {end}");
    }
}
#[test]
fn requests_encode_nine_distinct_opcodes_and_reserved_fields() {
    let requests = [
        (
            0x0452,
            SpecialLimitsRequest::new(7).encode(123).unwrap(),
            14,
        ),
        (
            0x051b,
            IntradayAuxRequest::new(0, "000001", IntradayAuxKind::BuySellStrength)
                .unwrap()
                .encode(123)
                .unwrap(),
            28,
        ),
        (
            0x0fd1,
            SparklineRequest::new(0, "000001", SparklineOptions::default())
                .unwrap()
                .encode(123)
                .unwrap(),
            37,
        ),
        (
            0x0feb,
            RecentMinutesRequest::new(0, "000001", 20261023)
                .unwrap()
                .encode(123)
                .unwrap(),
            11,
        ),
        (
            0x056a,
            AuctionSeriesRequest::new(0, "000001", Some(20261023), AuctionOptions::default())
                .unwrap()
                .encode(123)
                .unwrap(),
            28,
        ),
        (
            0x0ffc,
            MoneyFlowRequest::new(0, "000001")
                .unwrap()
                .encode(123)
                .unwrap(),
            38,
        ),
        (
            0x054c,
            SnapshotsRequest::new(&[(0, "000001")])
                .unwrap()
                .encode(123)
                .unwrap(),
            17,
        ),
        (
            0x054b,
            CategoryQuotesRequest::new(
                QuoteCategory::AShares,
                QuoteSortField::Amount,
                CategoryQuoteOptions::default(),
            )
            .unwrap()
            .encode(123)
            .unwrap(),
            18,
        ),
        (
            0x0547,
            RefreshRequest::from_stocks(&[(0, "000001")])
                .unwrap()
                .encode(123)
                .unwrap(),
            13,
        ),
    ];
    for (opcode, bytes, body_len) in requests {
        assert_eq!(bytes.len(), 12 + body_len);
        assert_eq!(
            u16::from_le_bytes(bytes[10..12].try_into().unwrap()),
            opcode
        );
        assert_eq!(bytes[6..8], bytes[8..10]);
        assert_eq!(
            u16::from_le_bytes(bytes[6..8].try_into().unwrap()) as usize,
            body_len + 2
        );
        assert_eq!(
            &bytes[1..5],
            &if opcode == 0x0ffc { 0x007e2d7bu32 } else { 123 }.to_le_bytes()
        );
    }
    assert!(SnapshotsRequest::new(&[(0, "000001"), (0, "000001")]).is_err());
    assert!(RefreshRequest::from_stocks(&[]).is_err());
    let codes: Vec<_> = (0..101).map(|n| format!("{n:06}")).collect();
    let symbols: Vec<_> = codes.iter().map(|c| (0, c.as_str())).collect();
    assert!(RefreshRequest::from_stocks(&symbols[..100]).is_ok());
    assert!(RefreshRequest::from_stocks(&symbols).is_err());
    assert!(RecentMinutesRequest::new(0, "000001", 20260230).is_err());
    assert!(
        SparklineRequest::new(
            0,
            "000001",
            SparklineOptions {
                window: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn limits_fixed_length_validation_and_negative_auction_direction() {
    let mut limits = 1u16.to_le_bytes().to_vec();
    limits.push(0);
    limits.extend(1u32.to_le_bytes());
    limits.extend(20f32.to_le_bytes());
    limits.extend(10f32.to_le_bytes());
    let page = decode_special_limits(&limits).unwrap();
    assert_eq!(page.records[0].code, "000001");
    truncations(&limits, |b| decode_special_limits(b).is_ok());
    let mut bad = limits.clone();
    bad[7..11].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(decode_special_limits(&bad).is_err());
    let request = AuctionSeriesRequest::new(0, "000001", None, AuctionOptions::default()).unwrap();
    let mut auction = 1u16.to_le_bytes().to_vec();
    auction.extend(565u16.to_le_bytes());
    auction.extend(12f32.to_le_bytes());
    auction.extend(100u32.to_le_bytes());
    auction.extend(i32::MIN.to_le_bytes());
    auction.extend([0, 0]);
    let result = decode_auction_series(&request, &auction).unwrap();
    let point = &result.page.records[0];
    assert_eq!(point.time_label, "09:25:00");
    assert_eq!(point.unmatched_volume_raw, 2_147_483_648);
    assert_eq!(point.unmatched_sign, -1);
    assert!(result.process_snapshots);
    truncations(&auction, |b| decode_auction_series(&request, b).is_ok());
    *auction.last_mut().unwrap() = 60;
    assert!(decode_auction_series(&request, &auction).is_err());
}
#[test]
fn auxiliary_supports_i64_extremes_and_finite_float_pairs() {
    let request = IntradayAuxRequest::new(0, "000001", IntradayAuxKind::BuySellStrength).unwrap();
    let mut bytes = 1u16.to_le_bytes().to_vec();
    bytes.extend(varint(i64::MIN));
    bytes.extend(varint(i64::MAX));
    assert_eq!(
        decode_intraday_aux(&request, &bytes).unwrap().page.records[0],
        IntradayAuxPoint::IntegerPair {
            index: 0,
            series_a: i64::MIN,
            series_b: i64::MAX
        }
    );
    truncations(&bytes, |b| decode_intraday_aux(&request, b).is_ok());
    let mut overlong = vec![1, 0];
    overlong.extend([0xff; 11]);
    assert!(decode_intraday_aux(&request, &overlong).is_err());
    let request = IntradayAuxRequest::new(0, "000001", IntradayAuxKind::VolumeComparison).unwrap();
    let mut bytes = vec![1, 0];
    bytes.extend(12f32.to_le_bytes());
    bytes.extend(f32::INFINITY.to_le_bytes());
    assert!(decode_intraday_aux(&request, &bytes).is_err());
}
#[test]
fn recent_minute_deltas_are_relative_to_first_and_average_has_extra_scale() {
    let request = RecentMinutesRequest::new(0, "159915", 20261023).unwrap();
    let mut bytes = 3u16.to_le_bytes().to_vec();
    bytes.extend(3f32.to_le_bytes());
    bytes.extend(3f32.to_le_bytes());
    for (p, a, v) in [(3000, 300000, 10), (10, 1000, 20), (20, 2000, 30)] {
        for field in [p, a, v] {
            bytes.extend(varint(field));
        }
    }
    let series = decode_recent_minutes(&request, &bytes, &metadata()).unwrap();
    assert_eq!(series.page.records[2].price, 3.02);
    assert_eq!(series.page.records[2].average_price, 3.02);
    assert_eq!(series.server_trading_date, None);
    truncations(&bytes, |b| {
        decode_recent_minutes(&request, b, &metadata()).is_ok()
    });
}
#[test]
fn sparkline_validates_identity_and_capacity() {
    let request = SparklineRequest::new(0, "000001", SparklineOptions::default()).unwrap();
    let mut bytes = vec![0; 42];
    bytes[2..8].copy_from_slice(b"000001");
    bytes[24] = 1;
    bytes[34..36].copy_from_slice(&2u16.to_le_bytes());
    bytes[40..42].copy_from_slice(&2u16.to_le_bytes());
    bytes.extend(12f32.to_le_bytes());
    bytes.extend(13f32.to_le_bytes());
    assert_eq!(
        decode_sparkline(&request, &bytes)
            .unwrap()
            .page
            .records
            .len(),
        2
    );
    truncations(&bytes, |b| decode_sparkline(&request, b).is_ok());
    bytes[0] = 1;
    assert!(decode_sparkline(&request, &bytes).is_err());
}
#[test]
fn snapshots_handle_missing_out_of_order_codes_and_preserve_tails() {
    let request = SnapshotsRequest::new(&[(0, "000001"), (0, "159915"), (1, "510300")]).unwrap();
    let mut bytes = vec![0, 0, 2, 0];
    bytes.extend(quote(1, "510300", false, 0));
    bytes.extend([9, 8]);
    bytes.extend(quote(0, "159915", false, 0));
    let result = decode_snapshots(&request, &bytes, &metadata()).unwrap();
    assert_eq!(result.missing_codes[0].code(), "000001");
    assert_eq!(result.page.records[0].tail_raw, [9, 8]);
    assert_eq!(result.page.records[1].price, Some(1.234));
    let mut duplicate = vec![0, 0, 2, 0];
    duplicate.extend(quote(0, "000001", false, 0));
    duplicate.extend(quote(0, "000001", false, 0));
    assert!(decode_snapshots(&request, &duplicate, &metadata()).is_err());
    let one = SnapshotsRequest::new(&[(0, "000001")]).unwrap();
    let mut bytes = vec![0, 0, 1, 0];
    bytes.extend(quote(0, "000001", false, 0));
    truncations(&bytes, |b| decode_snapshots(&one, b, &metadata()).is_ok());
}
#[test]
fn refresh_xor_is_distinct_from_snapshot_and_depth_is_complete() {
    let request = RefreshRequest::from_stocks(&[(0, "000001")]).unwrap();
    let bytes = refresh_payload(10);
    let result = decode_quote_refresh(&request, &bytes, &metadata()).unwrap();
    assert_eq!(result.page.records[0].update_cursor, Some(10));
    assert_eq!(result.page.records[0].bids.len(), 5);
    assert_eq!(result.page.records[0].price, Some(12.34));
    truncations(&bytes, |b| {
        decode_quote_refresh(&request, b, &metadata()).is_ok()
    });
}
#[test]
fn category_tail_and_sort_contract_are_independent() {
    let request = CategoryQuotesRequest::new(
        QuoteCategory::AShares,
        QuoteSortField::Amount,
        CategoryQuoteOptions {
            ascending: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(&request.encode(1).unwrap()[20..22], &2u16.to_le_bytes());
    let mut bytes = vec![0, 0, 1, 0];
    bytes.extend(quote(0, "000001", false, 0));
    let mut tail = vec![0; 56];
    tail[2..4].copy_from_slice(&(-100i16).to_le_bytes());
    tail[54..56].copy_from_slice(&123u16.to_le_bytes());
    bytes.extend(tail);
    let result = decode_category_quotes(&request, &bytes, &metadata()).unwrap();
    assert_eq!(result.page.records[0].rise_speed, -1.0);
    assert_eq!(result.page.records[0].active2_raw, 123);
    truncations(&bytes, |b| {
        decode_category_quotes(&request, b, &metadata()).is_ok()
    });
    bytes[4] = 2;
    let raw = decode_category_quotes(&request, &bytes, &metadata()).unwrap();
    assert_eq!(raw.page.records[0].quote.market_raw, 2);
    assert!(raw.page.records[0].quote.price.is_none());
    assert!(raw.page.records[0].quote.scale.is_none());
    assert_eq!(raw.page.records[0].quote.price_fields_raw[0], 1234);
}
#[test]
fn money_flow_packed_buckets_allow_negative_amounts_and_keep_both_sets() {
    let request = MoneyFlowRequest::new(0, "000001").unwrap();
    let mut bytes = vec![0; 40];
    bytes[2..8].copy_from_slice(b"000001");
    bytes[38] = 1;
    let mut row = vec![0; 88];
    row[..4].copy_from_slice(&20261023u32.to_le_bytes());
    row[8..12].copy_from_slice(&100000f32.to_le_bytes());
    for (i, (a, b)) in [
        (20u16, 100u16),
        (300, 100),
        (10, 50),
        (200, 100),
        (60, 10),
        (30, 10),
        (20, 70),
        (70, 20),
    ]
    .into_iter()
    .enumerate()
    {
        let offset = 4 + (10 + i) * 4;
        row[offset..offset + 2].copy_from_slice(&a.to_le_bytes());
        row[offset + 2..offset + 4].copy_from_slice(&b.to_le_bytes());
    }
    bytes.extend(row);
    let block = decode_money_flow(&request, &bytes).unwrap();
    let r = &block.page.records[0];
    assert_eq!(r.main_net, -240.0);
    assert_eq!(r.main_ratio, -0.24);
    assert_eq!(r.active_buy_net, 740.0);
    truncations(&bytes, |b| decode_money_flow(&request, b).is_ok());
}
proptest::proptest! {
    #[test]
    fn malformed_payloads_never_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(),0..2048)) {
        let metadata=metadata();
        let _=decode_special_limits(&bytes);
        let _=decode_intraday_aux(&IntradayAuxRequest::new(0,"000001",IntradayAuxKind::BuySellStrength).unwrap(),&bytes);
        let _=decode_snapshots(&SnapshotsRequest::new(&[(0,"000001")]).unwrap(),&bytes,&metadata);
        let _=decode_quote_refresh(&RefreshRequest::from_stocks(&[(0,"000001")]).unwrap(),&bytes,&metadata);
        let _=decode_recent_minutes(&RecentMinutesRequest::new(0,"000001",20261023).unwrap(),&bytes,&metadata);
    }
}

#[test]
fn independently_verified_live_frames_match_all_owned_records() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/protocol_7709");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let decimals: Vec<_> = manifest["precision"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row[0].as_u64().unwrap() as u16,
                row[1].as_str().unwrap(),
                row[2].as_u64().unwrap() as u8,
            )
        })
        .collect();
    let precision = PrecisionSnapshot::from_decimals(&decimals, chrono::Utc::now(), None).unwrap();
    fn json<T: serde::Serialize>(rows: &T) -> serde_json::Value {
        // Use the same shortest f32 JSON representation as the captured DTO.
        serde_json::from_slice(&serde_json::to_vec(rows).unwrap()).unwrap()
    }
    let stocks = [(0, "000001"), (0, "159915"), (1, "510300")];
    for (name, fixture) in manifest["fixtures"].as_object().unwrap() {
        let read = |suffix: &str| std::fs::read(root.join(format!("{name}.{suffix}"))).unwrap();
        let payload = read("payload.bin");
        let header: [u8; 16] = read("header.bin").try_into().unwrap();
        assert_eq!(
            rustdx_complete::tcp::decode_frame(&header, &read("wire.bin"), true).unwrap(),
            payload
        );
        let opcode = u16::from_str_radix(
            fixture["opcode"].as_str().unwrap().trim_start_matches("0x"),
            16,
        )
        .unwrap();
        let records = match opcode {
            0x0452 => json(&decode_special_limits(&payload).unwrap().records),
            0x051b => json(
                &decode_intraday_aux(
                    &IntradayAuxRequest::new(
                        0,
                        "000001",
                        if name == "aux-11" {
                            IntradayAuxKind::VolumeComparison
                        } else {
                            IntradayAuxKind::BuySellStrength
                        },
                    )
                    .unwrap(),
                    &payload,
                )
                .unwrap()
                .page
                .records,
            ),
            0x0fd1 => json(
                &decode_sparkline(
                    &SparklineRequest::new(0, "000001", SparklineOptions::default()).unwrap(),
                    &payload,
                )
                .unwrap()
                .page
                .records,
            ),
            0x0feb => json(
                &decode_recent_minutes(
                    &RecentMinutesRequest::new(0, "000001", 20261023).unwrap(),
                    &payload,
                    &precision,
                )
                .unwrap()
                .page
                .records,
            ),
            0x056a => json(
                &decode_auction_series(
                    &AuctionSeriesRequest::new(0, "000001", None, AuctionOptions::default())
                        .unwrap(),
                    &payload,
                )
                .unwrap()
                .page
                .records,
            ),
            0x0ffc => json(
                &decode_money_flow(&MoneyFlowRequest::new(0, "000001").unwrap(), &payload)
                    .unwrap()
                    .page
                    .records,
            ),
            0x054c => json(
                &decode_snapshots(
                    &SnapshotsRequest::new(&stocks).unwrap(),
                    &payload,
                    &precision,
                )
                .unwrap()
                .page
                .records,
            ),
            0x054b => json(
                &decode_category_quotes(
                    &CategoryQuotesRequest::new(
                        QuoteCategory::AShares,
                        QuoteSortField::Amount,
                        CategoryQuoteOptions {
                            count: 5,
                            ascending: name == "category-true",
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                    &payload,
                    &precision,
                )
                .unwrap()
                .page
                .records,
            ),
            0x0547 => json(
                &decode_quote_refresh(
                    &RefreshRequest::from_stocks(&stocks).unwrap(),
                    &payload,
                    &precision,
                )
                .unwrap()
                .page
                .records,
            ),
            _ => panic!("unexpected fixture"),
        };
        let expected: serde_json::Value = serde_json::from_slice(&read("expected.json")).unwrap();
        assert_eq!(records, expected["records"], "{name}");
    }
}
