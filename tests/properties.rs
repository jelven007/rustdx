//! Offline properties spanning checked inputs and mathematical invariants.
use proptest::prelude::*;
use rustdx_complete::{
    bytes_helper::{into_arr2, into_arr4},
    file::{
        day::Day,
        gbbq::{Gbbq, Gbbqs, try_parse},
        lc::Lc,
    },
    indicators::{bollinger_bands, kdj},
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn checked_binary_entrypoints_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..256), pos in any::<usize>()) {
        let _ = into_arr2(&bytes, pos);
        let _ = into_arr4(&bytes, pos);
        let day = Day::try_from_bytes("sz000001", &bytes);
        let minute = Lc::try_from_bytes(1, &bytes);
        let event = Gbbq::try_from_chunk(&bytes);
        if bytes.len() != 32 { prop_assert!(day.is_err()); prop_assert!(minute.is_err()); }
        if bytes.len() != 29 { prop_assert!(event.is_err()); }
        let mut encrypted = bytes.clone();
        let result = try_parse(&mut encrypted);
        if bytes.len() != 29 { prop_assert!(result.is_err()); prop_assert_eq!(&encrypted, &bytes); }
        if let Ok(mut source) = Gbbqs::from_bytes(bytes) { let _ = source.try_to_vec(); }
    }

    #[test]
    fn bands_match_independent_two_pass_formula(
        prices in prop::collection::vec(1i32..100000, 2..256), period in 2usize..50,
    ) {
        let prices: Vec<_> = prices.iter().map(|v| *v as f64 / 100.0).collect();
        let bands = bollinger_bands(&prices, period, 2.0);
        prop_assert_eq!(bands.middle.len(), prices.len());
        for (index, window) in prices.windows(period).enumerate() {
            let mean = window.iter().sum::<f64>() / period as f64;
            let variance = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / period as f64;
            let index = index + period - 1;
            prop_assert!((bands.middle[index].unwrap() - mean).abs() < 1e-9);
            prop_assert!((bands.upper[index].unwrap() - mean - 2.0 * variance.sqrt()).abs() < 1e-8);
        }
    }

    #[test]
    fn kdj_is_invariant_to_positive_price_scale(
        prices in prop::collection::vec(100i32..10000, 1..256), period in 1usize..50, scale in 1u32..20,
    ) {
        let close: Vec<_> = prices.iter().map(|v| *v as f64).collect();
        let high: Vec<_> = close.iter().map(|v| v + 100.0).collect();
        let low: Vec<_> = close.iter().map(|v| v - 100.0).collect();
        let baseline = kdj(&high, &low, &close, period, 3, 3);
        let scaled = |data: &[f64]| data.iter().map(|v| v * scale as f64).collect::<Vec<_>>();
        let result = kdj(&scaled(&high), &scaled(&low), &scaled(&close), period, 3, 3);
        for (a, b) in baseline.k.iter().zip(result.k) {
            match (a, b) {
                (Some(a), Some(b)) => prop_assert!((a - b).abs() < 1e-10),
                (None, None) => {},
                _ => prop_assert!(false, "alignment changed"),
            }
        }
    }
}

#[test]
fn checked_gbbq_matches_legacy_on_real_fixture_and_decode_is_idempotent() {
    let bytes = std::fs::read("tests/fixtures/tdx/gbbq").unwrap();
    let mut safe = Gbbqs::from_bytes(bytes.clone()).unwrap();
    let mut legacy = Gbbqs::from_bytes(bytes).unwrap();
    let expected = format!("{:?}", legacy.to_vec());
    assert_eq!(format!("{:?}", safe.try_to_vec().unwrap()), expected);
    assert_eq!(format!("{:?}", safe.try_to_vec().unwrap()), expected);
}
