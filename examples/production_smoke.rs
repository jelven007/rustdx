use chrono::Datelike;
use rustdx_complete::{
    calendar::{BuiltinCalendar, CalendarProvider},
    tcp::{
        TcpConfig,
        stock::{Adj, Client},
    },
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
    let now = chrono::Utc::now()
        .with_timezone(&rustdx_complete::data::exchange_timezone())
        .naive_local();
    let market_date = BuiltinCalendar.market_trading_day_at(now)?;
    let end = if market_date == now.date()
        && now.time() < chrono::NaiveTime::from_hms_opt(15, 0, 0).unwrap()
    {
        BuiltinCalendar.previous_trading_day(market_date)?
    } else {
        market_date
    };
    let as_u32 =
        |date: chrono::NaiveDate| date.year() as u32 * 10000 + date.month() * 100 + date.day();
    let begin = as_u32(end - chrono::Duration::days(60));
    let end = as_u32(end);
    println!("latest closed-session acceptance: {begin}..={end}");

    println!("heartbeat security count: {}", client.heartbeat()?);
    require(
        "quotes",
        client.quotes(&[(0, "000001"), (1, "600519")])?.len(),
    )?;
    let daily = client.k(1, "600519", Some(begin), Some(end))?;
    require("stock daily bars", daily.len())?;
    if daily.last().is_none_or(|bar| bar.dt.to_u32() != end) {
        return Err(format!("stock daily bars do not reach latest closed session {end}").into());
    }
    for bar in &daily {
        rustdx_complete::data::validate_ohlcv(
            bar.open, bar.high, bar.low, bar.close, bar.vol, bar.amount,
        )?;
    }
    println!(
        "stock daily bounds: {}..={}",
        daily[0].dt.to_u32(),
        daily.last().unwrap().dt.to_u32()
    );
    require(
        "index daily bars",
        client
            .index_bars_range(1, "000001", 9, Some(begin), Some(end))?
            .len(),
    )?;
    require(
        "adjusted daily bars",
        client
            .k_adjusted(
                1,
                "600519",
                Adj::Qfq,
                Some(end / 10000 * 10000 + 101),
                Some(end),
            )?
            .len(),
    )?;
    require("minute", client.minute(0, "000001")?.len())?;
    require(
        "history minute",
        client.history_minute(0, "000001", end)?.len(),
    )?;
    println!(
        "current transaction (may be empty outside a trading session): {}",
        client.transaction(0, "000001", 0, 20)?.len()
    );
    require(
        "history transaction",
        client.history_transaction(0, "000001", 0, 20, end)?.len(),
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
