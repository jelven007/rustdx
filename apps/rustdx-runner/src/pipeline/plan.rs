//! Read-only planning against explicit inputs. Execution must freeze and validate them again.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use chrono::NaiveDate;
use rustdx_complete::calendar::CalendarProvider;
use rustdx_complete::limit::TradingState;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{facts, tcp_request};
use crate::{
    Result,
    config::{Config, MAX_JSON},
    database::capacity::{Capacity, IO_LIMIT},
    invalid, storage,
};

// Fixed canonical fields, six finite f64s and bounded ASCII dataset/release IDs.
// This is an upper bound, not a measurement of the not-yet-collected rows.
const PLAN_ROW_BYTES: u64 = 1024;

fn database_plan(config: &Config, rows: u64) -> Result<Value> {
    let capacity = Capacity::estimate(
        rows,
        rows.checked_mul(PLAN_ROW_BYTES)
            .ok_or_else(|| invalid("plan database row byte overflow"))?,
    )?;
    let mut usage = storage::TreeUsage::default();
    let mut reasons = Vec::new();
    for target in &config.database_targets {
        if let Err(error) = capacity.check(0, usage, config.max_workspace_bytes) {
            reasons.push(format!("{}:{error}", target.id()));
        }
        usage.bytes = usage
            .bytes
            .checked_add(capacity.evidence_bytes)
            .ok_or_else(|| invalid("plan database evidence byte overflow"))?;
        usage.entries = usage
            .entries
            .checked_add(capacity.evidence_entries)
            .ok_or_else(|| invalid("plan database evidence entry overflow"))?;
    }
    Ok(json!({
        "per_target":capacity,"targets":config.database_targets.iter().map(|t| t.id()).collect::<Vec<_>>(),
        "all_target_evidence_bytes":usage.bytes,"all_target_evidence_entries":usage.entries,
        "evidence_byte_headroom":config.max_workspace_bytes.saturating_sub(usage.bytes),
        "budget_fits":reasons.is_empty(),"blockers":reasons,
        "limits":{"per_target_operations":IO_LIMIT,"per_target_proof_bytes":MAX_JSON,
            "all_target_entries":storage::MAX_TREE_ENTRIES,"all_target_bytes":config.max_workspace_bytes},
        "row_bytes_upper_bound":PLAN_ROW_BYTES,
        "scope":"new release, publication and one recovery; runtime remeasures sealed row bytes and existing evidence; consumer audit is separate",
    }))
}

fn history_windows(config: &Config, days: &BTreeMap<NaiveDate, (u64, u64)>) -> Result<Vec<Value>> {
    let mut windows = Vec::new();
    let mut from = config.from;
    let mut expected = 0;
    let mut trading = 0;
    let mut has_day = false;
    let window = |from, through, expected, trading| -> Result<Value> {
        let database = database_plan(config, trading)?;
        Ok(
            json!({"from":from,"through":through,"expected_keys":expected,"trading_rows":trading,
            "row_budget_fits":expected <= config.max_rows,
            "database_budget_fits":database["budget_fits"],
            "all_target_evidence_bytes":database["all_target_evidence_bytes"]}),
        )
    };
    for (&date, &(day_expected, day_trading)) in days {
        if has_day
            && (expected + day_expected > config.max_rows
                || database_plan(config, trading + day_trading)?["budget_fits"] == false)
        {
            windows.push(window(
                from,
                date.pred_opt()
                    .ok_or_else(|| invalid("plan date underflow"))?,
                expected,
                trading,
            )?);
            from = date;
            expected = 0;
            trading = 0;
        }
        expected += day_expected;
        trading += day_trading;
        has_day = true;
    }
    windows.push(window(from, config.through, expected, trading)?);
    Ok(windows)
}

fn bytes(path: &Path) -> Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Err(invalid("plan input must be a regular file"));
    }
    Ok(meta.len())
}

pub fn preflight_plan(config: &Config, connection_limit: usize, max_keys: usize) -> Result<Value> {
    config.validate()?;
    if connection_limit == 0 || !(1..=100_000).contains(&max_keys) {
        return Err(invalid(
            "plan requires a positive connection limit and 1..100000 key samples",
        ));
    }
    let facts = facts::load_config(config)?;
    let mut keys = Vec::new();
    let mut key_hash = Sha256::new();
    let mut expected = 0_u64;
    let mut trading = 0_u64;
    let mut suspended = 0_u64;
    let mut delisted = 0_u64;
    let mut trading_codes = BTreeSet::new();
    let mut days: BTreeMap<_, _> = facts
        .calendar
        .trading_days_between(config.from, config.through)
        .map_err(|e| invalid(e.to_string()))?
        .into_iter()
        .map(|date| (date, (0_u64, 0_u64)))
        .collect();
    let mut enumeration = config.clone();
    enumeration.max_rows = i64::MAX as u64;
    facts::visit_expected(&enumeration, &facts, |date, code, state| {
        let day = days.entry(date).or_default();
        day.0 += 1;
        let state = match state {
            TradingState::Trading => {
                trading += 1;
                day.1 += 1;
                trading_codes.insert(code.to_owned());
                "trading"
            }
            TradingState::Suspended => {
                suspended += 1;
                "suspended"
            }
            TradingState::Delisted => {
                delisted += 1;
                "delisted"
            }
        };
        let key = json!({"date":date,"market":&code[..2],"code":&code[2..],
            "instrument_class":"a_share","state":state});
        key_hash.update(serde_json::to_vec(&key)?);
        key_hash.update(b"\n");
        if keys.len() < max_keys {
            keys.push(key);
        }
        expected += 1;
        Ok(())
    })?;
    let mut digests = BTreeSet::from([
        config.calendar_source_sha256.clone(),
        facts.universe.source_sha256.clone(),
    ]);
    for interval in &facts.history.dataset().intervals {
        digests.insert(interval.announcement.sha256.to_ascii_lowercase());
    }
    if let Some(source) = &config.tcp_day {
        digests.insert(source.volume_evidence_sha256.clone());
    }
    let mut fact_bytes = 0_u64;
    for digest in digests {
        crate::config::digest(&digest)?;
        let path = config.facts_directory.join(&digest);
        bytes(&path)?;
        let (actual, size) = storage::hash_file(&path, config.max_source_bytes)?;
        if actual != digest {
            return Err(invalid("plan announcement digest differs"));
        }
        fact_bytes = fact_bytes
            .checked_add(size)
            .ok_or_else(|| invalid("plan size overflow"))?;
    }
    let mut source_bytes = fact_bytes;
    let mut blockers = Vec::new();
    if expected > config.max_rows {
        blockers.push("expected_row_budget".into());
    }
    if config.tcp_day.is_none() && trading != 0 {
        for member in &facts.universe.members {
            if member.through < config.from || member.from > config.through {
                continue;
            }
            let mut found = None;
            for source in &config.day_directories {
                let path = source.join(format!("{}.day", member.code));
                if path.exists() {
                    if found.is_some() {
                        return Err(invalid("plan source is ambiguous"));
                    }
                    found = Some(path);
                }
            }
            if let Some(path) = found {
                source_bytes = source_bytes
                    .checked_add(bytes(&path)?)
                    .ok_or_else(|| invalid("plan size overflow"))?;
            } else if trading_codes.contains(&member.code) {
                blockers.push(format!("missing_day_source:{}", member.code));
            }
        }
    }
    if source_bytes > config.max_source_bytes {
        blockers.push("source_byte_budget".into());
    }
    let mut snapshot_bytes = source_bytes
        .checked_add(bytes(&config.cli)?)
        .and_then(|n| n.checked_add(serde_json::to_vec_pretty(config).ok()?.len() as u64))
        .ok_or_else(|| invalid("plan snapshot size overflow"))?;
    for path in [&config.calendar, &config.security_history, &config.universe] {
        snapshot_bytes = snapshot_bytes
            .checked_add(bytes(path)?)
            .ok_or_else(|| invalid("plan size overflow"))?;
    }
    if snapshot_bytes > config.max_workspace_bytes {
        blockers.push("snapshot_workspace_budget".into());
    }
    let collection_seconds = config.timeout_seconds - config.publish_reserve_seconds;
    let tcp = if config.tcp_day.is_some() && trading != 0 {
        let request = tcp_request(
            config,
            &facts,
            config.max_source_bytes.saturating_sub(fact_bytes),
        )?;
        let mut plan = rustdx_cli::tcp_day::reservation_plan(&request, connection_limit)?;
        plan.deadline_seconds = plan.deadline_seconds.min(collection_seconds);
        if plan.effective_parallel == 0 {
            blockers.push("tcp_initial_reservation".into());
        }
        Some(plan)
    } else {
        None
    };
    // statvfs against the nearest existing ancestor; the plan never initializes root.
    let mut parent = config.root.as_path();
    while !parent.exists() {
        parent = parent
            .parent()
            .ok_or_else(|| invalid("plan root has no existing ancestor"))?;
    }
    let free = storage::free_bytes(parent)?;
    let database = database_plan(config, trading)?;
    if database["budget_fits"] == false {
        blockers.push("database_capacity".into());
    }
    let evidence_bytes = database["all_target_evidence_bytes"]
        .as_u64()
        .ok_or_else(|| invalid("plan database byte forecast missing"))?;
    if free
        < snapshot_bytes
            .saturating_add(evidence_bytes)
            .saturating_add(config.min_free_bytes)
    {
        blockers.push("disk_reserve".into());
    }
    let windows = history_windows(config, &days)?;
    let result = json!({
        "schema_version":1,"mode":"offline_plan","dataset_id":config.dataset_id,
        "contract":config.contract_version,"from":config.from,"through":config.through,
        "can_start":blockers.is_empty(),"blockers":blockers,
        "expected_keys":{"total":expected,"trading":trading,"suspended":suspended,"delisted":delisted,
            "samples":keys,"truncated":expected > max_keys as u64,
            "all_keys_jsonl_sha256":format!("{:x}",key_hash.finalize())},
        "capacity":{"fact_bytes":fact_bytes,"known_source_bytes":source_bytes,
            "minimum_snapshot_bytes":snapshot_bytes,"workspace_limit":config.max_workspace_bytes,
            "additional_space_required":"CSV, normalization SQLite/journal and sealed manifests; runtime budgets still apply",
            "free_bytes":free,"free_sample_path":parent,"min_free_bytes":config.min_free_bytes},
        "collection_deadline_seconds":collection_seconds,"publication_reserve_seconds":config.publish_reserve_seconds,
        "tcp":tcp,"collector_needed":trading != 0,"database":database,"history_windows":windows,
        "history_window_contract":"complete expected universe per date; execute chronologically with frozen facts and nondecreasing source revision/as_of; catalog continuity and all runtime budgets are revalidated",
        "qualification":"input coverage and initial reservations only; no network, deployment or full-history completion verification",
    });
    if serde_json::to_vec(&result)?.len() as u64 > MAX_JSON {
        return Err(invalid("plan report exceeds JSON byte budget"));
    }
    Ok(result)
}
