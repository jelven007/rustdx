//! Consistent local backups and isolated restore validation.
use std::{fs, path::Path, time::Duration};

use rusqlite::backup::{Backup, StepResult};
use serde::{Deserialize, Serialize};

use crate::{
    Result, config::MAX_JSON, invalid, ledger::Ledger, model::Evidence, pipeline, storage,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    schema_version: u32,
    created_at: String,
    files: Vec<Evidence>,
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    relative: &Path,
    remaining: &mut u64,
    files: &mut Vec<Evidence>,
) -> Result<()> {
    for entry in fs::read_dir(source.join(relative))? {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("backup refuses symlinks"));
        }
        if metadata.is_dir() {
            storage::private_directory(&destination.join(&path))?;
            copy_tree(source, destination, &path, remaining, files)?;
        } else if metadata.is_file() {
            let (_, bytes) =
                storage::copy_new(&source.join(&path), &destination.join(&path), *remaining)?;
            *remaining -= bytes;
            files.push(storage::evidence(
                destination,
                &destination.join(path),
                bytes,
            )?);
            if files.len() > 200_000 {
                return Err(invalid("backup file count exceeded"));
            }
        } else {
            return Err(invalid("backup refuses special files"));
        }
    }
    Ok(())
}

/// Holds the same controller lock as collection/publication. SQLite's online API
/// includes committed WAL content; copying only ledger.sqlite would be incorrect.
pub fn backup(root: &Path, destination: &Path, max_bytes: u64) -> Result<()> {
    let _lock = storage::RootLock::acquire(root)?;
    if destination.exists() || destination.starts_with(root) {
        return Err(invalid(
            "backup destination must be new and outside the live root",
        ));
    }
    let ledger = Ledger::open(&root.join("ledger.sqlite"))?;
    if ledger
        .unfinished()?
        .iter()
        .any(|a| a.state == "running" || a.pid.is_some_and(crate::supervisor::process_group_alive))
    {
        return Err(invalid(
            "recover active/ambiguous workers before taking a consistent backup",
        ));
    }
    fs::create_dir(destination)?;
    storage::private_directory(destination)?;
    let mut target = rusqlite::Connection::open(destination.join("ledger.sqlite"))?;
    {
        let backup = Backup::new(&ledger.connection, &mut target)?;
        loop {
            rustdx_complete::operation::check().map_err(crate::Error::from)?;
            match backup.step(128)? {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => return Err(invalid("unsupported SQLite backup result")),
            }
        }
    }
    drop(target);
    fs::File::open(destination.join("ledger.sqlite"))?.sync_all()?;
    let db = storage::evidence(destination, &destination.join("ledger.sqlite"), max_bytes)?;
    let mut remaining = max_bytes - db.bytes;
    let mut files = vec![db];
    for name in ["attempts", "releases", "catalogs", "database"] {
        if root.join(name).exists() {
            storage::private_directory(&destination.join(name))?;
            copy_tree(
                root,
                destination,
                Path::new(name),
                &mut remaining,
                &mut files,
            )?;
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let inventory = Inventory {
        schema_version: 1,
        created_at: crate::now(),
        files,
    };
    let bytes = serde_json::to_vec_pretty(&inventory)?;
    if bytes.len() as u64 > MAX_JSON {
        return Err(invalid("backup inventory exceeds JSON budget"));
    }
    storage::write_new(&destination.join("backup.json"), &bytes)?;
    storage::sync_directory(destination)?;
    Ok(())
}

pub fn restore(source: &Path, destination: &Path, max_bytes: u64) -> Result<()> {
    if destination.exists() || destination.starts_with(source) {
        return Err(invalid(
            "restore destination must be a new isolated directory",
        ));
    }
    let inventory: Inventory = serde_json::from_slice(&storage::read_limited(
        &source.join("backup.json"),
        MAX_JSON,
    )?)?;
    if inventory.schema_version != 1 || inventory.files.len() > 200_000 {
        return Err(invalid("unsupported backup inventory"));
    }
    let mut paths = std::collections::BTreeSet::new();
    let mut remaining = max_bytes;
    for file in &inventory.files {
        if !paths.insert(&file.path) || file.bytes > remaining {
            return Err(invalid(
                "duplicate backup path or restore byte limit exceeded",
            ));
        }
        storage::verify(source, file)?;
        remaining -= file.bytes;
    }
    if !paths.contains(&"ledger.sqlite".to_string()) {
        return Err(invalid("backup has no ledger"));
    }
    fs::create_dir(destination)?;
    storage::private_directory(destination)?;
    for file in &inventory.files {
        let path = Path::new(&file.path);
        // Validate paths before any mkdir or write at the destination.
        storage::safe_join(source, &file.path)?;
        if let Some(parent) = path.parent() {
            storage::private_directory(&destination.join(parent))?;
        }
        storage::copy_new(&source.join(path), &destination.join(path), file.bytes)?;
        storage::verify(destination, file)?;
    }
    validate_restore(destination)?;
    if destination.join("releases").exists() {
        for entry in fs::read_dir(destination.join("releases"))? {
            storage::freeze_tree(&entry?.path())?;
        }
    }
    storage::write_new(
        &destination.join("restore-validated.json"),
        &serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"validated_at":crate::now(),"backup_created_at":inventory.created_at}),
        )?,
    )?;
    Ok(())
}

pub fn validate_restore(root: &Path) -> Result<()> {
    let ledger = Ledger::open(&root.join("ledger.sqlite"))?;
    let integrity: String = ledger
        .connection
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(invalid("SQLite integrity check failed"));
    }
    let mut query = ledger.connection.prepare("PRAGMA foreign_key_check")?;
    if query.query([])?.next()?.is_some() {
        return Err(invalid("ledger references invalid"));
    }
    let mut query = ledger.connection.prepare("SELECT id FROM releases")?;
    let mut releases = query.query([])?;
    while let Some(row) = releases.next()? {
        let release: String = row.get(0)?;
        pipeline::load_release(root, &ledger.release_evidence(&release)?)?;
    }
    let mut query = ledger
        .connection
        .prepare("SELECT request,response FROM target_io")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        for index in [0, 1] {
            if let Some(text) = row.get::<_, Option<String>>(index)? {
                storage::verify(root, &serde_json::from_str::<Evidence>(&text)?)?;
            }
        }
    }
    let mut query = ledger
        .connection
        .prepare("SELECT dataset,version FROM publications ORDER BY dataset,version")?;
    let mut publications = query.query([])?;
    while let Some(row) = publications.next()? {
        let dataset: String = row.get(0)?;
        let version = crate::ledger::unsigned(row, 1)?;
        let catalog = pipeline::fixed_catalog(root, &ledger, &dataset, Some(version))?
            .ok_or_else(|| invalid("missing published catalog"))?;
        for (release, expected) in &catalog.database_releases {
            let receipts = crate::database::receipts(root, &ledger, release)?;
            if serde_json::to_vec(&receipts)? != serde_json::to_vec(expected)? {
                return Err(invalid("restored database receipt differs from catalog"));
            }
        }
        let mut last_manifest: Option<crate::model::ReleaseManifest> = None;
        for partition in catalog.partitions.values() {
            crate::config::identifier(&partition.release_id)?;
            if last_manifest
                .as_ref()
                .is_none_or(|m| m.release_id != partition.release_id)
            {
                last_manifest = Some(pipeline::release_manifest(
                    root,
                    &ledger.release_evidence(&partition.release_id)?,
                )?);
            }
            let manifest = last_manifest
                .as_ref()
                .ok_or_else(|| invalid("missing manifest"))?;
            if manifest.dataset_id != dataset
                || serde_json::to_vec(
                    manifest
                        .partitions
                        .get(&partition.date)
                        .ok_or_else(|| invalid("catalog references absent partition"))?,
                )? != serde_json::to_vec(partition)?
            {
                return Err(invalid("catalog partition differs from release"));
            }
        }
    }
    Ok(())
}

pub fn metrics(ledger: &Ledger) -> Result<String> {
    let mut text = String::from("# TYPE rustdx_jobs gauge\n");
    let mut query = ledger
        .connection
        .prepare("SELECT state,count(*) FROM jobs GROUP BY state")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        let state: String = row.get(0)?;
        if !state.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            return Err(invalid("invalid metric state label"));
        }
        text.push_str(&format!(
            "rustdx_jobs{{state=\"{state}\"}} {}\n",
            crate::ledger::unsigned(row, 1)?
        ));
    }
    let events: u64 = ledger
        .connection
        .query_row("SELECT count(*) FROM events", [], |r| {
            crate::ledger::unsigned(r, 0)
        })?;
    text.push_str(&format!(
        "# TYPE rustdx_events_total counter\nrustdx_events_total {events}\n"
    ));
    let pending: u64 = ledger.connection.query_row(
        "SELECT count(*) FROM attempts WHERE state IN ('running','evaluated','reconciliation_required')", [], |r| crate::ledger::unsigned(r, 0))?;
    text.push_str(&format!(
        "# TYPE rustdx_unfinished_attempts gauge\nrustdx_unfinished_attempts {pending}\n"
    ));
    text.push_str("# TYPE rustdx_publication_timestamp_seconds gauge\n");
    let mut query = ledger.connection.prepare(
        "SELECT p.dataset,p.committed FROM publications p JOIN pointers c ON p.dataset=c.dataset AND p.version=c.version")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        let dataset: String = row.get(0)?;
        crate::config::identifier(&dataset)?;
        let committed: String = row.get(1)?;
        let time = chrono::DateTime::parse_from_rfc3339(&committed)
            .map_err(|_| invalid("invalid publication timestamp"))?;
        text.push_str(&format!(
            "rustdx_publication_timestamp_seconds{{dataset=\"{dataset}\"}} {}\n",
            time.timestamp()
        ));
    }
    text.push_str(&format!(
        "# TYPE rustdx_metrics_timestamp_seconds gauge\nrustdx_metrics_timestamp_seconds {}\n",
        chrono::Utc::now().timestamp()
    ));
    text.push_str(&observation_metrics(ledger)?);
    Ok(text)
}

fn observation_metrics(ledger: &Ledger) -> Result<String> {
    use std::collections::BTreeMap;
    let mut totals: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut stages: BTreeMap<(String, String), (u64, u64, f64)> = BTreeMap::new();
    let mut query = ledger
        .connection
        .prepare("SELECT owner,payload FROM attempt_observations ORDER BY created,attempt")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        let owner: String = row.get(0)?;
        if !matches!(owner.as_str(), "worker" | "controller") {
            return Err(invalid("invalid observation metric owner"));
        }
        let payload: String = row.get(1)?;
        if payload.len() > 1024 * 1024 {
            return Err(invalid("oversized persisted observation"));
        }
        let record: serde_json::Value = serde_json::from_str(&payload)?;
        let status = record["status"]
            .as_str()
            .ok_or_else(|| invalid("observation status missing"))?;
        if !matches!(status, "complete" | "failed" | "unavailable") {
            return Err(invalid("invalid persisted observation status"));
        }
        *totals
            .entry((owner.clone(), format!("records_{status}")))
            .or_default() += 1;
        if status == "unavailable" {
            continue;
        }
        for field in [
            "requests",
            "request_failures",
            "empty_responses",
            "acquisition_timeouts",
            "queue_wait_micros",
        ] {
            let count = record["network"][field]
                .as_u64()
                .ok_or_else(|| invalid("invalid network observation count"))?;
            let total = totals.entry((owner.clone(), field.into())).or_default();
            *total = total
                .checked_add(count)
                .ok_or_else(|| invalid("metric count overflow"))?;
        }
        if let Some(observations) = record["observations"]["stages"].as_object() {
            for (stage, value) in observations {
                if !matches!(
                    stage.as_str(),
                    "source"
                        | "download"
                        | "extract"
                        | "snapshot"
                        | "parse"
                        | "sort"
                        | "write"
                        | "readback"
                ) {
                    return Err(invalid("invalid observation stage label"));
                }
                let total = stages.entry((owner.clone(), stage.clone())).or_default();
                let calls = value["calls"]
                    .as_u64()
                    .ok_or_else(|| invalid("invalid stage calls"))?;
                let failures = value["failures"]
                    .as_u64()
                    .ok_or_else(|| invalid("invalid stage failures"))?;
                let elapsed = value["completed_span_ms"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .ok_or_else(|| invalid("invalid stage duration"))?;
                total.0 = total
                    .0
                    .checked_add(calls)
                    .ok_or_else(|| invalid("metric overflow"))?;
                total.1 = total
                    .1
                    .checked_add(failures)
                    .ok_or_else(|| invalid("metric overflow"))?;
                total.2 += elapsed / 1000.0;
            }
        }
    }
    let mut text = String::from(
        "# Persisted per-process records; unavailable workers are not zero-network samples.\n",
    );
    for ((owner, field), count) in totals {
        text.push_str(&format!(
            "rustdx_observed_{field}_total{{owner=\"{owner}\"}} {count}\n"
        ));
    }
    for ((owner, stage), (calls, failures, seconds)) in stages {
        text.push_str(&format!(
            "rustdx_stage_calls_total{{owner=\"{owner}\",stage=\"{stage}\"}} {calls}\n\
             rustdx_stage_failures_total{{owner=\"{owner}\",stage=\"{stage}\"}} {failures}\n\
             rustdx_stage_completed_seconds_total{{owner=\"{owner}\",stage=\"{stage}\"}} {seconds}\n"
        ));
    }
    Ok(text)
}
