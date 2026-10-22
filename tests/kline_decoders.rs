//! Expectations come from a pinned independent pytdx decoder, not Rust encoding.
use rustdx_complete::tcp::{
    Tdx,
    helper::DateTime,
    stock::{IndexKline, Kline, KlineRequest, decode_index_bars, decode_stock_bars},
};
use serde::Deserialize;
use std::{io::ErrorKind, path::Path};

#[derive(Debug, Deserialize)]
struct Expected {
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    open: f64,
    close: f64,
    high: f64,
    low: f64,
    vol: f64,
    amount: f64,
    up_count: Option<u16>,
    down_count: Option<u16>,
}

fn fixture(name: &str, suffix: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/kline")
        .join(format!("{name}.{suffix}.hex"));
    let hex = std::fs::read_to_string(path).unwrap();
    hex.trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn check(expected: &Expected, dt: DateTime, values: [f64; 6]) {
    assert_eq!(
        dt,
        DateTime {
            year: expected.year,
            month: expected.month,
            day: expected.day,
            hour: expected.hour,
            minute: expected.minute,
        }
    );
    assert_eq!(
        values,
        [
            expected.open,
            expected.close,
            expected.high,
            expected.low,
            expected.vol,
            expected.amount,
        ]
    );
}

#[test]
fn independent_pytdx_requests_and_all_decoded_fields_match() {
    for (name, market, category) in [
        ("stock-day", 0, 9),
        ("index-day", 1, 9),
        ("stock-minute", 0, 0),
    ] {
        let request = KlineRequest::new(market, "000001", category, 0, 3).unwrap();
        assert_eq!(request.encode().as_slice(), fixture(name, "request"));
        let bytes = fixture(name, "response");
        let expected: Vec<Expected> = csv::Reader::from_path(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/kline")
                .join(format!("{name}.expected.csv")),
        )
        .unwrap()
        .deserialize()
        .collect::<Result<_, _>>()
        .unwrap();
        assert_eq!(expected.len(), 3);
        if name == "index-day" {
            let bars = decode_index_bars(&request, &bytes).unwrap();
            let mut legacy = IndexKline::from_request(request);
            legacy.try_parse(bytes.clone()).unwrap();
            assert_eq!(bars.len(), expected.len());
            for (i, bar) in bars.iter().enumerate() {
                assert_eq!(bar.code, "000001");
                check(
                    &expected[i],
                    bar.dt,
                    [bar.open, bar.close, bar.high, bar.low, bar.vol, bar.amount],
                );
                assert_eq!(Some(bar.up_count), expected[i].up_count);
                assert_eq!(Some(bar.down_count), expected[i].down_count);
                let old = &legacy.data[i];
                check(
                    &expected[i],
                    old.dt,
                    [old.open, old.close, old.high, old.low, old.vol, old.amount],
                );
                assert_eq!(old.up_count, bar.up_count);
                assert_eq!(old.down_count, bar.down_count);
            }
        } else {
            let bars = decode_stock_bars(&request, &bytes).unwrap();
            let mut legacy = Kline::from_request(request);
            legacy.try_parse(bytes.clone()).unwrap();
            assert_eq!(bars.len(), expected.len());
            for (i, bar) in bars.iter().enumerate() {
                assert_eq!(bar.code, "000001");
                check(
                    &expected[i],
                    bar.dt,
                    [bar.open, bar.close, bar.high, bar.low, bar.vol, bar.amount],
                );
                let old = &legacy.data[i];
                check(
                    &expected[i],
                    old.dt,
                    [old.open, old.close, old.high, old.low, old.vol, old.amount],
                );
            }
        }
    }
}

#[test]
fn malformed_responses_never_return_partial_or_stale_data() {
    for name in ["stock-day", "index-day", "stock-minute"] {
        let is_index = name == "index-day";
        let request = KlineRequest::new(
            u16::from(is_index),
            "000001",
            if name == "stock-minute" { 0 } else { 9 },
            0,
            3,
        )
        .unwrap();
        let bytes = fixture(name, "response");
        let decode = |bytes: &[u8]| {
            if is_index {
                decode_index_bars(&request, bytes).map(|bars| bars.len())
            } else {
                decode_stock_bars(&request, bytes).map(|bars| bars.len())
            }
        };
        for end in 0..bytes.len() {
            assert!(
                decode(&bytes[..end]).is_err(),
                "{name}: truncation at {end}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            decode(&trailing).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        let mut excess = bytes.clone();
        excess[..2].copy_from_slice(&4u16.to_le_bytes());
        assert_eq!(decode(&excess).unwrap_err().kind(), ErrorKind::InvalidData);
        assert_eq!(decode(&[0, 0]).unwrap(), 0);
        assert!(decode(&[0, 0, 1]).is_err());
        let mut invalid_date = bytes.clone();
        if name == "stock-minute" {
            invalid_date[4..6].copy_from_slice(&1440u16.to_le_bytes());
        } else {
            invalid_date[2..6].copy_from_slice(&20260230u32.to_le_bytes());
        }
        assert_eq!(
            decode(&invalid_date).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        if is_index {
            assert!(decode_stock_bars(&request, &bytes).is_err());
            let mut legacy = IndexKline::from_request(request);
            legacy.try_parse(bytes).unwrap();
            assert!(legacy.try_parse(trailing).is_err());
            assert!(legacy.data.is_empty());
        } else {
            assert!(decode_index_bars(&request, &bytes).is_err());
            let mut legacy = Kline::from_request(request);
            legacy.try_parse(bytes).unwrap();
            assert!(legacy.try_parse(trailing).is_err());
            assert!(legacy.data.is_empty());
        }
    }
    let request = KlineRequest::new(0, "000001", 9, 0, 3).unwrap();
    for packed in [20260001u32, 20261301, 655360001, u32::MAX] {
        let mut bytes = fixture("stock-day", "response");
        bytes[2..6].copy_from_slice(&packed.to_le_bytes());
        assert!(decode_stock_bars(&request, &bytes).is_err());
    }
}

#[test]
fn checked_request_rejects_inputs_without_panicking() {
    for code in ["", "12345", "0000010", "中文", "0000/1"] {
        assert!(KlineRequest::new(0, code, 9, 0, 3).is_err());
    }
    for count in [0, 801, u16::MAX] {
        assert!(KlineRequest::new(0, "000001", 9, 0, count).is_err());
    }
    assert!(KlineRequest::new(2, "000001", 9, 0, 3).is_err());
    assert!(KlineRequest::new(0, "000001", 12, 0, 3).is_err());
    assert!(KlineRequest::new(0, "200b07", 9, u16::MAX, 800).is_ok());
}
