//! Compare a captured, closed Eastmoney CSV against independently fetched TDX bars.
use rustdx_complete::{
    data::Symbol,
    tcp::{TcpConfig, stock::Client},
};
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
struct Snapshot {
    date: String,
    code: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    amount: f64,
    vol: f64,
}

fn main() -> eyre::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("usage: source_crosscheck <closed-east.csv>"))?;
    let mut client = Client::with_config(&TcpConfig {
        timeout: Duration::from_secs(5),
        auto_reconnect: 2,
        retry_delay_ms: 200,
        recheck_empty: true,
        ..Default::default()
    })?;
    let mut checked = 0;
    for row in csv::Reader::from_path(path)?
        .deserialize::<Snapshot>()
        .take(100)
    {
        let row = row?;
        let symbol: Symbol = row.code.parse()?;
        let date = chrono::NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")?;
        use chrono::Datelike;
        let date = date.year() as u32 * 10000 + date.month() * 100 + date.day();
        let bars = client.k(
            symbol.market().tdx_market()?,
            symbol.code(),
            Some(date),
            Some(date),
        )?;
        let bar = bars
            .first()
            .filter(|_| bars.len() == 1)
            .ok_or_else(|| eyre::eyre!("{} {date}: expected one TDX bar", row.code))?;
        for (field, east, tdx) in [
            ("open", row.open, bar.open),
            ("high", row.high, bar.high),
            ("low", row.low, bar.low),
            ("close", row.close, bar.close),
        ] {
            if east != tdx {
                eyre::bail!(
                    "{} {date}: {field} differs: east={east}, tdx={tdx}",
                    row.code
                );
            }
        }
        // TDX encodes volume/amount with a 32-bit floating format. Bound its
        // quantization using two f32 relative ULPs, plus one lot/yuan.
        let volume_tolerance = (row.vol.abs() * f32::EPSILON as f64 * 2.).max(1.);
        let amount_tolerance = (row.amount.abs() * f32::EPSILON as f64 * 2.).max(1.);
        let volume_delta = (row.vol - bar.vol).abs();
        let amount_delta = (row.amount - bar.amount).abs();
        println!(
            "{} {date}: OHLC exact; volume Δ={volume_delta} ≤ {volume_tolerance} lots; amount Δ={amount_delta} ≤ {amount_tolerance} CNY",
            row.code
        );
        if volume_delta > volume_tolerance || amount_delta > amount_tolerance {
            eyre::bail!(
                "{}: volume/amount exceeds declared quantization bounds",
                row.code
            );
        }
        checked += 1;
    }
    if checked == 0 {
        eyre::bail!("no snapshot rows");
    }
    println!("independent source comparison passed: {checked} rows");
    Ok(())
}
