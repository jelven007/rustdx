//! Same-connection, bounded full-day 0fc6 capture with pagination cross-checks.
//! cargo run --example validate_enhanced_history_day -- <new-dir> <ip:port> <market> <code> <YYYYMMDD>
use rustdx_complete::{
    operation::OperationContext,
    tcp::{
        FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame,
        ip::IpPool,
        stock::{EnhancedHistoryTransactionPage, EnhancedHistoryTransactionRequest, KlineRequest},
    },
};
use serde_json::{Value, json};
use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_FRAMES: usize = 128;
const MAX_PAGES_PER_PASS: usize = 32;

#[derive(Debug)]
struct Capture {
    directory: PathBuf,
    frames: Mutex<usize>,
}

impl Capture {
    fn frame_count(&self) -> io::Result<usize> {
        self.frames
            .lock()
            .map(|count| *count)
            .map_err(|_| io::Error::other("capture lock"))
    }
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
            if *frames >= MAX_FRAMES {
                return Err(io::Error::other("capture frame budget exhausted"));
            }
            let stem = self.directory.join(format!("frame-{:03}", *frames));
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

fn save_json(path: &Path, value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

struct Session {
    tcp: Tcp,
    capture: Arc<Capture>,
    directory: PathBuf,
    connection_id: u64,
    peer: SocketAddr,
    local: SocketAddr,
    manifest: Value,
}

impl Session {
    fn verify_connection(&self) -> io::Result<()> {
        if self.tcp.connection_id().get() != self.connection_id
            || self.tcp.get_ref().0.peer_addr()? != self.peer
            || self.tcp.get_ref().0.local_addr()? != self.local
        {
            return Err(io::Error::other("connection changed during capture"));
        }
        Ok(())
    }

    fn record_query(
        &mut self,
        frame: usize,
        query: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.verify_connection()?;
        if self.capture.frame_count()? != frame + 1 {
            return Err("query must produce exactly one response frame".into());
        }
        let mut query = query;
        query["frame"] = json!(format!("frame-{frame:03}"));
        query["connection_id_before"] = json!(self.connection_id);
        query["connection_id_after"] = json!(self.tcp.connection_id().get());
        query["peer_after"] = json!(self.peer);
        query["local_after"] = json!(self.local);
        self.manifest["queries"]
            .as_array_mut()
            .ok_or("manifest queries")?
            .push(query);
        save_json(&self.directory.join("capture.json"), &self.manifest)?;
        Ok(())
    }

    fn enhanced_page(
        &mut self,
        label: &str,
        request: EnhancedHistoryTransactionRequest<'_>,
    ) -> Result<EnhancedHistoryTransactionPage, Box<dyn std::error::Error>> {
        self.verify_connection()?;
        let frame = self.capture.frame_count()?;
        let page = request.recv(&mut self.tcp)?;
        self.verify_connection()?;
        save_json(&self.directory.join(format!("{label}.json")), &page)?;
        self.record_query(
            frame,
            json!({
                "kind": "enhanced_history", "label": label,
                "start": request.start(), "count": request.count(),
                "rows": page.raw_count(), "page": format!("{label}.json"),
                "fetched_at": page.fetched_at,
            }),
        )?;
        println!(
            "{label}: start={} rows={}",
            request.start(),
            page.raw_count()
        );
        Ok(page)
    }

    fn full_pass(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        page_size: u16,
    ) -> Result<(usize, Vec<u16>), Box<dyn std::error::Error>> {
        let mut start = 0;
        let mut boundaries = Vec::new();
        for number in 0..MAX_PAGES_PER_PASS {
            let label = format!("pass-{page_size}-page-{number:02}");
            let request =
                EnhancedHistoryTransactionRequest::new(market, code, start, page_size, date)?;
            let page = self.enhanced_page(&label, request)?;
            match page.next_start()? {
                Some(next) => {
                    start = next;
                    boundaries.push(next);
                }
                None => return Ok((usize::from(start), boundaries)),
            }
        }
        Err("page budget exhausted before an empty response".into())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage =
        "usage: validate_enhanced_history_day <new-dir> <ip:port> <market> <code> <YYYYMMDD>";
    let directory = PathBuf::from(args.next().ok_or(usage)?);
    let address = args.next().ok_or(usage)?.parse()?;
    let market = args.next().ok_or(usage)?.parse()?;
    let code = args.next().ok_or(usage)?;
    let date = args.next().ok_or(usage)?.parse()?;
    if args.next().is_some() {
        return Err(usage.into());
    }
    EnhancedHistoryTransactionRequest::new(market, &code, 0, 1800, date)?;
    std::fs::create_dir(&directory)?;
    let operation = OperationContext::new(Duration::from_secs(120), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![address])?;
    save_json(&directory.join("daily-pool.json"), &pool.refresh()?)?;
    let capture = Arc::new(Capture {
        directory: directory.clone(),
        frames: Mutex::new(0),
    });
    let config = TcpConfig {
        ip: Some(address),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(3),
        auto_reconnect: 0,
        ..Default::default()
    };
    let tcp = Tcp::with_config_and_observer(&config, capture.clone())?;
    let mut session = Session {
        connection_id: tcp.connection_id().get(),
        peer: tcp.get_ref().0.peer_addr()?,
        local: tcp.get_ref().0.local_addr()?,
        tcp,
        capture,
        directory,
        manifest: json!({
            "started_utc": chrono::Utc::now(),
            "market": market, "code": code, "requested_date": date,
            "auto_reconnect": 0, "global_budget_seconds": 120,
            "frame_budget": MAX_FRAMES, "max_pages_per_pass": MAX_PAGES_PER_PASS,
            "queries": [],
            "scope": "one source connection; enumeration and aggregate validation, no exchange trade IDs",
        }),
    };
    session.manifest["connection_id"] = json!(session.connection_id);
    session.manifest["peer"] = json!(session.peer);
    session.manifest["local"] = json!(session.local);
    save_json(&session.directory.join("capture.json"), &session.manifest)?;
    let (primary_count, boundaries) = session.full_pass(market, &code, date, 1800)?;
    let (secondary_count, _) = session.full_pass(market, &code, date, 1000)?;
    session.manifest["primary_raw_count"] = json!(primary_count);
    session.manifest["secondary_raw_count"] = json!(secondary_count);
    for (index, boundary) in boundaries.into_iter().enumerate() {
        let request = EnhancedHistoryTransactionRequest::new(
            market,
            &code,
            boundary.saturating_sub(16),
            32,
            date,
        )?;
        session.enhanced_page(&format!("overlap-{index:02}"), request)?;
    }
    session.enhanced_page(
        "latest-32",
        EnhancedHistoryTransactionRequest::new(market, &code, 0, 32, date)?,
    )?;
    for (category, count) in [(9, 3), (4, 3), (0, 96)] {
        session.verify_connection()?;
        let frame = session.capture.frame_count()?;
        let bars =
            KlineRequest::new(market, &code, category, 0, count)?.recv_stock(&mut session.tcp)?;
        let label = format!("kline-{category}");
        save_json(&session.directory.join(format!("{label}.json")), &bars)?;
        session.record_query(
            frame,
            json!({
                "kind": "kline", "label": label, "category": category,
                "start": 0, "count": count, "rows": bars.len(),
                "page": format!("{label}.json"), "fetched_at": chrono::Utc::now(),
            }),
        )?;
        println!("{label}: {} bars", bars.len());
    }
    session.verify_connection()?;
    session.manifest["finished_utc"] = json!(chrono::Utc::now());
    session.manifest["frame_count"] = json!(session.capture.frame_count()?);
    session.manifest["capture_complete"] = json!(true);
    save_json(&session.directory.join("capture.json"), &session.manifest)?;
    Ok(())
}
