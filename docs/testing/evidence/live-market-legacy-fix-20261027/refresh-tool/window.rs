use chrono::Utc;
use rustdx_complete::{
    operation::OperationContext,
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use serde_json::json;
use std::{io, net::SocketAddr, path::PathBuf, sync::{Arc, Mutex}, time::Duration};

#[derive(Debug)]
struct Capture { root: PathBuf, count: Mutex<usize> }
impl FrameObserver for Capture {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        if let FrameEvent::Response { endpoint, request, header, body } = event {
            let mut count = self.count.lock().map_err(|_| io::Error::other("capture lock"))?;
            if *count >= 96 || body.len() > 1024 * 1024 { return Err(io::Error::other("capture budget")); }
            let payload = decode_frame(header, body, true)?;
            let stem = self.root.join(format!("frame-{count:03}"));
            for (suffix, bytes) in [("request.bin", request), ("header.bin", header.as_slice()),
                                   ("wire.bin", body), ("payload.bin", payload.as_slice())] {
                std::fs::write(stem.with_extension(suffix), bytes)?;
            }
            std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
            std::fs::write(stem.with_extension("utc.txt"), Utc::now().to_rfc3339())?;
            *count += 1;
        }
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 { return Err("new-dir endpoint precision-source".into()); }
    let root = PathBuf::from(&args[0]);
    let endpoint: SocketAddr = args[1].parse()?;
    std::fs::create_dir(&root)?;
    let precision = std::fs::read(PathBuf::from(&args[2]).join("precision.json"))?;
    std::fs::write(root.join("precision.json"), &precision)?;
    let rows: Vec<(u16, String, u8)> = serde_json::from_slice(&precision)?;
    let symbols = [(0, "000001"), (0, "159915"), (1, "510300")];
    let inputs: Vec<_> = rows.iter().filter(|(m,c,_)| symbols.contains(&(*m,c.as_str())))
        .map(|(m,c,d)| (*m,c.as_str(),*d)).collect();
    let metadata = PrecisionSnapshot::from_decimals(&inputs, Utc::now(), Some(endpoint))?;
    let pool = IpPool::new(vec![endpoint])?;
    std::fs::write(root.join("daily-pool.json"), serde_json::to_vec_pretty(&pool.refresh()?)?)?;
    let config = TcpConfig { ip: Some(endpoint), ip_pool: Some(pool),
        auto_reconnect: 0, timeout: Duration::from_secs(4), ..Default::default() };
    let observer = Arc::new(Capture { root: root.clone(), count: Mutex::new(0) });
    let mut tcp = Tcp::with_config_and_observer(&config, observer)?;
    tcp.set_heartbeat_interval(None)?;
    let operation = OperationContext::new(Duration::from_secs(120), None)?;
    let _guard = operation.enter();
    let mut summary = json!({"endpoint":endpoint, "source":"live_tcp", "synthetic":false,
        "started_utc":Utc::now(), "status":"running", "schema":2, "results":[],
        "scope":"Bounded requested-date availability probes; neither actual-date evidence nor a retention guarantee."});
    let mut results = Vec::new();
    for date in [20260105,20260401,20260506,20260601,20260701,20260803,
                 20260810,20260814,20260817,20260901,20261026] {
        for (market, code) in symbols {
            let series = RecentMinutesRequest::new(market, code, date)?.recv(&mut tcp, &metadata)?;
            assert!(series.require_trading_date().is_err());
            let file = format!("recent-{date}-{market}-{code}.json");
            std::fs::write(root.join(&file), serde_json::to_vec_pretty(&series)?)?;
            results.push(json!({"requested_date":date,"market":market,"code":code,
                "points":series.page.records.len(),"file":file,"actual_date_qualified":false}));
            summary["results"] = json!(results);
            std::fs::write(root.join("summary.json"), serde_json::to_vec_pretty(&summary)?)?;
        }
    }
    summary["status"] = json!("captured");
    summary["finished_utc"] = json!(Utc::now());
    std::fs::write(root.join("summary.json"), serde_json::to_vec_pretty(&summary)?)?;
    println!("{} probes completed for {endpoint}", results.len());
    Ok(())
}
