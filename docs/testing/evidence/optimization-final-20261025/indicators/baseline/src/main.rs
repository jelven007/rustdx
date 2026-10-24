//! Shared batch API harness, compiled unchanged against each source snapshot.
#[allow(dead_code, unused_imports)]
mod indicators;

use std::{hint::black_box, time::Instant};

fn hash(value: impl std::fmt::Debug) -> String {
    let digest = format!("{value:?}")
        .bytes()
        .fold(0xcbf29ce484222325_u64, |n, b| {
            (n ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
    format!("{digest:016x}")
}

fn measurement<T: std::fmt::Debug>(
    name: &str,
    rows: usize,
    period: usize,
    mut work: impl FnMut() -> T,
) -> serde_json::Value {
    black_box(work());
    let start = Instant::now();
    black_box(work());
    let elapsed = start.elapsed().as_nanos();
    // Check the complete output outside the timed interval.
    serde_json::json!({
        "name": name, "rows": rows, "period": period,
        "elapsed_ns": elapsed, "result_hash": hash(work()),
    })
}

fn main() {
    let input = std::fs::read(std::env::args().nth(1).expect("input path")).expect("input");
    assert_eq!(input.len(), 100_000 * 24);
    let mut values = Vec::new();
    let mut highs = Vec::new();
    let mut lows = Vec::new();
    for row in input.as_chunks::<24>().0 {
        values.push(f64::from_le_bytes(row[0..8].try_into().unwrap()));
        highs.push(f64::from_le_bytes(row[8..16].try_into().unwrap()));
        lows.push(f64::from_le_bytes(row[16..24].try_into().unwrap()));
    }
    let mut measurements = Vec::new();
    for rows in [10_000, 100_000] {
        let (v, h, l) = (&values[..rows], &highs[..rows], &lows[..rows]);
        for period in [20, 200] {
            measurements.push(measurement("sma_rsi", rows, period, || {
                (
                    indicators::sma(black_box(v), period),
                    indicators::rsi(black_box(v), period),
                )
            }));
            measurements.push(measurement("bands_kdj", rows, period, || {
                (
                    indicators::bollinger_bands(black_box(v), period, 2.0),
                    indicators::kdj(black_box(h), black_box(l), black_box(v), period, 3, 3),
                )
            }));
        }
        measurements.push(measurement("ema_macd_batch", rows, 0, || {
            (
                indicators::ema(black_box(v), 12),
                indicators::macd(black_box(v), 12, 26, 9),
            )
        }));
    }
    println!("{}", serde_json::json!(measurements));
}
