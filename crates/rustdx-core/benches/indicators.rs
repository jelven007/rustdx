use rustdx_complete::indicators::{
    MacdParameters, MacdState, bollinger_bands, ema, kdj, macd, rsi, sma,
};
use std::{hint::black_box, time::Instant};

fn sample(mut work: impl FnMut()) -> u128 {
    work(); // warm caches before taking five samples
    let mut samples: Vec<_> = (0..5)
        .map(|_| {
            let start = Instant::now();
            work();
            start.elapsed().as_nanos()
        })
        .collect();
    samples.sort_unstable();
    samples[2]
}

fn main() {
    let mut measurements = Vec::new();
    for rows in [10_000, 100_000, 1_000_000] {
        let values: Vec<_> = (0..rows).map(|i| 10.0 + (i as f64 * 0.1).sin()).collect();
        let highs: Vec<_> = values.iter().map(|v| v + 1.0).collect();
        let lows: Vec<_> = values.iter().map(|v| v - 1.0).collect();
        for period in [20, 200, 2000] {
            let sma_rsi = sample(|| {
                black_box(sma(black_box(&values), period));
                black_box(rsi(black_box(&values), period));
            });
            let bands_kdj = sample(|| {
                black_box(bollinger_bands(black_box(&values), period, 2.0));
                black_box(kdj(
                    black_box(&highs),
                    black_box(&lows),
                    black_box(&values),
                    period,
                    3,
                    3,
                ));
            });
            for (name, elapsed) in [("sma_rsi", sma_rsi), ("bands_kdj", bands_kdj)] {
                measurements.push(serde_json::json!({
                    "name": name, "rows": rows, "period": period, "median_ns": elapsed,
                }));
            }
        }
        let batch = sample(|| {
            black_box(ema(black_box(&values), 12));
            black_box(macd(black_box(&values), 12, 26, 9));
        });
        let stream = sample(|| {
            let mut state = MacdState::new(MacdParameters::default()).unwrap();
            for &value in black_box(&values) {
                black_box(state.update(Some(value)));
            }
        });
        for (name, elapsed) in [("ema_macd_batch", batch), ("macd_stream", stream)] {
            measurements.push(serde_json::json!({
                "name": name, "rows": rows, "period": 0, "median_ns": elapsed,
            }));
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "samples_per_measurement": 5, "statistic": "median", "synthetic": true,
            "measurements": measurements,
        })
    );
}
