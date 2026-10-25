//! Bounded live 0feb/056a sampling with dated 052d and 0fb4 controls.
//! cargo run --example validate_recent_auction -- <new-dir> <ip:port> [--archive|--date-selector-check]
//! No response without an echoed date is promoted to an observed trading date.
use chrono::{Datelike, NaiveDate, Utc};
use rustdx_complete::{
    calendar::TradingCalendar,
    operation::{OperationContext, sleep},
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use serde_json::{Value, json};
use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const STOCKS: [(u16, &str); 6] = [
    (0, "000001"),
    (0, "000988"),
    (0, "159915"),
    (1, "600000"),
    (1, "600519"),
    (1, "510300"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureMode {
    Default,
    Archive,
    DateSelector,
}

#[derive(Debug)]
struct Capture {
    output: PathBuf,
    frames: Mutex<(usize, usize)>,
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
            let payload = decode_frame(header, body, true)?;
            let mut state = self
                .frames
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            let bytes = request.len() + header.len() + body.len() + payload.len();
            if state.0 >= 512 || bytes > (64 * 1024 * 1024usize).saturating_sub(state.1) {
                return Err(io::Error::other("live evidence budget exhausted"));
            }
            let stem = self.output.join(format!("frame-{:03}", state.0));
            for (extension, data) in [
                ("request.bin", request),
                ("header.bin", header.as_slice()),
                ("wire.bin", body),
                ("payload.bin", payload.as_slice()),
            ] {
                std::fs::write(stem.with_extension(extension), data)?;
            }
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            std::fs::write(stem.with_extension("utc.txt"), Utc::now().to_rfc3339())?;
            state.0 += 1;
            state.1 += bytes;
        }
        Ok(())
    }
}

fn save(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)
}

fn day_number(day: NaiveDate) -> u32 {
    day.year() as u32 * 10_000 + day.month() * 100 + day.day()
}

fn record<T: serde::Serialize>(
    output: &Path,
    name: &str,
    result: io::Result<T>,
    summary: &mut Value,
    tcp: &mut Tcp,
) -> io::Result<()> {
    let status = match result {
        Ok(value) => {
            save(&output.join(format!("{name}.json")), &value)?;
            json!({"name": name, "decoded": true, "connection_id": tcp.connection_id().get()})
        }
        Err(error) => json!({
            "name": name, "decoded": false, "kind": format!("{:?}", error.kind()),
            "error": error.to_string(), "connection_id": tcp.connection_id().get(),
        }),
    };
    println!("{status}");
    summary["queries"]
        .as_array_mut()
        .ok_or_else(|| io::Error::other("invalid query manifest"))?
        .push(status);
    save(&output.join("summary.json"), summary)?;
    sleep(Duration::from_millis(250))?;
    if !tcp.is_reusable() {
        tcp.reconnect()?;
        tcp.set_heartbeat_interval(None)?;
    }
    Ok(())
}

fn capture(
    output: &Path,
    endpoint: SocketAddr,
    mode: CaptureMode,
    summary: &mut Value,
) -> io::Result<()> {
    let operation = OperationContext::new(Duration::from_secs(180), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    save(&output.join("daily-pool.json"), &pool.refresh()?)?;
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(3),
        auto_reconnect: 0,
        ..Default::default()
    };
    let observer = Arc::new(Capture {
        output: output.to_owned(),
        frames: Mutex::new((0, 0)),
    });
    let mut tcp = Tcp::with_config_and_observer(&config, observer.clone())?;
    tcp.set_heartbeat_interval(None)?;
    let mut decimals = Vec::new();
    for market in [0, 1] {
        let rows = security_directory(&mut tcp, market)?;
        for &(m, code) in STOCKS.iter().filter(|(m, _)| *m == market) {
            let row = rows.iter().find(|r| r.code == code).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "validation symbol absent from directory",
                )
            })?;
            decimals.push((m, code, row.decimal));
        }
        save(&output.join(format!("directory-{market}.json")), &rows)?;
    }
    save(&output.join("precision.json"), &decimals)?;
    let metadata = PrecisionSnapshot::from_decimals(&decimals, Utc::now(), Some(endpoint))?;
    let today = Utc::now()
        .with_timezone(&rustdx_complete::data::exchange_timezone())
        .date_naive();
    let mut dates = Vec::new();
    let mut day = today;
    while dates.len() < 10 {
        day = day
            .pred_opt()
            .ok_or_else(|| io::Error::other("date underflow"))?;
        if TradingCalendar::try_is_trading_day(&day).map_err(io::Error::other)? {
            dates.push(day_number(day));
        }
    }
    let next = TradingCalendar::try_next_trading_day(&today).map_err(io::Error::other)?;
    summary["recent_dates"] = json!(dates);
    let mut recent_dates = dates.clone();
    recent_dates.extend([day_number(today), 20260817]);
    summary["recent_dates"] = json!(recent_dates);
    let mut auction_dates: Vec<_> = dates.iter().take(5).copied().map(Some).collect();
    auction_dates.extend([None, Some(day_number(today)), Some(20260814)]);
    auction_dates.push(Some(day_number(next)));
    if mode == CaptureMode::Archive {
        recent_dates = vec![20260814, 20260817, 20260818];
        auction_dates = recent_dates.iter().copied().map(Some).collect();
    }
    if mode == CaptureMode::DateSelector {
        recent_dates = vec![
            20260609, 20260610, 20260817, 20260818, 20261022, 20261023, 20261024, 20261026,
        ];
        auction_dates.clear();
    }
    let control_dates = if mode != CaptureMode::Default {
        recent_dates.clone()
    } else {
        dates.iter().take(2).copied().collect()
    };
    summary["recent_dates"] = json!(recent_dates);
    summary["auction_dates"] = json!(auction_dates);
    summary["control_dates"] = json!(control_dates);
    macro_rules! query {
        ($name:expr, $result:expr) => {
            record(output, &$name, $result, summary, &mut tcp)?
        };
    }
    query!("heartbeat", tcp.heartbeat());
    query!(
        "snapshots",
        SnapshotsRequest::new(&STOCKS)?.recv(&mut tcp, &metadata)
    );
    for &(market, code) in &STOCKS {
        query!(
            format!("bars-{market}-{code}"),
            KlineRequest::new(
                market,
                code,
                9,
                0,
                match mode {
                    CaptureMode::Default => 12,
                    CaptureMode::Archive => 80,
                    CaptureMode::DateSelector => 160,
                },
            )?
            .recv_stock(&mut tcp)
        );
        for &date in &control_dates {
            let request = HistoryMinuteTime::new(market, code, date);
            // Keep raw 0fb4 controls without the legacy fixed /100 parser.
            query!(
                format!("history-minute-{date}-{market}-{code}"),
                rustdx_complete::tcp::send_recv_decompress(&mut tcp, &request.send, "0fb4 control")
                    .map(|bytes| json!({
                        "opcode": "0x0fb4", "requested_date": date,
                        "market": market, "code": code, "payload": bytes,
                    }))
            );
        }
        for &date in &recent_dates {
            query!(
                format!("recent-{date}-{market}-{code}"),
                RecentMinutesRequest::new(market, code, date)?.recv(&mut tcp, &metadata)
            );
        }
        for &date in &auction_dates {
            query!(
                format!("auction-{}-{market}-{code}", date.unwrap_or(0)),
                AuctionSeriesRequest::new(market, code, date, AuctionOptions::default())?
                    .recv(&mut tcp)
            );
        }
    }
    let state = observer
        .frames
        .lock()
        .map_err(|_| io::Error::other("capture lock"))?;
    summary["wire_frames"] = json!(state.0);
    summary["archive_frame_bytes"] = json!(state.1);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("new output directory required")?);
    let endpoint: SocketAddr = args.next().ok_or("endpoint required")?.parse()?;
    let mode = match args.next().as_deref() {
        Some("--archive") => CaptureMode::Archive,
        Some("--date-selector-check") => CaptureMode::DateSelector,
        None => CaptureMode::Default,
        Some(_) => return Err("expected --archive or --date-selector-check".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    std::fs::create_dir(&output)?;
    let mut summary = json!({
        "schema_version": 1, "kind": "live_recent_auction_contract_validation",
        "source": "live_tcp", "endpoint": endpoint, "started_utc": Utc::now(),
        "archive_focus": mode == CaptureMode::Archive,
        "date_selector_check": mode == CaptureMode::DateSelector,
        "recent_minutes_date_schema_version": RECENT_MINUTES_DATE_SCHEMA_VERSION,
        "symbols": STOCKS, "request_spacing_ms": 250,
        "total_timeout_seconds": 180, "automatic_heartbeat": false,
        "queries": [], "status": "running",
        "date_policy": "Requested dates are not server date evidence",
    });
    save(&output.join("summary.json"), &summary)?;
    match capture(&output, endpoint, mode, &mut summary) {
        Ok(()) => summary["status"] = json!("captured"),
        Err(error) => {
            summary["status"] = json!("capture_error");
            summary["error"] = json!({
                "kind": format!("{:?}", error.kind()), "message": error.to_string(),
            });
        }
    }
    summary["finished_utc"] = json!(Utc::now());
    save(&output.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if summary["status"] != "captured" {
        std::process::exit(2);
    }
    Ok(())
}
