use chrono::Utc;
use rustdx_complete::{
    operation::OperationContext,
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use serde_json::json;
use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const SYMBOLS: [(u16, &str); 3] = [(0, "000001"), (0, "159915"), (1, "510300")];

fn save(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)
}

#[derive(Debug)]
struct Capture {
    root: PathBuf,
    count: Mutex<usize>,
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
            let mut count = self.count.lock().map_err(|_| io::Error::other("lock"))?;
            if *count >= 16 || body.len() > 1024 * 1024 {
                return Err(io::Error::other("capture budget"));
            }
            let stem = self.root.join(format!("frame-{count:03}"));
            for (suffix, data) in [
                ("request.bin", request),
                ("header.bin", header.as_slice()),
                ("wire.bin", body),
                ("payload.bin", payload.as_slice()),
            ] {
                std::fs::write(stem.with_extension(suffix), data)?;
            }
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            std::fs::write(stem.with_extension("utc.txt"), Utc::now().to_rfc3339())?;
            *count += 1;
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("new directory")?);
    let endpoint: SocketAddr = args.next().ok_or("endpoint")?.parse()?;
    let source = PathBuf::from(args.next().ok_or("captured precision directory")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    std::fs::create_dir(&root)?;
    let operation = OperationContext::new(Duration::from_secs(90), None)?;
    let _guard = operation.enter();
    let precision_bytes = std::fs::read(source.join("precision.json"))?;
    std::fs::write(root.join("precision.json"), &precision_bytes)?;
    let rows: Vec<(u16, String, u8)> = serde_json::from_slice(&precision_bytes)?;
    let inputs: Vec<_> = rows
        .iter()
        .filter(|(market, code, _)| SYMBOLS.contains(&(*market, code.as_str())))
        .map(|(market, code, decimal)| (*market, code.as_str(), *decimal))
        .collect();
    let metadata = Arc::new(PrecisionSnapshot::from_decimals(
        &inputs,
        Utc::now(),
        Some(endpoint),
    )?);
    let observer = Arc::new(Capture {
        root: root.clone(),
        count: Mutex::new(0),
    });
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(IpPool::new(vec![endpoint])?),
        auto_reconnect: 0,
        timeout: Duration::from_secs(4),
        ..Default::default()
    };
    let tcp = Tcp::with_config_and_observer(&config, observer)?;
    let mut subscription = QuoteSubscription::from_tcp(
        tcp,
        RefreshRequest::from_stocks(&SYMBOLS)?,
        metadata,
        SubscriptionOptions {
            refresh_timeout: Duration::from_secs(4),
            ..Default::default()
        },
    )?;
    let mut summary = json!({
        "source":"live_tcp", "synthetic":false, "status":"running",
        "started_utc":Utc::now(), "endpoint":endpoint, "symbols":SYMBOLS,
        "precision_source":source, "heartbeat":false, "auto_reconnect":0,
        "scope":"Four poll_update calls with explicit passive/refresh source distinction",
        "idle_timeout_seconds":2, "refreshes":[]
    });
    save(&root.join("summary.json"), &summary)?;
    for index in 0..4 {
        let (origin, refresh) = match subscription.poll_update(Duration::from_secs(2))? {
            QuoteUpdate::RequestedRefresh(refresh) => ("requested_refresh", refresh),
            QuoteUpdate::PassivePush(push) => {
                std::fs::write(root.join(format!("push-{index}.header.bin")), push.header)?;
                std::fs::write(root.join(format!("push-{index}.wire.bin")), &push.wire_body)?;
                ("passive_push", push.refresh)
            }
        };
        let snapshot = subscription.synchronized_snapshot()?;
        save(&root.join(format!("refresh-{index}.json")), &refresh)?;
        save(&root.join(format!("snapshot-{index}.json")), &snapshot)?;
        summary["refreshes"]
            .as_array_mut()
            .ok_or_else(|| io::Error::other("summary"))?
            .push(
                json!({"index":index,"origin":origin,"utc":Utc::now(),"connection_id":snapshot.connection_id,
                         "generation":snapshot.generation,"status":snapshot.status}),
            );
        save(&root.join("summary.json"), &summary)?;
    }
    summary["status"] = json!("captured");
    summary["finished_utc"] = json!(Utc::now());
    save(&root.join("summary.json"), &summary)?;
    println!("{summary}");
    Ok(())
}
