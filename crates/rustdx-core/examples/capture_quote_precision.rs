//! Bounded live capture: cargo run --example capture_quote_precision -- <new-dir> <ip:port>
use rustdx_complete::{
    operation::OperationContext,
    tcp::{
        FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame,
        ip::IpPool,
        stock::{PrecisionSnapshot, QuotesRequest, stocks},
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
    sequence: Mutex<usize>,
}

impl FrameObserver for Capture {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        if let FrameEvent::Response {
            request,
            header,
            body,
            ..
        } = event
        {
            let mut sequence = self
                .sequence
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            if *sequence >= 200 {
                return Err(io::Error::other("capture frame budget exhausted"));
            }
            let stem = self.directory.join(format!("frame-{:03}", *sequence));
            std::fs::write(stem.with_extension("request.bin"), request)?;
            std::fs::write(stem.with_extension("header.bin"), header)?;
            std::fs::write(stem.with_extension("wire.bin"), body)?;
            std::fs::write(
                stem.with_extension("payload.bin"),
                decode_frame(header, body, true)?,
            )?;
            *sequence += 1;
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("new output directory required")?);
    let address = args.next().ok_or("explicit ip:port required")?.parse()?;
    // A new directory keeps previous captures intact.
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(90), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![address])?;
    let daily = pool.refresh()?;
    std::fs::write(
        output.join("daily-pool.json"),
        serde_json::to_vec_pretty(&daily)?,
    )?;
    let observer = Arc::new(Capture {
        directory: output.clone(),
        sequence: Mutex::new(0),
    });
    let config = TcpConfig {
        ip: Some(address),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(3),
        ..Default::default()
    };
    let mut tcp = Tcp::with_config_and_observer(&config, observer)?;
    let started = chrono::Utc::now();
    let mut directories = Vec::new();
    for market in [0, 1] {
        let rows = stocks(&mut tcp, market)?;
        std::fs::write(
            output.join(format!("directory-{market}.json")),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        directories.push((market, rows));
    }
    let inputs: Vec<_> = directories
        .iter()
        .flat_map(|(market, rows)| {
            rows.iter()
                .map(move |row| (*market, row.code.as_str(), row.decimal_point))
        })
        .collect();
    let fetched_at = chrono::Utc::now();
    let timezone = rustdx_complete::data::exchange_timezone();
    if started.with_timezone(&timezone).date_naive()
        != fetched_at.with_timezone(&timezone).date_naive()
    {
        return Err(
            "directory capture crossed exchange midnight; rerun with a new directory".into(),
        );
    }
    let precision = PrecisionSnapshot::from_decimals(&inputs, fetched_at, Some(address))?;
    let mut distribution = BTreeMap::<u8, usize>::new();
    for (_, _, decimal) in &inputs {
        *distribution.entry(*decimal).or_default() += 1;
    }
    let mut selected = vec![(0, "000001"), (0, "159915"), (1, "510300")];
    selected.retain(|key| inputs.iter().any(|(m, c, _)| (*m, *c) == *key));
    for (market, rows) in &directories {
        let candidates: Vec<_> = rows
            .iter()
            .filter(|row| row.decimal_point == 4 && row.pre_close > 0.0 && row.pre_close != 100.0)
            .collect();
        for index in [
            0,
            1,
            2,
            candidates.len() / 2,
            candidates.len().saturating_sub(2),
            candidates.len().saturating_sub(1),
        ] {
            if let Some(row) = candidates.get(index)
                && !selected.contains(&(*market, row.code.as_str()))
            {
                selected.push((*market, row.code.as_str()));
            }
        }
    }
    let request = QuotesRequest::new(selected)?;
    let page = request.recv_with_metadata(&mut tcp, &precision)?;
    let report = serde_json::json!({
        "scope": "Live wire and 0450 metadata consistency; independent price verification separate",
        "distribution": distribution, "metadata": precision.status(), "quotes": page,
    });
    std::fs::write(
        output.join("live.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
