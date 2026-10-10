use rustdx_complete::tcp::{
    TcpConfig,
    stock::{Adj, Client},
};
use std::time::Duration;

fn require(label: &str, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    if count == 0 {
        return Err(format!("{label} returned no data").into());
    }
    println!("{label}: {count}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TcpConfig {
        timeout: Duration::from_secs(5),
        auto_reconnect: 2,
        retry_delay_ms: 200,
        recheck_empty: true,
        ..Default::default()
    };
    let mut client = Client::with_config(&config)?;

    println!("heartbeat security count: {}", client.heartbeat()?);
    require(
        "quotes",
        client.quotes(&[(0, "000001"), (1, "600519")])?.len(),
    )?;
    require(
        "stock daily bars",
        client.k(1, "600519", Some(20260901), Some(20261009))?.len(),
    )?;
    require(
        "index daily bars",
        client
            .index_bars_range(1, "000001", 9, Some(20260901), Some(20261009))?
            .len(),
    )?;
    require(
        "adjusted daily bars",
        client
            .k_adjusted(1, "600519", Adj::Qfq, Some(20260101), Some(20261009))?
            .len(),
    )?;
    require("minute", client.minute(0, "000001")?.len())?;
    require(
        "history minute",
        client.history_minute(0, "000001", 20261009)?.len(),
    )?;
    println!(
        "current transaction (may be empty outside a trading session): {}",
        client.transaction(0, "000001", 0, 20)?.len()
    );
    require(
        "history transaction",
        client
            .history_transaction(0, "000001", 0, 20, 20261009)?
            .len(),
    )?;

    let finance = client.finance(1, "600519")?;
    println!(
        "finance: code={} updated={} net_assets={}",
        finance.code, finance.updated_date, finance.jingzichan
    );
    require("corporate actions", client.xdxr(1, "600519")?.len())?;

    for market in [0, 1] {
        println!(
            "market {market}: count={}, listed={}",
            client.stock_count(market)?,
            client.stocks(market)?.len()
        );
    }
    require("concept block records", client.block("block_gn.dat")?.len())?;
    require("F10 categories", client.f10_categories(1, "600519")?.len())?;
    require("F10 contents", client.f10(1, "600519")?.len())?;
    Ok(())
}
