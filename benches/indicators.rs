use rustdx_complete::indicators::{bollinger_bands, kdj, rsi, sma};
use std::{hint::black_box, time::Instant};

fn main() {
    let values: Vec<_> = (0..100_000)
        .map(|i| 10.0 + (i as f64 * 0.1).sin())
        .collect();
    let highs: Vec<_> = values.iter().map(|v| v + 1.0).collect();
    let lows: Vec<_> = values.iter().map(|v| v - 1.0).collect();
    for period in [20, 200, 2000] {
        let start = Instant::now();
        for _ in 0..10 {
            black_box(sma(black_box(&values), period));
            black_box(rsi(black_box(&values), period));
        }
        println!(
            "100k rows, SMA+RSI period={period}, 10 iterations: {:?}",
            start.elapsed()
        );
        let start = Instant::now();
        for _ in 0..10 {
            black_box(bollinger_bands(black_box(&values), period, 2.0));
            black_box(kdj(
                black_box(&highs),
                black_box(&lows),
                black_box(&values),
                period,
                3,
                3,
            ));
        }
        println!(
            "100k rows, Bollinger+KDJ period={period}, 10 iterations: {:?}",
            start.elapsed()
        );
    }
}
