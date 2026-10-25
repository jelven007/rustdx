//! Low-rate capture of 0004/044d/0537, with raw frames and 053e cross-checks.
//! cargo run --example capture_modern_commands -- <new-dir> <ip:port>
use rustdx_complete::{
    operation::OperationContext,
    tcp::{
        FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame,
        ip::IpPool,
        stock::{PrecisionSnapshot, QuotesRequest, TodayMinuteRequest, security_directory},
    },
};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug)]
struct Capture {
    output: PathBuf,
    budget: Mutex<(usize, usize)>,
}

impl FrameObserver for Capture {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        if let FrameEvent::Response {
            endpoint,
            request,
            header,
            body,
        } = event
        {
            let mut budget = self
                .budget
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            if budget.0 >= 128 || budget.1 + body.len() > 8 * 1024 * 1024 {
                return Err(io::Error::other("capture budget exhausted"));
            }
            let stem = self.output.join(format!("frame-{:03}", budget.0));
            std::fs::write(stem.with_extension("request.bin"), request)?;
            std::fs::write(stem.with_extension("header.bin"), header)?;
            std::fs::write(stem.with_extension("wire.bin"), body)?;
            std::fs::write(
                stem.with_extension("payload.bin"),
                decode_frame(header, body, true)?,
            )?;
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            budget.0 += 1;
            budget.1 += body.len();
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("missing new directory")?);
    let endpoint = args.next().ok_or("missing IP:port")?.parse()?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(120), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    let daily = pool.refresh()?;
    std::fs::write(
        output.join("daily-pool.json"),
        serde_json::to_vec_pretty(&daily)?,
    )?;
    let observer = Arc::new(Capture {
        output: output.clone(),
        budget: Mutex::new((0, 0)),
    });
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(5),
        ..Default::default()
    };
    let mut tcp = Tcp::with_config_and_observer(&config, observer)?;
    let heartbeat = tcp.heartbeat()?;
    std::fs::write(
        output.join("heartbeat.json"),
        serde_json::to_vec_pretty(&heartbeat)?,
    )?;
    let mut inputs = Vec::new();
    for market in [0, 1] {
        let rows = security_directory(&mut tcp, market)?;
        std::fs::write(
            output.join(format!("directory-{market}.json")),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        inputs.extend(rows.into_iter().map(|row| (market, row.code, row.decimal)));
    }
    let decimals: Vec<_> = inputs
        .iter()
        .map(|(m, c, d)| (*m, c.as_str(), *d))
        .collect();
    let metadata = PrecisionSnapshot::from_decimals(&decimals, chrono::Utc::now(), Some(endpoint))?;
    let symbols = [(0, "000001"), (0, "159915"), (1, "510300")];
    let mut summaries = Vec::new();
    for &(market, code) in &symbols {
        std::thread::sleep(Duration::from_millis(100));
        let series = TodayMinuteRequest::new(market, code)?.recv(&mut tcp, &metadata)?;
        std::fs::write(
            output.join(format!("minutes-{market}-{code}.json")),
            serde_json::to_vec_pretty(&series)?,
        )?;
        summaries.push(serde_json::json!({
            "market": market, "code": code, "decimal": series.scale.decimal(),
            "count": series.points.len(),
            "first_price": series.points.first().map(|p| p.price),
            "last_price": series.points.last().map(|p| p.price),
            "last_average": series.points.last().map(|p| p.average_price),
            "trading_date": series.trading_date,
        }));
    }
    let quotes = QuotesRequest::new(symbols.to_vec())?.recv_with_metadata(&mut tcp, &metadata)?;
    std::fs::write(
        output.join("quotes.json"),
        serde_json::to_vec_pretty(&quotes)?,
    )?;
    let summary = serde_json::json!({
        "endpoint": endpoint, "captured_at": chrono::Utc::now(), "heartbeat": heartbeat,
        "directory_counts": metadata.status(), "minutes": summaries,
        "scope": "one endpoint, closed-session sample; date-less 0537 does not verify a trading day"
    });
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
