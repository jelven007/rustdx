//! Bounded single-page live capture with complete wire evidence.
//! cargo run --example capture_enhanced_history_transaction -- <new-dir> <ip:port> <market> <code> <YYYYMMDD> [start] [count]
use rustdx_complete::{
    operation::OperationContext,
    tcp::{
        FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame,
        ip::IpPool,
        stock::{EnhancedHistoryTransactionRequest, decode_enhanced_history_transactions},
    },
};
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug)]
struct Capture {
    directory: PathBuf,
    frames: Mutex<usize>,
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
            let mut frames = self
                .frames
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            if *frames >= 12 {
                return Err(io::Error::other("capture frame budget exhausted"));
            }
            let stem = self.directory.join(format!("frame-{:02}", *frames));
            std::fs::write(stem.with_extension("request.bin"), request)?;
            std::fs::write(stem.with_extension("header.bin"), header)?;
            std::fs::write(stem.with_extension("wire.bin"), body)?;
            std::fs::write(
                stem.with_extension("payload.bin"),
                decode_frame(header, body, true)?,
            )?;
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            *frames += 1;
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: capture_enhanced_history_transaction <new-dir> <ip:port> <market> <code> <YYYYMMDD> [start] [count]";
    let output = PathBuf::from(args.next().ok_or(usage)?);
    let address = args.next().ok_or(usage)?.parse()?;
    let market = args.next().ok_or(usage)?.parse()?;
    let code = args.next().ok_or(usage)?;
    let date = args.next().ok_or(usage)?.parse()?;
    let start = args
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(0);
    let count = args
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1800);
    if args.next().is_some() {
        return Err(usage.into());
    }
    let request = EnhancedHistoryTransactionRequest::new(market, &code, start, count, date)?;
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(30), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![address])?;
    let daily = pool.refresh()?;
    std::fs::write(
        output.join("daily-pool.json"),
        serde_json::to_vec_pretty(&daily)?,
    )?;
    let observer = Arc::new(Capture {
        directory: output.clone(),
        frames: Mutex::new(0),
    });
    let config = TcpConfig {
        ip: Some(address),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(3),
        auto_reconnect: 0,
        ..Default::default()
    };
    let mut tcp = Tcp::with_config_and_observer(&config, observer)?;
    let page = request.recv(&mut tcp)?;
    // Re-decode the retained payload as an offline reproducibility check.
    let replay = decode_enhanced_history_transactions(&request, &page.raw_payload)?;
    if replay.records != page.records {
        return Err("offline replay differs".into());
    }
    std::fs::write(output.join("page.json"), serde_json::to_vec_pretty(&page)?)?;
    let mut statuses = BTreeMap::<i32, usize>::new();
    for record in &page.records {
        *statuses.entry(record.status_raw).or_default() += 1;
    }
    let summary = serde_json::json!({
        "scope": "one source, one requested day, one page; not full-day coverage",
        "market": market,
        "code": code,
        "requested_date": date,
        "start": start,
        "request_count": count,
        "raw_count": page.raw_count(),
        "actual_view_count": page.actual_trades().count(),
        "auction_snapshot_count": page.auction_snapshots().count(),
        "opening_match_count": page.opening_matches().count(),
        "after_hours_count": page.after_hours_trades().count(),
        "status_counts": statuses,
        "price_base_raw_f32": page.price_base_raw_f32,
        "first_time": page.records.first().map(|record| record.time.as_str()),
        "last_time": page.records.last().map(|record| record.time.as_str()),
        "source_endpoint": page.source_endpoint,
        "fetched_at": page.fetched_at,
        "offline_replay_matches": true,
    });
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
