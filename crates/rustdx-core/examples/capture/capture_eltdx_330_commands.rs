//! Bounded live capture: cargo run --example capture_eltdx_330_commands -- <new-dir> <ip:port>
use rustdx_complete::{
    operation::OperationContext,
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug)]
struct Capture {
    output: PathBuf,
    count: Mutex<(usize, usize)>,
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
            let mut count = self
                .count
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            if count.0 >= 256 || count.1 + body.len() > 16 * 1024 * 1024 {
                return Err(io::Error::other("capture budget exhausted"));
            }
            let stem = self.output.join(format!("frame-{:03}", count.0));
            std::fs::write(stem.with_extension("request.bin"), request)?;
            std::fs::write(stem.with_extension("header.bin"), header)?;
            std::fs::write(stem.with_extension("wire.bin"), body)?;
            std::fs::write(
                stem.with_extension("payload.bin"),
                decode_frame(header, body, true)?,
            )?;
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            count.0 += 1;
            count.1 += body.len();
        }
        Ok(())
    }
}
fn save<T: serde::Serialize>(
    output: &Path,
    name: &str,
    result: io::Result<T>,
    summary: &mut Vec<serde_json::Value>,
) -> io::Result<()> {
    let status = match result {
        Ok(value) => {
            std::fs::write(
                output.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&value)?,
            )?;
            serde_json::json!({"name":name,"decoded":true})
        }
        Err(e) => {
            serde_json::json!({"name":name,"decoded":false,"kind":format!("{:?}",e.kind()),"error":e.to_string()})
        }
    };
    println!("{status}");
    summary.push(status);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("new directory required")?);
    let endpoint = args.next().ok_or("endpoint required")?.parse()?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(180), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    std::fs::write(
        output.join("daily-pool.json"),
        serde_json::to_vec_pretty(&pool.refresh()?)?,
    )?;
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(4),
        ..Default::default()
    };
    let mut tcp = Tcp::with_config_and_observer(
        &config,
        Arc::new(Capture {
            output: output.clone(),
            count: Mutex::new((0, 0)),
        }),
    )?;
    let mut summary = Vec::new();
    save(
        &output,
        "block-metadata",
        BlockMetaRequest::new("block_gn.dat")?.recv(&mut tcp),
        &mut summary,
    )?;
    for (market, code) in [(0, "000001"), (1, "600000")] {
        let result = CompanyInfoDirectoryRequest::new(market, code)?.recv(&mut tcp);
        if let Ok(directory) = &result
            && let Some(item) = directory
                .entries
                .iter()
                .filter(|i| i.length > 0 && i.length <= 2 * 1024 * 1024)
                .max_by_key(|i| i.length)
        {
            let content = company_info_text(
                &mut tcp,
                CompanyInfoTextRequest::new(market, code, &item.filename, item.start, item.length)?,
                CompanyInfoReadOptions {
                    max_bytes: 2 * 1024 * 1024,
                    timeout: Duration::from_secs(30),
                    ..Default::default()
                },
            );
            save(
                &output,
                &format!("f10-text-{market}-{code}"),
                content,
                &mut summary,
            )?;
        }
        save(
            &output,
            &format!("f10-directory-{market}-{code}"),
            result,
            &mut summary,
        )?;
        rustdx_complete::operation::sleep(Duration::from_millis(150))?;
        if !tcp.is_reusable() {
            tcp.reconnect()?;
        }
    }
    for (market, code) in [(0, "000038"), (0, "002087"), (1, "600001")] {
        save(
            &output,
            &format!("delisted-one-{market}-{code}"),
            DelistedBarsRequest::new(market, code, 0, 1)?.recv(&mut tcp),
            &mut summary,
        )?;
        save(
            &output,
            &format!("delisted-all-{market}-{code}"),
            delisted_bars_all(
                &mut tcp,
                DelistedBarsRequest::new(market, code, 0, 800)?,
                CommandScanOptions {
                    max_pages: 32,
                    max_records: 20_000,
                    timeout: Duration::from_secs(45),
                },
            ),
            &mut summary,
        )?;
        rustdx_complete::operation::sleep(Duration::from_millis(150))?;
        if !tcp.is_reusable() {
            tcp.reconnect()?;
        }
    }
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "endpoint":endpoint,"captured_at":chrono::Utc::now(),"results":summary,
            "scope":"Sampled archive and F10 responses on a non-trading day; no all-command or exchange completeness claim"
        }))?,
    )?;
    Ok(())
}
