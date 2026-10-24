use rustdx_complete::{
    data::{Bar, Market},
    tcp::{TcpConfig, stock::Client},
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TcpConfig {
        timeout: Duration::from_secs(5),
        auto_reconnect: 2,
        retry_delay_ms: 200,
        recheck_empty: true,
        ..Default::default()
    };
    let client = Client::with_config(&config)?;
    for result in client.k_batch(
        &[(1, "600000"), (0, "000001")],
        Some(20260901),
        Some(20260930),
        2,
    )? {
        let market = if result.market == 1 {
            Market::Shanghai
        } else {
            Market::Shenzhen
        };
        for raw in result.result? {
            let bar = Bar::from_kline(&raw, market)?;
            println!(
                "{} {} close={} volume_shares={}",
                bar.symbol, bar.datetime, bar.close, bar.volume_shares
            );
        }
    }
    Ok(())
}
