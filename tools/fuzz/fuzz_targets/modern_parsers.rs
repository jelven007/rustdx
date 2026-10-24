#![no_main]
use libfuzzer_sys::fuzz_target;
use rustdx_complete::tcp::{decode_frame, stock::*};

fuzz_target!(|input: &[u8]| {
    let Some((&selector, data)) = input.split_first() else {
        return;
    };
    if data.len() > 65_535 {
        return;
    }
    let precision = PrecisionSnapshot::from_decimals(
        &[
            (0, "000001", 3),
            (0, "159915", 3),
            (1, "510300", 3),
            (1, "600000", 4),
        ],
        std::time::UNIX_EPOCH.into(),
        None,
    )
    .expect("constant precision");
    match selector % 17 {
        0 => {
            let _ = decode_security_directory(
                &SecurityDirectoryRequest::new(0, 0, 1600).unwrap(),
                data,
            );
        }
        1 => {
            let _ = decode_company_info_directory(
                &CompanyInfoDirectoryRequest::new(0, "000001").unwrap(),
                data,
            );
        }
        2 => {
            let _ = decode_company_info_text(
                &CompanyInfoTextRequest::new(0, "000001", "000001.txt", 0, 60_000).unwrap(),
                data,
            );
        }
        3 => {
            let _ = decode_block_metadata(data);
        }
        4 => {
            let _ = decode_delisted_bars(
                &DelistedBarsRequest::new(0, "000001", 0, 800).unwrap(),
                data,
            );
        }
        5 => {
            let _ = decode_intraday_aux(
                &IntradayAuxRequest::new(0, "000001", IntradayAuxKind::BuySellStrength).unwrap(),
                data,
            );
        }
        6 => {
            let _ = decode_intraday_aux(
                &IntradayAuxRequest::new(0, "000001", IntradayAuxKind::VolumeComparison).unwrap(),
                data,
            );
        }
        7 => {
            let _ = decode_sparkline(
                &SparklineRequest::new(0, "000001", SparklineOptions::default()).unwrap(),
                data,
            );
        }
        8 => {
            let _ = decode_recent_minutes(
                &RecentMinutesRequest::new(0, "000001", 20261022).unwrap(),
                data,
                &precision,
            );
        }
        9 => {
            let _ = decode_auction_series(
                &AuctionSeriesRequest::new(0, "000001", Some(20261022), AuctionOptions::default())
                    .unwrap(),
                data,
            );
        }
        10 => {
            let _ = decode_money_flow(&MoneyFlowRequest::new(0, "000001").unwrap(), data);
        }
        11 => {
            let _ = decode_snapshots(
                &SnapshotsRequest::new(&[
                    (0, "000001"),
                    (0, "159915"),
                    (1, "510300"),
                    (1, "600000"),
                ])
                .unwrap(),
                data,
                &precision,
            );
        }
        12 => {
            let _ = decode_category_quotes(
                &CategoryQuotesRequest::new(
                    QuoteCategory::AShares,
                    QuoteSortField::Code,
                    CategoryQuoteOptions::default(),
                )
                .unwrap(),
                data,
                &precision,
            );
        }
        13 => {
            let _ = decode_quote_refresh(
                &RefreshRequest::from_stocks(&[
                    (0, "000001"),
                    (0, "159915"),
                    (1, "510300"),
                    (1, "600000"),
                ])
                .unwrap(),
                data,
                &precision,
            );
        }
        14 => {
            let _ = decode_special_limits(data);
        }
        15 => {
            let _ = decode_today_minutes(
                &TodayMinuteRequest::new(0, "000001").unwrap(),
                data,
                &precision,
            );
        }
        _ => {
            if let Some(header) = data.get(..16).and_then(|bytes| bytes.try_into().ok()) {
                let _ = decode_frame(&header, &data[16..], true);
            }
        }
    }
});
