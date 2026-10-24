//! `cargo run --example transaction_all -- today [IP:PORT]`
//! `cargo run --example transaction_all -- YYYYMMDD [IP:PORT]`
//! Queries sz000001 and prints provenance/page metadata, not classified totals.
use rustdx_complete::tcp::{
    TcpConfig,
    ip::IpPool,
    stock::{Client, TransactionPaginationOptions},
};
use std::{io, net::SocketAddr, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let date = args.next().unwrap_or_else(|| "today".into());
    let endpoint = args
        .next()
        .map(|value| value.parse::<SocketAddr>())
        .transpose()?;
    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: transaction_all today|YYYYMMDD [IP:PORT]",
        )
        .into());
    }
    let config = TcpConfig {
        ip: endpoint,
        ip_pool: endpoint
            .map(|address| IpPool::new(vec![address]))
            .transpose()?,
        timeout: Duration::from_secs(3),
        recheck_empty: true,
        ..Default::default()
    };
    let mut client = Client::with_config(&config)?;
    let options = TransactionPaginationOptions::default();
    let all = if date == "today" {
        client.transaction_all(0, "000001", options)?
    } else {
        client.history_transaction_all(0, "000001", date.parse()?, options)?
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "command": format!("0x{:04x}", all.command),
            "market": all.market,
            "code": all.code,
            "requested_date": all.requested_date,
            "observed_date": all.observed_date,
            "source_endpoint": all.source_endpoint,
            "connection_generation": all.connection_generation,
            "started_at": all.started_at,
            "fetched_at": all.fetched_at,
            "termination": all.termination,
            "raw_records": all.records.len(),
            "first": all.records.first(),
            "last": all.records.last(),
            "requests": all.requests,
            "payload_bytes": all.payload_bytes,
            "pages": all.pages,
            "coverage_note": "Source enumeration only; trading date and daily coverage are not independently verified.",
        }))?
    );
    Ok(())
}
