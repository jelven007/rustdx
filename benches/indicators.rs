use rustdx_complete::indicators::{rsi, sma};
use std::{hint::black_box, time::Instant};

fn main() {
    let values: Vec<_> = (0..100_000)
        .map(|i| 10.0 + (i as f64 * 0.1).sin())
        .collect();
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
    }
}
