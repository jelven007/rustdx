//! Bounded 0547 validation: one baseline request, then only passive reads.
//! cargo run --example validate_quote_push -- <new-dir> <ip:port> [--seconds 180] [--allow-closed]
//! Exit 0 means trading-hour acceptance; 2 means captured but not accepted.
use chrono::{DateTime, Datelike, NaiveDateTime, Timelike, Utc};
use rustdx_complete::{
    calendar::{BuiltinCalendar, CalendarProvider, TradingCalendar},
    operation::OperationContext,
    tcp::{FrameEvent, FrameObserver, Tcp, TcpConfig, decode_frame, ip::IpPool, stock::*},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

const GUARD_SECONDS: u64 = 5;
const MAX_EVENTS: usize = 8192;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const STOCKS: [(u16, &str); 3] = [(0, "000001"), (0, "159915"), (1, "510300")];

#[cfg(test)]
#[path = "../../tests/common/extended_protocol.rs"]
mod protocol;

fn save(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn session_at(at: NaiveDateTime) -> io::Result<Option<u32>> {
    if !TradingCalendar::try_is_trading_day(&at.date()).map_err(io::Error::other)? {
        return Ok(None);
    }
    let seconds = at.time().num_seconds_from_midnight();
    Ok(if (34200..41400).contains(&seconds) {
        Some(34200)
    } else if (46800..54000).contains(&seconds) {
        Some(46800)
    } else {
        None
    })
}

fn window_open(at: NaiveDateTime, seconds: u64) -> io::Result<bool> {
    let end = at + chrono::Duration::seconds(seconds as i64);
    same_session(at, end)
}

fn same_session(start: NaiveDateTime, end: NaiveDateTime) -> io::Result<bool> {
    Ok(start <= end
        && start.date() == end.date()
        && session_at(start)?
            .is_some_and(|session| session_at(end).ok().flatten() == Some(session)))
}

fn local(at: DateTime<Utc>) -> NaiveDateTime {
    at.with_timezone(&rustdx_complete::data::exchange_timezone())
        .naive_local()
}

fn calendar(at: DateTime<Utc>) -> Value {
    let metadata = BuiltinCalendar.metadata();
    let closed_weekdays: Vec<_> = metadata
        .start
        .iter_days()
        .take_while(|day| *day <= metadata.end)
        .filter(|day| day.weekday().number_from_monday() <= 5)
        .filter(|day| TradingCalendar::try_is_trading_day(day).is_ok_and(|open| !open))
        .collect();
    let next = TradingCalendar::try_next_trading_day(&local(at).date())
        .ok()
        .and_then(|day| day.and_hms_opt(9, 35, 0));
    json!({
        "version": metadata.version, "source": metadata.source,
        "coverage_start": metadata.start, "coverage_end": metadata.end,
        "local_started": local(at), "timezone": "UTC+08:00",
        "closed_weekdays": closed_weekdays,
        "is_trading_day": TradingCalendar::try_is_trading_day(&local(at).date()).ok(),
        "next_trading_day_suggested_start": next,
    })
}

#[derive(Debug)]
struct CaptureState {
    trace: File,
    phase: &'static str,
    connection_id: Option<u64>,
    events: usize,
    bytes: usize,
    requests: usize,
    passive_requests: usize,
}

#[derive(Debug)]
struct Capture {
    directory: PathBuf,
    start: Instant,
    state: Mutex<CaptureState>,
}

impl Capture {
    fn new(directory: &Path) -> io::Result<Self> {
        Ok(Self {
            directory: directory.into(),
            start: Instant::now(),
            state: Mutex::new(CaptureState {
                trace: File::create(directory.join("trace.ndjson"))?,
                phase: "warmup",
                connection_id: None,
                events: 0,
                bytes: 0,
                requests: 0,
                passive_requests: 0,
            }),
        })
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, CaptureState>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("capture lock"))
    }

    fn phase(&self, phase: &'static str, connection: u64) -> io::Result<()> {
        let mut state = self.lock()?;
        state.phase = phase;
        state.connection_id = Some(connection);
        self.event(&mut state, json!({"kind": "phase"}), &[])?;
        Ok(())
    }

    // Budgets include files and NDJSON. An archive failure stops the session.
    fn event(
        &self,
        state: &mut CaptureState,
        mut event: Value,
        files: &[(&str, &[u8])],
    ) -> io::Result<()> {
        let stem = format!("event-{:05}", state.events);
        event["id"] = json!(state.events);
        event["stem"] = json!(stem);
        event["phase"] = json!(state.phase);
        event["connection_id"] = json!(state.connection_id);
        event["utc"] = json!(Utc::now());
        event["elapsed_ms"] = json!(self.start.elapsed().as_millis());
        event["requests_so_far"] = json!(state.requests);
        let mut line = serde_json::to_vec(&event)?;
        line.push(b'\n');
        let bytes = line.len() + files.iter().map(|(_, data)| data.len()).sum::<usize>();
        if state.events >= MAX_EVENTS || bytes > MAX_BYTES.saturating_sub(state.bytes) {
            return Err(io::Error::other("0547 evidence budget exhausted"));
        }
        for (suffix, data) in files {
            std::fs::write(self.directory.join(format!("{stem}.{suffix}")), data)?;
        }
        state.trace.write_all(&line)?;
        state.trace.flush()?;
        state.events += 1;
        state.bytes += bytes;
        Ok(())
    }
}

impl FrameObserver for Capture {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        let mut state = self.lock()?;
        match event {
            FrameEvent::Request { endpoint, request } => {
                state.requests += 1;
                if state.phase == "passive" {
                    state.passive_requests += 1;
                }
                self.event(
                    &mut state,
                    json!({"kind": "request", "endpoint": endpoint}),
                    &[("request.bin", request)],
                )?;
            }
            FrameEvent::Response {
                endpoint,
                request,
                header,
                body,
            } => {
                let payload = decode_frame(header, body, true)?;
                let xor: Vec<_> = payload.iter().map(|b| b ^ 0x93).collect();
                let mut files = vec![
                    ("request.bin", request),
                    ("header.bin", header.as_slice()),
                    ("wire.bin", body),
                    ("payload.bin", payload.as_slice()),
                ];
                if header[10..12] == QUOTE_REFRESH_COMMAND.to_le_bytes() {
                    files.push(("xor.bin", &xor));
                }
                self.event(
                    &mut state,
                    json!({"kind": "response", "endpoint": endpoint}),
                    &files,
                )?;
            }
            FrameEvent::Failed {
                endpoint,
                request,
                kind,
            } => {
                self.event(
                    &mut state,
                    json!({"kind": "failed", "endpoint": endpoint, "error_kind": format!("{kind:?}")}),
                    &[("request.bin", request)],
                )?;
            }
        }
        Ok(())
    }
}

impl PushObserver for Capture {
    fn observe(&self, endpoint: SocketAddr, push: &QuotePush) -> io::Result<()> {
        let decoded = serde_json::to_vec_pretty(&push.refresh)?;
        let mut state = self.lock()?;
        self.event(
            &mut state,
            json!({
                "kind": "push", "endpoint": endpoint, "received_at": push.received_at,
                "push_connection_id": push.connection_id, "generation": push.generation,
                "message_id": push.message_id,
            }),
            &[
                ("header.bin", &push.header),
                ("wire.bin", &push.wire_body),
                ("payload.bin", &push.refresh.page.raw_payload),
                ("xor.bin", &push.refresh.decoded_payload),
                ("decoded.json", &decoded),
            ],
        )?;
        Ok(())
    }
}

// Compare known market fields, excluding the cursor and opaque/status slots.
fn market_changed(previous: &BasicQuote, current: &BasicQuote) -> bool {
    previous.price_fields_raw != current.price_fields_raw
        || previous.total_volume_raw != current.total_volume_raw
        || previous.current_volume_raw != current.current_volume_raw
        || previous.amount_raw != current.amount_raw
        || previous.inside_volume_raw != current.inside_volume_raw
        || previous.outside_volume_raw != current.outside_volume_raw
        || previous.bids != current.bids
        || previous.asks != current.asks
}

fn advancing_change(previous: &BasicQuote, current: &BasicQuote) -> bool {
    matches!((previous.update_cursor, current.update_cursor), (Some(a), Some(b)) if b > a)
        && market_changed(previous, current)
}

fn assess_rows(
    previous: &mut BTreeMap<(u8, String), BasicQuote>,
    rows: &[BasicQuote],
    received_at: DateTime<Utc>,
    started: DateTime<Utc>,
) -> (bool, Vec<Value>) {
    let after_guard = received_at >= started + chrono::Duration::seconds(GUARD_SECONDS as i64);
    let mut changes = Vec::new();
    for row in rows {
        let key = (row.market_raw, row.code.clone());
        if previous
            .get(&key)
            .is_some_and(|old| advancing_change(old, row))
        {
            changes.push(json!({"market": row.market_raw, "code": row.code,
                "previous_cursor": previous[&key].update_cursor, "cursor": row.update_cursor}));
        }
        // Notifications queued before the baseline must not roll this view back.
        if previous
            .get(&key)
            .is_none_or(|old| row.update_cursor >= old.update_cursor)
        {
            previous.insert(key, row.clone());
        }
    }
    (after_guard, changes)
}

fn acceptance(
    allow_closed: bool,
    trading_window: bool,
    complete: bool,
    passive_requests: usize,
    accepted_frames: usize,
    final_synchronized: bool,
) -> &'static str {
    if allow_closed {
        "diagnostic_not_verified"
    } else if !trading_window {
        "market_closed"
    } else if complete && passive_requests == 0 && accepted_frames >= 2 && final_synchronized {
        "passed"
    } else {
        "not_verified"
    }
}

fn capture(
    output: &Path,
    endpoint: SocketAddr,
    seconds: u64,
    allow_closed: bool,
    summary: &mut Value,
) -> io::Result<()> {
    let operation = OperationContext::new(Duration::from_secs(seconds + 120), None)?;
    let _guard = operation.enter();
    let pool = IpPool::new(vec![endpoint])?;
    save(&output.join("daily-pool.json"), &pool.refresh()?)?;
    let observer = Arc::new(Capture::new(output)?);
    let config = TcpConfig {
        ip: Some(endpoint),
        ip_pool: Some(pool),
        timeout: Duration::from_secs(4),
        auto_reconnect: 0,
        ..Default::default()
    };
    let mut tcp = Tcp::with_config_and_observer(&config, observer.clone())?;
    tcp.set_heartbeat_interval(None)?;
    let connection = tcp.connection_id().get();
    summary["connection_id"] = json!(connection);
    summary["local_address"] = json!(tcp.get_ref().0.local_addr()?);
    observer.phase("warmup", connection)?;
    let mut precision = Vec::new();
    for market in [0, 1] {
        let rows = security_directory(&mut tcp, market)?;
        for &(m, code) in STOCKS.iter().filter(|(m, _)| *m == market) {
            let row = rows
                .iter()
                .find(|row| row.code == code)
                .ok_or_else(|| io::Error::other("subscription symbol missing from directory"))?;
            precision.push((m, code, row.decimal));
        }
        println!("044d market={market}: {} entries checked", rows.len());
    }
    let metadata = Arc::new(PrecisionSnapshot::from_decimals(
        &precision,
        Utc::now(),
        Some(endpoint),
    )?);
    save(&output.join("precision.json"), &precision)?;
    // Recheck after warmup so a late baseline cannot cross lunch/close.
    if !allow_closed && !window_open(local(Utc::now()), seconds + 5)? {
        summary["status"] = json!("market_closed");
        return Ok(());
    }
    observer.phase("baseline", connection)?;
    let mut subscription = QuoteSubscription::from_tcp(
        tcp,
        RefreshRequest::from_stocks(&STOCKS)?,
        metadata,
        SubscriptionOptions::default(),
    )?;
    subscription.set_push_observer(Some(observer.clone()));
    let baseline = subscription.refresh()?;
    save(&output.join("baseline.json"), &baseline)?;
    let snapshot = subscription.snapshot();
    save(&output.join("baseline-snapshot.json"), &snapshot)?;
    summary["baseline_complete"] = json!(snapshot.status == DepthSyncStatus::Synchronized);
    if snapshot.status != DepthSyncStatus::Synchronized {
        subscription.close();
        summary["status"] = json!("baseline_incomplete");
        return Ok(());
    }
    observer.phase("passive", connection)?;
    let started = Utc::now();
    let start = Instant::now();
    summary["passive_started_utc"] = json!(started);
    save(&output.join("summary.json"), summary)?;
    let mut previous: BTreeMap<_, _> = baseline
        .page
        .records
        .into_iter()
        .map(|q| ((q.market_raw, q.code.clone()), q))
        .collect();
    // Include any valid notifications preceding the baseline response.
    for state in &snapshot.states {
        if let Some(q) = &state.quote {
            previous.insert((q.market_raw, q.code.clone()), q.clone());
        }
    }
    let mut observed = 0usize;
    let mut accepted = 0usize;
    let mut next_progress = Duration::ZERO;
    let mut window = window_open(local(started), seconds)?;
    let result = (|| -> io::Result<()> {
        while start.elapsed() < Duration::from_secs(seconds) {
            window &= session_at(local(Utc::now()))? == session_at(local(started))?
                && session_at(local(started))?.is_some();
            if !allow_closed && !window {
                return Err(io::Error::other(
                    "passive observation left the trading session",
                ));
            }
            let remaining = Duration::from_secs(seconds).saturating_sub(start.elapsed());
            if let Some(push) = subscription.poll_push(remaining.min(Duration::from_millis(500)))? {
                observed += 1;
                if push.connection_id != connection || push.generation != snapshot.generation {
                    return Err(io::Error::other(
                        "subscription connection/generation changed",
                    ));
                }
                let (after_guard, changes) = assess_rows(
                    &mut previous,
                    &push.refresh.page.records,
                    push.received_at,
                    started,
                );
                let qualifies = after_guard && !changes.is_empty();
                accepted += usize::from(qualifies);
                let mut state = observer.lock()?;
                observer.event(
                    &mut state,
                    json!({"kind": "assessment", "message_id": push.message_id,
                        "received_at": push.received_at, "after_guard": after_guard,
                        "qualifies": qualifies, "changes": changes}),
                    &[],
                )?;
            }
            if start.elapsed() >= next_progress {
                println!(
                    "passive {}s/{seconds}s: frames={observed}, advancing_changes={accepted}, requests={}",
                    start.elapsed().as_secs(),
                    observer.lock()?.passive_requests
                );
                next_progress = start.elapsed() + Duration::from_secs(25);
            }
        }
        Ok(())
    })();
    let finished = Utc::now();
    window &= same_session(local(started), local(finished))?;
    let observation_complete = result.is_ok()
        && start.elapsed() >= Duration::from_secs(seconds)
        && finished - started >= chrono::Duration::milliseconds((seconds * 1000) as i64 - 100);
    let final_snapshot = subscription.snapshot();
    let final_synchronized = final_snapshot.status == DepthSyncStatus::Synchronized
        && final_snapshot.connection_id == Some(connection)
        && final_snapshot.generation == snapshot.generation
        && final_snapshot.states.len() == STOCKS.len()
        && final_snapshot
            .states
            .iter()
            .all(|state| state.quote.is_some());
    save(&output.join("final-snapshot.json"), &final_snapshot)?;
    subscription.close();
    summary["passive_finished_utc"] = json!(finished);
    summary["passive_elapsed_ms"] = json!(start.elapsed().as_millis());
    summary["passive_frames"] = json!(observed);
    summary["advancing_change_frames"] = json!(accepted);
    summary["trading_window_complete"] = json!(window);
    summary["observation_complete"] = json!(observation_complete);
    summary["final_synchronized"] = json!(final_synchronized);
    let state = observer.lock()?;
    summary["events"] = json!(state.events);
    summary["archive_bytes"] = json!(state.bytes);
    summary["requests_total"] = json!(state.requests);
    summary["passive_requests"] = json!(state.passive_requests);
    summary["status"] = json!(acceptance(
        allow_closed,
        window,
        observation_complete,
        state.passive_requests,
        accepted,
        final_synchronized
    ));
    result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let usage = "validate_quote_push <new-dir> <ip:port> [--seconds 180] [--allow-closed]";
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or(usage)?);
    let endpoint: SocketAddr = args.next().ok_or(usage)?.parse()?;
    let mut seconds = 180u64;
    let mut allow_closed = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => seconds = args.next().ok_or(usage)?.parse()?,
            "--allow-closed" => allow_closed = true,
            _ => return Err(usage.into()),
        }
    }
    if !(30..=600).contains(&seconds) {
        return Err("seconds must be in 30..=600".into());
    }
    std::fs::create_dir(&output)?;
    let started = Utc::now();
    let mut summary = json!({
        "schema": 1, "opcode": "0x0547", "source": "live_tcp",
        "endpoint": endpoint, "started_utc": started, "calendar": calendar(started),
        "requested_seconds": seconds, "guard_seconds": GUARD_SECONDS,
        "allow_closed": allow_closed, "auto_reconnect": 0, "heartbeat": false,
        "symbols": STOCKS, "baseline_complete": false,
        "passive_requests": 0, "passive_frames": 0, "advancing_change_frames": 0,
        "observation_complete": false, "status": "starting",
        "criteria": "full trading session window; complete baseline; same connection/generation; zero passive requests; >=2 frames after 5s with strictly advancing cursors and known price/volume/amount/depth changes",
    });
    save(&output.join("summary.json"), &summary)?;
    if !allow_closed && !window_open(local(started), seconds + 120).unwrap_or(false) {
        summary["status"] = json!(if summary["calendar"]["is_trading_day"].is_null() {
            "calendar_uncovered"
        } else {
            "market_closed"
        });
    } else if let Err(error) = capture(&output, endpoint, seconds, allow_closed, &mut summary) {
        summary["status"] = json!("transport_error");
        summary["error"] =
            json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()});
    }
    summary["finished_utc"] = json!(Utc::now());
    save(&output.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if summary["status"] != "passed" {
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> NaiveDateTime {
        value.parse().unwrap()
    }

    #[test]
    fn strict_gate_rejects_weekend_holiday_lunch_close_and_crossing() {
        for value in [
            "2026-10-24T14:00:00",
            "2026-10-01T10:00:00",
            "2026-10-26T12:00:00",
            "2026-10-26T11:30:00",
            "2026-10-26T15:00:00",
            "2026-10-26T11:29:50",
        ] {
            assert!(!window_open(at(value), 180).unwrap(), "{value}");
        }
        assert!(window_open(at("2026-10-26T09:35:00"), 300).unwrap());
        assert!(window_open(at("2026-10-26T13:05:00"), 300).unwrap());
        assert!(window_open(at("2027-01-04T10:00:00"), 180).is_err());
        assert_eq!(
            TradingCalendar::try_next_trading_day(&at("2026-10-24T14:00:00").date()).unwrap(),
            at("2026-10-26T09:35:00").date()
        );
    }

    #[test]
    fn acceptance_never_promotes_diagnostics_or_incomplete_observations() {
        assert_eq!(acceptance(false, true, true, 0, 2, true), "passed");
        assert_ne!(acceptance(false, true, true, 0, 2, false), "passed");
        for (closed, window, complete, requests, frames) in [
            (true, true, true, 0, 2),
            (false, false, true, 0, 2),
            (false, true, false, 0, 2),
            (false, true, true, 1, 2),
            (false, true, true, 0, 1),
        ] {
            assert_ne!(
                acceptance(closed, window, complete, requests, frames, true),
                "passed"
            );
        }
    }

    #[test]
    fn cursor_only_duplicate_and_opaque_tail_do_not_prove_market_changes() {
        let metadata = protocol::metadata();
        let request = RefreshRequest::from_stocks(&[(0, "000001")]).unwrap();
        let old = decode_quote_refresh(&request, &protocol::refresh_payload(10), &metadata)
            .unwrap()
            .page
            .records
            .remove(0);
        let mut new = old.clone();
        new.update_cursor = Some(11);
        new.tail_raw = vec![99];
        new.active_raw += 1;
        new.status_or_unknown_raw += 1;
        assert!(!advancing_change(&old, &new));
        new.bids[0].volume_raw += 1;
        assert!(advancing_change(&old, &new));
        new.update_cursor = Some(10);
        assert!(!advancing_change(&old, &new));
        new.update_cursor = Some(9);
        assert!(!advancing_change(&old, &new));
    }

    fn baseline_quote() -> BasicQuote {
        decode_quote_refresh(
            &RefreshRequest::from_stocks(&[(0, "000001")]).unwrap(),
            &protocol::refresh_payload(10),
            &protocol::metadata(),
        )
        .unwrap()
        .page
        .records
        .remove(0)
    }

    #[test]
    fn synthetic_trading_window_requires_two_post_guard_changes_not_duplicate_frames() {
        // This injected timestamp is a test clock, not an observed live session.
        let started: DateTime<Utc> = "2026-10-26T01:35:00Z".parse().unwrap();
        let trading = window_open(local(started), 30).unwrap();
        assert!(trading);
        let baseline = baseline_quote();
        let mut previous = BTreeMap::from([((0, "000001".to_owned()), baseline.clone())]);
        let mut accepted = 0;
        for (second, cursor, volume, expected_count) in [
            (4, 11, 101, 0), // Valid update inside the guard becomes the new reference.
            (6, 11, 101, 0), // Delayed duplicate of that update cannot prove a change.
            (7, 12, 102, 1),
            (8, 12, 102, 1),
            (9, 13, 103, 2),
        ] {
            let mut row = baseline.clone();
            row.update_cursor = Some(cursor);
            row.total_volume_raw = volume;
            let (guard, changes) = assess_rows(
                &mut previous,
                &[row],
                started + chrono::Duration::seconds(second),
                started,
            );
            accepted += usize::from(guard && !changes.is_empty());
            assert_eq!(accepted, expected_count);
            assert_eq!(previous[&(0, "000001".to_owned())].total_volume_raw, volume);
            assert_eq!(
                acceptance(false, trading, true, 0, accepted, true),
                if accepted >= 2 {
                    "passed"
                } else {
                    "not_verified"
                },
            );
        }
        assert_ne!(
            acceptance(false, trading, true, 1, accepted, true),
            "passed"
        );
        assert_ne!(acceptance(true, trading, true, 0, accepted, true), "passed");
        assert_ne!(
            acceptance(false, trading, false, 0, accepted, true),
            "passed"
        );
        assert_ne!(
            acceptance(false, trading, true, 0, accepted, false),
            "passed"
        );
    }

    #[test]
    fn guard_boundary_and_old_queued_notification_do_not_corrupt_the_reference() {
        let started: DateTime<Utc> = "2026-10-26T01:35:00Z".parse().unwrap();
        let baseline = baseline_quote();
        let key = (0, "000001".to_owned());
        let mut previous = BTreeMap::from([(key.clone(), baseline.clone())]);
        let mut queued = baseline.clone();
        queued.update_cursor = Some(9);
        queued.total_volume_raw = 90;
        let (guard, changes) = assess_rows(
            &mut previous,
            &[queued],
            started - chrono::Duration::seconds(1),
            started,
        );
        assert!(!guard && changes.is_empty());
        assert_eq!(previous[&key].update_cursor, Some(10));
        let mut new = baseline;
        new.update_cursor = Some(11);
        new.bids[4].volume_raw += 1;
        let before_guard =
            started + chrono::Duration::seconds(5) - chrono::Duration::nanoseconds(1);
        let mut before = previous.clone();
        let (guard, changes) = assess_rows(&mut before, &[new.clone()], before_guard, started);
        assert!(!guard && !changes.is_empty());
        let (guard, changes) = assess_rows(
            &mut previous,
            &[new],
            started + chrono::Duration::seconds(5),
            started,
        );
        assert!(guard && !changes.is_empty());
        assert_eq!(previous[&key].update_cursor, Some(11));
    }

    #[test]
    fn exact_session_endpoints_and_utc_to_beijing_conversion_are_checked() {
        let utc: DateTime<Utc> = "2026-10-26T01:30:00Z".parse().unwrap();
        assert_eq!(local(utc), at("2026-10-26T09:30:00"));
        for value in [
            "2026-10-26T09:30:00",
            "2026-10-26T13:00:00",
            "2026-10-26T11:29:29",
            "2026-10-26T14:59:29",
        ] {
            assert!(window_open(at(value), 30).unwrap(), "{value}");
        }
        for value in [
            "2026-10-26T09:29:59",
            "2026-10-26T11:29:30",
            "2026-10-26T14:59:30",
        ] {
            assert!(!window_open(at(value), 30).unwrap(), "{value}");
        }
        assert!(!same_session(at("2026-10-26T11:29:29.9"), at("2026-10-26T11:30:00.1")).unwrap());
        assert!(!same_session(at("2026-10-26T09:35:00"), at("2026-10-26T09:34:59")).unwrap());
        assert!(same_session(at("2026-10-26T11:29:29.9"), at("2026-10-26T11:29:59.9")).unwrap());
    }
}
