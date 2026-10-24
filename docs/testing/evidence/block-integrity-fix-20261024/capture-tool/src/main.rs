use rustdx::{
    operation::{OperationContext, sleep},
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use std::{io, net::SocketAddr, path::PathBuf, sync::{Arc, Mutex}, time::Duration};
use serde_json::json;

#[derive(Debug)]
struct Capture { output: PathBuf, state: Mutex<(usize, usize)> }
impl FrameObserver for Capture {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        if let FrameEvent::Response { endpoint, request, header, body } = event {
            let payload = decode_frame(header, body, true)?;
            let mut state = self.state.lock().map_err(|_| io::Error::other("capture lock"))?;
            let size = request.len() + header.len() + body.len() + payload.len();
            if state.0 >= 128 || size > (16 * 1024 * 1024usize).saturating_sub(state.1) {
                return Err(io::Error::other("capture budget"));
            }
            let stem = self.output.join(format!("frame-{:03}", state.0));
            for (suffix, bytes) in [("request.bin", request), ("header.bin", header.as_slice()),
                ("wire.bin", body), ("payload.bin", payload.as_slice())] {
                std::fs::write(stem.with_extension(suffix), bytes)?;
            }
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            std::fs::write(stem.with_extension("utc.txt"), chrono::Utc::now().to_rfc3339())?;
            state.0 += 1; state.1 += size;
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = PathBuf::from(&args[0]);
    let endpoint: SocketAddr = args[1].parse()?;
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(180), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    std::fs::write(output.join("pool.json"), serde_json::to_vec_pretty(&pool.refresh()?)?)?;
    let observer = Arc::new(Capture { output: output.clone(), state: Mutex::new((0, 0)) });
    let mut tcp = Tcp::with_config_and_observer(&TcpConfig {
        ip: Some(endpoint), ip_pool: Some(pool), timeout: Duration::from_secs(3),
        auto_reconnect: 0, ..Default::default()
    }, observer.clone())?;
    tcp.set_heartbeat_interval(None)?;
    let started = chrono::Utc::now();
    let mut results = Vec::new();
    for filename in ["block_gn.dat", "block_zs.dat"] {
        match download_block_file(&mut tcp, filename) {
            Ok(file) => {
                std::fs::write(output.join(filename), &file.content)?;
                std::fs::write(output.join(format!("{filename}.metadata.json")), serde_json::to_vec_pretty(&file)?)?;
                match file.decode_records() {
                    Ok(records) => {
                        std::fs::write(output.join(format!("{filename}.records.json")), serde_json::to_vec_pretty(&records)?)?;
                        results.push(json!({"filename": filename, "md5_verified": true, "decoded": true,
                            "size": file.content.len(), "records": records.len(), "chunks":file.chunks.len()}));
                    }
                    Err(error) => results.push(json!({"filename": filename, "md5_verified":true, "decoded":false,
                        "kind": format!("{:?}",error.kind()), "error":error.to_string()})),
                }
            }
            Err(error) => results.push(json!({"filename": filename, "md5_verified":false, "decoded":false,
                "kind":format!("{:?}",error.kind()), "error":error.to_string()})),
        }
        println!("{}", results.last().unwrap());
        sleep(Duration::from_millis(250))?;
        if !tcp.is_reusable() { tcp.reconnect()?; tcp.set_heartbeat_interval(None)?; }
    }
    std::fs::write(output.join("summary.json"), serde_json::to_vec_pretty(&json!({
        "source":"live_tcp", "endpoint":endpoint, "started_utc":started,
        "finished_utc":chrono::Utc::now(), "queries":results,
        "between_file_spacing_ms":250, "total_timeout_seconds":180,
        "wire_frames":observer.state.lock().unwrap().0
    }))?)?;
    Ok(())
}
