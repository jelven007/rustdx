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

fn forecast_tree(root: &Path, relative: &Path, files: &mut Vec<Evidence>) -> Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            forecast_tree(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(Evidence {
                path: path
                    .to_str()
                    .ok_or_else(|| invalid("non UTF8 backup path"))?
                    .into(),
                sha256: "0".repeat(64),
                bytes: metadata.len(),
            });
            if files.len() > storage::MAX_TREE_ENTRIES as usize {
                return Err(invalid("backup file count exceeded"));
            }
        } else {
            return Err(invalid("backup refuses symlinks and special files"));
        }
    }
    Ok(())
}

/// Holds the same controller lock as collection/publication. SQLite's online API
/// includes committed WAL content; copying only ledger.sqlite would be incorrect.
pub fn backup(root: &Path, destination: &Path, max_bytes: u64) -> Result<()> {
    let destination = storage::isolated_destination(root, destination)?;
    let destination = destination.as_path();
    let _lock = storage::RootLock::acquire(root)?;
    let ledger = Ledger::open_existing_readonly(&root.join("ledger.sqlite"))?;
    if ledger
        .unfinished()?
        .iter()
        .any(|a| a.state == "running" || a.pid.is_some_and(crate::supervisor::process_group_alive))
    {
        return Err(invalid(
            "recover active/ambiguous workers before taking a consistent backup",
        ));
    }
    // Predict the inventory using fixed-length SHA values. The same timestamp
    // and exact file sizes are reused after copying, so its size is reserved.
    let pages: u64 = ledger
        .connection
        .query_row("PRAGMA page_count", [], |r| crate::ledger::unsigned(r, 0))?;
    let page_size: u64 = ledger
        .connection
        .query_row("PRAGMA page_size", [], |r| crate::ledger::unsigned(r, 0))?;
    let db_bytes = pages
        .checked_mul(page_size)
        .ok_or_else(|| invalid("backup SQLite size overflow"))?;
    let mut forecast = Inventory {
        schema_version: 1,
        created_at: crate::now(),
        files: vec![Evidence {
            path: "ledger.sqlite".into(),
            sha256: "0".repeat(64),
            bytes: db_bytes,
        }],
    };
    for name in ["attempts", "releases", "catalogs", "database"] {
        if root.join(name).exists() {
            forecast_tree(root, Path::new(name), &mut forecast.files)?;
        }
    }
    forecast.files.sort_by(|a, b| a.path.cmp(&b.path));
    let inventory_bytes = serde_json::to_vec_pretty(&forecast)?.len() as u64;
    let copied_bytes = forecast
        .files
        .iter()
        .try_fold(0_u64, |n, f| n.checked_add(f.bytes))
        .ok_or_else(|| invalid("backup size overflow"))?;
    if inventory_bytes > MAX_JSON
        || copied_bytes
            .checked_add(inventory_bytes)
            .is_none_or(|n| n > max_bytes)
    {
        return Err(invalid(
            "backup total budget cannot cover SQLite, files and inventory",
        ));
    }
    fs::create_dir(destination)?;
    storage::private_directory(destination)?;
    let mut target = rusqlite::Connection::open(destination.join("ledger.sqlite"))?;
    {
        let backup = Backup::new(&ledger.connection, &mut target)?;
        loop {
            rustdx_complete::operation::check().map_err(crate::Error::from)?;
            let result = backup.step(128)?;
            if fs::metadata(destination.join("ledger.sqlite"))?.len() > max_bytes - inventory_bytes
            {
                return Err(invalid("SQLite backup byte budget exceeded"));
            }
            match result {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => return Err(invalid("unsupported SQLite backup result")),
            }
        }
    }
    // The completed online snapshot has no independent WAL. Store it in DELETE
    // mode so later readonly validation needs no unbudgeted shm/WAL files.
    target.execute_batch("PRAGMA journal_mode=DELETE;")?;
    drop(target);
    fs::File::open(destination.join("ledger.sqlite"))?.sync_all()?;
    let db = storage::evidence(destination, &destination.join("ledger.sqlite"), max_bytes)?;
    if db.bytes != db_bytes {
        return Err(invalid("SQLite backup differs from reserved page count"));
    }
    let mut remaining = max_bytes - inventory_bytes - db.bytes;
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
        created_at: forecast.created_at,
        files,
    };
    let bytes = serde_json::to_vec_pretty(&inventory)?;
    if bytes.len() as u64 > inventory_bytes
        || inventory
            .files
            .iter()
            .map(|f| (&f.path, f.bytes))
            .ne(forecast.files.iter().map(|f| (&f.path, f.bytes)))
        || storage::tree_bytes(destination)?.saturating_add(bytes.len() as u64) > max_bytes
    {
        return Err(invalid(
            "backup inventory or source differs from reserved budget",
        ));
    }
    storage::write_new(&destination.join("backup.json"), &bytes)?;
    storage::sync_directory(destination)?;
    Ok(())
}

pub fn restore(source: &Path, destination: &Path, max_bytes: u64) -> Result<()> {
    let destination = storage::isolated_destination(source, destination)?;
    let destination = destination.as_path();
    let inventory: Inventory = serde_json::from_slice(&storage::read_limited(
        &source.join("backup.json"),
        MAX_JSON,
    )?)?;
    if inventory.schema_version != 1 || inventory.files.len() > 200_000 {
        return Err(invalid("unsupported backup inventory"));
    }
    let mut paths = std::collections::BTreeSet::new();
    let original = Ledger::open_backup_readonly(&storage::safe_join(source, "ledger.sqlite")?)?
        .original_roots()?;
    let marker = serde_json::to_vec_pretty(
        &serde_json::json!({"schema_version":1,"validated_at":crate::now(),
            "backup_created_at":inventory.created_at,"original_roots":original}),
    )?;
    let mut remaining = max_bytes
        .checked_sub(marker.len() as u64)
        .ok_or_else(|| invalid("restore budget cannot cover completion marker"))?;
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
    let _restore_lock = storage::RootLock::acquire(destination)?;
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
    if storage::tree_bytes(destination)?.saturating_add(marker.len() as u64) > max_bytes {
        return Err(invalid(
            "restored validation files exceed total byte budget",
        ));
    }
    storage::write_new(&destination.join("restore-validated.json"), &marker)?;
    Ok(())
}

pub fn validate_restore(root: &Path) -> Result<()> {
    let ledger = Ledger::open_backup_readonly(&root.join("ledger.sqlite"))?;
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
    let attempts: u64 = ledger
        .connection
        .query_row("SELECT count(*) FROM attempts", [], |r| {
            crate::ledger::unsigned(r, 0)
        })?;
    text.push_str("# TYPE rustdx_observation_missing_records gauge\n");
    for owner in ["worker", "controller"] {
        let recorded: u64 = ledger.connection.query_row(
            "SELECT coalesce(sum(total),0) FROM observation_counters
             WHERE owner=?1 AND field IN ('records_complete','records_failed','records_unavailable')",
            [owner], |r| crate::ledger::unsigned(r, 0),
        )?;
        let missing = attempts.checked_sub(recorded).ok_or_else(|| {
            invalid("observation projection exceeds attempt history; rebuild metrics")
        })?;
        text.push_str(&format!(
            "rustdx_observation_missing_records{{owner=\"{owner}\"}} {missing}\n"
        ));
    }
    Ok(text)
}

fn observation_metrics(ledger: &Ledger) -> Result<String> {
    Ok(crate::observations::Totals::load(&ledger.connection)?.render())
}

#[cfg(test)]
fn legacy_observation_metrics(ledger: &Ledger) -> Result<String> {
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

#[cfg(test)]
mod observation_tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn projection_matches_legacy_scan_and_rebuild_for_large_history() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ledger.sqlite");
        let ledger = Ledger::open(&path).unwrap();
        // Exercise migration from the old raw-only schema, preserving its payload bytes.
        ledger
            .connection
            .execute_batch(
                "DROP TABLE observation_counters; DROP TABLE observation_stages;
             DROP INDEX attempts_by_job; DROP INDEX jobs_by_created;
             DROP TRIGGER immutable_observations_update;
             DROP TRIGGER immutable_observations_delete; PRAGMA user_version=3;",
            )
            .unwrap();
        let tx = ledger.connection.unchecked_transaction().unwrap();
        let record = json!({"schema_version":1,"status":"complete",
            "network":{"requests":9,"request_failures":2,"empty_responses":1,
                "acquisition_timeouts":3,"queue_wait_micros":120},
            "observations":{"stages":{"parse":{"calls":2,"failures":1,"completed_span_ms":2000.0}}}});
        let text = serde_json::to_string(&record).unwrap();
        for n in 0..20_000 {
            let id = format!("j{n}");
            tx.execute(
                "INSERT INTO jobs VALUES(?1,'d','{}','failed',?2,'')",
                rusqlite::params![id, crate::now()],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO attempts(id,job,state,directory,started) VALUES(?1,?1,'failed',?1,?2)",
                rusqlite::params![id, crate::now()],
            )
            .unwrap();
            let payload = match n % 3 {
                0 => text.clone(),
                1 => text.replace("\"complete\"", "\"failed\""),
                _ => "{\"schema_version\":1,\"status\":\"unavailable\"}".into(),
            };
            tx.execute(
                "INSERT INTO attempt_observations VALUES(?1,'worker',?2,?3)",
                rusqlite::params![id, payload, crate::now()],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        drop(ledger);
        let mut ledger = Ledger::open(&path).unwrap();
        let start = Instant::now();
        let legacy = legacy_observation_metrics(&ledger).unwrap();
        let legacy_elapsed = start.elapsed();
        let start = Instant::now();
        let projected = observation_metrics(&ledger).unwrap();
        let projection_elapsed = start.elapsed();
        assert_eq!(projected, legacy);
        ledger.rebuild_observation_metrics().unwrap();
        assert_eq!(observation_metrics(&ledger).unwrap(), legacy);
        // Duplicate raw insertion rolls back both raw and projected changes.
        assert!(ledger.record_observations("j0", "worker", &record).is_err());
        assert_eq!(observation_metrics(&ledger).unwrap(), legacy);
        assert!(
            ledger
                .connection
                .execute("DELETE FROM attempt_observations WHERE attempt='j0'", [])
                .is_err()
        );
        let mut query = ledger
            .connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT j.id,j.dataset,j.state,
                (SELECT count(*) FROM attempts a WHERE a.job=j.id),j.detail
             FROM jobs j ORDER BY j.created DESC LIMIT 1000",
            )
            .unwrap();
        let plans = query
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(plans.iter().any(|p| p.contains("jobs_by_created")));
        assert!(plans.iter().any(|p| p.contains("attempts_by_job")));
        assert!(!plans.iter().any(|p| p.contains("TEMP B-TREE")));
        println!(
            "history=20000 legacy_observation_micros={} projection_micros={} plans={plans:?}",
            legacy_elapsed.as_micros(),
            projection_elapsed.as_micros()
        );
    }

    #[test]
    fn projection_overflow_and_invalid_records_leave_raw_history_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let mut ledger = Ledger::open(&temp.path().join("ledger.sqlite")).unwrap();
        ledger.register("j", "d", "{}").unwrap();
        ledger.begin_attempt("j", "a", Path::new("a")).unwrap();
        let record = json!({"schema_version":1,"status":"complete",
            "network":{"requests":1,"request_failures":0,"empty_responses":0,
                "acquisition_timeouts":0,"queue_wait_micros":0}});
        ledger
            .connection
            .execute(
                "INSERT INTO observation_counters VALUES('worker','requests',?1)",
                [i64::MAX],
            )
            .unwrap();
        assert!(ledger.record_observations("a", "worker", &record).is_err());
        assert!(
            ledger
                .record_observations("a", "worker", &json!({"status":"complete"}))
                .is_err()
        );
        let count: i64 = ledger
            .connection
            .query_row("SELECT count(*) FROM attempt_observations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = ledger
            .connection
            .query_row("SELECT count(*) FROM observation_counters", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn invalid_historical_record_rolls_back_schema_and_projection_migration() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ledger.sqlite");
        let mut ledger = Ledger::open(&path).unwrap();
        ledger.register("j", "d", "{}").unwrap();
        ledger.begin_attempt("j", "a", Path::new("a")).unwrap();
        ledger.connection.execute_batch(
            "DROP TABLE observation_counters; DROP TABLE observation_stages;
             DROP INDEX attempts_by_job; DROP INDEX jobs_by_created;
             DROP TRIGGER immutable_observations_update; DROP TRIGGER immutable_observations_delete;
             INSERT INTO attempt_observations VALUES('a','worker','{}','old'); PRAGMA user_version=3;",
        ).unwrap();
        drop(ledger);
        assert!(Ledger::open(&path).is_err());
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            db.query_row("SELECT payload FROM attempt_observations", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "{}"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='observation_counters'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
    }
}
