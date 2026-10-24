//! Ordered, bounded stock-range delivery without collecting the full batch.
use rustdx_complete::tcp::{
    NetworkPolicy, TcpConfig, configure_network, startup,
    stock::{BarBatchStreamOptions, BarRangeOptions, BatchStreamOptions, Client},
};

fn main() -> std::io::Result<()> {
    configure_network(NetworkPolicy::default())?;
    let config = TcpConfig::default();
    startup::initialize_with_config(&config)?;
    let client = Client::with_config(&config)?;
    let stocks = [(0, "000001"), (1, "600000")];
    let summary = client.k_batch_stream(
        &stocks,
        Some(20250101),
        Some(20251231),
        BarBatchStreamOptions {
            range: BarRangeOptions {
                max_rows: 4096,
                max_bytes: 4 * 1024 * 1024,
                ..Default::default()
            },
            delivery: BatchStreamOptions {
                max_parallel: 2,
                max_rows: 8192,
                max_bytes: 8 * 1024 * 1024,
                max_item_rows: 4096,
                max_item_bytes: 4 * 1024 * 1024,
                ..Default::default()
            },
        },
        |item| {
            let range = item.result?;
            if !range.complete {
                return Err(std::io::Error::other(format!(
                    "incomplete range for ({}, {}): {:?}",
                    item.market, item.code, range.termination
                )));
            }
            // Consumers can write these records here, then release this item.
            println!(
                "{} ({}, {}): {} bars, {:?}, {:?}",
                item.input_index,
                item.market,
                item.code,
                range.records.len(),
                range.coverage,
                range.source
            );
            Ok(())
        },
    )?;
    println!("{summary:?}");
    Ok(())
}
