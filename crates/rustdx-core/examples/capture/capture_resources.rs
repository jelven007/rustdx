//! Low-frequency bounded real resource capture, separate from board evidence.
//! cargo run -p jelven-rustdx-complete --example capture_resources -- <new-dir> <ip:port>
use rustdx_complete::{
    operation::{OperationContext, sleep},
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use serde_json::json;
use std::{
    io,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug)]
struct Capture {
    output: PathBuf,
    state: Mutex<(usize, usize)>,
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
                .state
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            let size = request.len() + header.len() + body.len() + payload.len();
            if state.0 >= 2048 || size > (64 * 1024 * 1024usize).saturating_sub(state.1) {
                return Err(io::Error::other("capture budget"));
            }
            let stem = self.output.join(format!("frame-{:04}", state.0));
            for (suffix, bytes) in [
                ("request.bin", request),
                ("header.bin", header.as_slice()),
                ("wire.bin", body),
                ("payload.bin", payload.as_slice()),
            ] {
                std::fs::write(stem.with_extension(suffix), bytes)?;
            }
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            std::fs::write(
                stem.with_extension("utc.txt"),
                chrono::Utc::now().to_rfc3339(),
            )?;
            state.0 += 1;
            state.1 += size;
        }
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("new output directory required")?);
    let endpoint: SocketAddr = args.next().ok_or("endpoint required")?.parse()?;
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(180), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    std::fs::write(
        output.join("pool.json"),
        serde_json::to_vec_pretty(&pool.refresh()?)?,
    )?;
    let observer = Arc::new(Capture {
        output: output.clone(),
        state: Mutex::new((0, 0)),
    });
    let mut tcp = Tcp::with_config_and_observer(
        &TcpConfig {
            ip: Some(endpoint),
            ip_pool: Some(pool),
            timeout: Duration::from_secs(3),
            auto_reconnect: 0,
            ..Default::default()
        },
        observer.clone(),
    )?;
    tcp.set_heartbeat_interval(None)?;
    let started = chrono::Utc::now();
    let mut results = Vec::new();
    for path in ["zhb.zip", "block_zs.dat"] {
        let options = ResourceReadOptions {
            max_bytes: 32 * 1024 * 1024 + 1,
            page_delay: Duration::from_millis(25),
            ..Default::default()
        };
        match download_resource_file(&mut tcp, path, options) {
            Ok(file) => {
                std::fs::write(output.join(path), &file.content)?;
                std::fs::write(
                    output.join(format!("{path}.json")),
                    serde_json::to_vec_pretty(&file)?,
                )?;
                if path == "zhb.zip" {
                    let stats = parse_tdx_stats_archive(file.require_complete()?, path)?;
                    for member in &stats.members {
                        std::fs::write(output.join(&member.name), &member.content)?;
                    }
                    std::fs::write(
                        output.join("zhb.stats.json"),
                        serde_json::to_vec_pretty(&stats)?,
                    )?;
                }
                results.push(json!({"path":path, "size":file.content.len(),
                    "complete":file.complete, "chunks":file.chunks.len(), "termination":file.termination}));
            }
            Err(error) => results.push(json!({"path":path, "error":error.to_string(),
                "kind":format!("{:?}",error.kind())})),
        }
        println!("{}", results.last().ok_or("missing result")?);
        sleep(Duration::from_millis(250))?;
        if !tcp.is_reusable() {
            break;
        }
    }
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "source":"live_tcp", "endpoint":endpoint, "started_utc":started,
            "finished_utc":chrono::Utc::now(), "queries":results, "auto_reconnect":0, "heartbeat":false,
            "page_delay_ms":25, "between_file_spacing_ms":250, "total_timeout_seconds":180,
            "wire_frames":observer.state.lock().map_err(|_| io::Error::other("capture lock"))?.0
        }))?,
    )?;
    Ok(())
}
