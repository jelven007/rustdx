//! Bounded 0feb nonempty sampling. Exit 2 means no accepted sample, never success.
//! cargo run --example capture_recent_minutes_boundaries -- <new-dir> <ip:port> [YYYYMMDD]
use chrono::{Datelike, Utc};
use rustdx_complete::{
    calendar::{BuiltinCalendar, CalendarProvider},
    operation::OperationContext,
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const STOCKS: [(u16, &str); 3] = [(0, "000001"), (0, "159915"), (1, "510300")];

fn save(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

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
            let decoded = decode_frame(header, body, true)?;
            let mut budget = self
                .budget
                .lock()
                .map_err(|_| io::Error::other("archive lock"))?;
            let bytes = request.len() + header.len() + body.len() + decoded.len();
            if budget.0 >= 128 || bytes > (16 * 1024 * 1024usize).saturating_sub(budget.1) {
                return Err(io::Error::other("recent-minute archive budget exhausted"));
            }
            let stem = self.output.join(format!("frame-{:03}", budget.0));
            std::fs::write(stem.with_extension("request.bin"), request)?;
            std::fs::write(stem.with_extension("header.bin"), header)?;
            std::fs::write(stem.with_extension("wire.bin"), body)?;
            std::fs::write(stem.with_extension("payload.bin"), decoded)?;
            save(
                &stem.with_extension("source.json"),
                &json!({
                    "source": "live_tcp", "synthetic": false, "endpoint": endpoint,
                    "received_at": Utc::now(),
                }),
            )?;
            budget.0 += 1;
            budget.1 += bytes;
        }
        Ok(())
    }
}

fn sample(
    output: &Path,
    endpoint: SocketAddr,
    date: Option<u32>,
    summary: &mut Value,
) -> io::Result<()> {
    let dates = match date {
        Some(date) => {
            RecentMinutesRequest::new(0, "000001", date)?;
            vec![date]
        }
        None => {
            let today = Utc::now()
                .with_timezone(&rustdx_complete::data::exchange_timezone())
                .date_naive();
            BuiltinCalendar
                .get_trading_days_before(today, 5)
                .map_err(io::Error::other)?
                .into_iter()
                .map(|day| day.year() as u32 * 10000 + day.month() * 100 + day.day())
                .collect()
        }
    };
    summary["dates"] = json!(dates);
    let operation = OperationContext::new(Duration::from_secs(180), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    save(&output.join("daily-pool.json"), &pool.refresh()?)?;
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(4),
        auto_reconnect: 0,
        ..Default::default()
    };
    let archive = Arc::new(Capture {
        output: output.into(),
        budget: Mutex::new((0, 0)),
    });
    let mut tcp = Tcp::with_config_and_observer(&config, archive)?;
    tcp.set_heartbeat_interval(None)?;
    let connection = tcp.connection_id().get();
    summary["connection_id"] = json!(connection);
    let mut precision = Vec::new();
    for market in [0, 1] {
        let rows = security_directory(&mut tcp, market)?;
        for &(m, code) in STOCKS.iter().filter(|(m, _)| *m == market) {
            let row = rows
                .iter()
                .find(|row| row.code == code)
                .ok_or_else(|| io::Error::other("sample symbol missing from checked directory"))?;
            precision.push((m, code, row.decimal));
        }
    }
    save(&output.join("precision.json"), &precision)?;
    let metadata = PrecisionSnapshot::from_decimals(&precision, Utc::now(), Some(endpoint))?;
    let mut covered = BTreeSet::new();
    let mut results = Vec::new();
    for date in dates {
        for (market, code) in STOCKS {
            let query = RecentMinutesRequest::new(market, code, date)?;
            let series = query.recv(&mut tcp, &metadata)?;
            let file = format!("recent-{date}-{market}-{code}.json");
            save(&output.join(&file), &series)?;
            let nonempty = !series.page.records.is_empty();
            if nonempty {
                covered.insert((market, code));
            }
            results.push(json!({
                "requested_date": date, "market": market, "code": code,
                "file": file, "points": series.page.records.len(), "nonempty": nonempty,
                "actual_server_date_verified": false,
            }));
            println!(
                "{date} {market}:{code}: {} points",
                series.page.records.len()
            );
            summary["results"] = json!(results);
            summary["nonempty_symbols"] = json!(covered);
            save(&output.join("summary.json"), summary)?;
            rustdx_complete::operation::sleep(Duration::from_millis(200))?;
        }
    }
    summary["capture_complete"] = json!(true);
    summary["status"] = json!(if covered.len() == STOCKS.len() {
        "nonempty_captured_requires_independent_verification"
    } else {
        "nonempty_not_observed_for_all_symbols"
    });
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("new directory required")?);
    let endpoint: SocketAddr = args.next().ok_or("endpoint required")?.parse()?;
    let date = args.next().map(|value| value.parse::<u32>()).transpose()?;
    if args.next().is_some() {
        return Err(
            "usage: capture_recent_minutes_boundaries <new-dir> <ip:port> [YYYYMMDD]".into(),
        );
    }
    std::fs::create_dir(&output)?;
    let mut summary = json!({
        "schema": 1, "opcode": "0x0feb", "source": "live_tcp", "synthetic": false,
        "recent_minutes_date_schema_version": RECENT_MINUTES_DATE_SCHEMA_VERSION,
        "endpoint": endpoint, "started_utc": Utc::now(), "symbols": STOCKS,
        "capture_complete": false, "status": "starting",
        "scope": "Nonempty samples only, no server trading-date or exchange-completeness proof",
        "criteria": "each stock/ETF has at least one nonempty response, followed by independent byte/field verification",
    });
    if let Err(error) = sample(&output, endpoint, date, &mut summary) {
        summary["status"] = json!("capture_error");
        summary["error"] =
            json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()});
    }
    summary["finished_utc"] = json!(Utc::now());
    save(&output.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if summary["status"] != "nonempty_captured_requires_independent_verification" {
        std::process::exit(2);
    }
    Ok(())
}
