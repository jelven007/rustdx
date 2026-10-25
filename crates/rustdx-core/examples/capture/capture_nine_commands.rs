//! Low-rate nine-command capture; every endpoint must pass the daily pool probe.
//! cargo run --example capture_nine_commands -- <new-dir> <ip:port> [YYYYMMDD]
use chrono::Datelike;
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
            if count.0 >= 512 || count.1 + body.len() > 16 * 1024 * 1024 {
                return Err(io::Error::other("capture budget"));
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
impl PushObserver for Capture {
    fn observe(&self, endpoint: std::net::SocketAddr, push: &QuotePush) -> io::Result<()> {
        let mut count = self
            .count
            .lock()
            .map_err(|_| io::Error::other("capture lock"))?;
        if count.0 >= 512 || count.1 + push.wire_body.len() > 16 * 1024 * 1024 {
            return Err(io::Error::other("capture budget"));
        }
        let stem = self.output.join(format!("push-{:03}", count.0));
        std::fs::write(stem.with_extension("header.bin"), push.header)?;
        std::fs::write(stem.with_extension("wire.bin"), &push.wire_body)?;
        std::fs::write(stem.with_extension("endpoint.txt"), endpoint.to_string())?;
        count.0 += 1;
        count.1 += push.wire_body.len();
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
        Err(error) => {
            serde_json::json!({"name":name,"decoded":false,"kind":format!("{:?}",error.kind()),"error":error.to_string()})
        }
    };
    println!("{status}");
    summary.push(status);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("new output directory required")?);
    let endpoint = args.next().ok_or("endpoint required")?.parse()?;
    let date = args
        .next()
        .map(|s| s.parse::<u32>())
        .transpose()?
        .unwrap_or(20261023);
    std::fs::create_dir(&output)?;
    let operation = OperationContext::new(Duration::from_secs(180), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    let daily = pool.refresh()?;
    std::fs::write(
        output.join("daily-pool.json"),
        serde_json::to_vec_pretty(&daily)?,
    )?;
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(4),
        ..Default::default()
    };
    let observer = Arc::new(Capture {
        output: output.clone(),
        count: Mutex::new((0, 0)),
    });
    let mut tcp = Tcp::with_config_and_observer(&config, observer.clone())?;
    let mut entries = Vec::new();
    for market in [0, 1] {
        let rows = security_directory(&mut tcp, market)?;
        std::fs::write(
            output.join(format!("directory-{market}.json")),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        entries.extend(rows.into_iter().map(|r| (market, r.code, r.decimal)));
    }
    let decimals: Vec<_> = entries
        .iter()
        .map(|(m, c, d)| (*m, c.as_str(), *d))
        .collect();
    let metadata = Arc::new(PrecisionSnapshot::from_decimals(
        &decimals,
        chrono::Utc::now(),
        Some(endpoint),
    )?);
    let mut summary = Vec::new();
    macro_rules! capture {
        ($name:expr,$result:expr) => {{
            save(&output, $name, $result, &mut summary)?;
            tcp.delay_capture()?;
        }};
    }
    // Reconnect after an error without hiding it in the per-command manifest.
    trait DelayCapture {
        fn delay_capture(&mut self) -> io::Result<()>;
    }
    impl DelayCapture for Tcp {
        fn delay_capture(&mut self) -> io::Result<()> {
            rustdx_complete::operation::sleep(Duration::from_millis(150))?;
            if !self.is_reusable() {
                self.reconnect()?;
            }
            Ok(())
        }
    }
    capture!(
        "special-limits",
        SpecialLimitsRequest::new(0).recv(&mut tcp)
    );
    capture!(
        "special-limits-next",
        SpecialLimitsRequest::new(1).recv(&mut tcp)
    );
    for kind in [
        IntradayAuxKind::BuySellStrength,
        IntradayAuxKind::VolumeComparison,
    ] {
        capture!(
            &format!("aux-{}", kind.selector()),
            IntradayAuxRequest::new(0, "000001", kind)?.recv(&mut tcp)
        );
    }
    for window in [20, 40] {
        capture!(
            &format!("sparkline-{window}"),
            SparklineRequest::new(
                0,
                "000001",
                SparklineOptions {
                    window,
                    ..Default::default()
                }
            )?
            .recv(&mut tcp)
        );
    }
    let requested_day = chrono::NaiveDate::parse_from_str(&date.to_string(), "%Y%m%d")?;
    let local_day = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).ok_or("timezone")?)
        .date_naive();
    let days: std::collections::BTreeSet<_> = [
        requested_day,
        requested_day.pred_opt().ok_or("previous date")?,
        local_day,
    ]
    .into_iter()
    .collect();
    for day in days {
        let day = day.year() as u32 * 10_000 + day.month() * 100 + day.day();
        for (market, code) in [(0, "000001"), (0, "159915")] {
            capture!(
                &format!("recent-{day}-{code}"),
                RecentMinutesRequest::new(market, code, day)?.recv(&mut tcp, &metadata)
            );
        }
    }
    capture!(
        "auction-history",
        AuctionSeriesRequest::new(0, "000001", Some(date), AuctionOptions::default())?
            .recv(&mut tcp)
    );
    capture!(
        "auction-current",
        AuctionSeriesRequest::new(0, "000001", None, AuctionOptions::default())?.recv(&mut tcp)
    );
    for start in [0, 40, 80] {
        capture!(
            &format!("auction-page-{start}"),
            AuctionSeriesRequest::new(
                0,
                "000001",
                None,
                AuctionOptions {
                    start,
                    count: 40,
                    ..Default::default()
                }
            )?
            .recv(&mut tcp)
        );
    }
    for (m, c) in [(0, "000001"), (1, "600519")] {
        capture!(
            &format!("money-{m}-{c}"),
            MoneyFlowRequest::new(m, c)?.recv(&mut tcp)
        );
    }
    let stocks = [(0, "000001"), (0, "159915"), (1, "510300")];
    capture!(
        "snapshots",
        SnapshotsRequest::new(&stocks)?.recv(&mut tcp, &metadata)
    );
    capture!(
        "quotes-053e",
        QuotesRequest::new(stocks.to_vec())?.recv_with_metadata(&mut tcp, &metadata)
    );
    for ascending in [false, true] {
        capture!(
            &format!("category-{ascending}"),
            CategoryQuotesRequest::new(
                QuoteCategory::AShares,
                QuoteSortField::Amount,
                CategoryQuoteOptions {
                    count: 5,
                    ascending,
                    ..Default::default()
                }
            )?
            .recv(&mut tcp, &metadata)
        );
    }
    for ascending in [false, true] {
        capture!(
            &format!("category-change-{ascending}"),
            CategoryQuotesRequest::new(
                QuoteCategory::AShares,
                QuoteSortField::ChangePercent,
                CategoryQuoteOptions {
                    count: 5,
                    ascending,
                    ..Default::default()
                }
            )?
            .recv(&mut tcp, &metadata)
        );
    }
    capture!(
        "category-next",
        CategoryQuotesRequest::new(
            QuoteCategory::AShares,
            QuoteSortField::Amount,
            CategoryQuoteOptions {
                start: 5,
                count: 5,
                ..Default::default()
            }
        )?
        .recv(&mut tcp, &metadata)
    );
    let mut subscription = QuoteSubscription::from_tcp(
        tcp,
        RefreshRequest::from_stocks(&stocks)?,
        metadata,
        SubscriptionOptions::default(),
    )?;
    subscription.set_push_observer(Some(observer));
    for index in 0..3 {
        save(
            &output,
            &format!("refresh-{index}"),
            subscription.refresh(),
            &mut summary,
        )?;
        rustdx_complete::operation::sleep(Duration::from_millis(150))?;
        if subscription.snapshot().status == DepthSyncStatus::Invalid {
            break;
        }
    }
    match subscription.poll_push(Duration::from_millis(250)) {
        Ok(Some(push)) => {
            save(&output, "push", Ok(push.refresh), &mut summary)?;
        }
        Ok(None) => summary.push(serde_json::json!({"name":"push","observed":false})),
        Err(e) => summary.push(serde_json::json!({"name":"push","error":e.to_string()})),
    }
    std::fs::write(
        output.join("depth-state.json"),
        serde_json::to_vec_pretty(&subscription.snapshot())?,
    )?;
    subscription.close();
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "endpoint":endpoint,"captured_at":chrono::Utc::now(),"requested_date":date,"results":summary,
            "recent_minutes_date_schema_version": RECENT_MINUTES_DATE_SCHEMA_VERSION,
            "scope":"Single endpoint and sampled parameters; no trading-hour push or exchange-completeness claim"
        }))?,
    )?;
    Ok(())
}
