//! Insert-only versioned targets. A missing write acknowledgement is never retried.
use std::{
    cell::{Cell, RefCell},
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    time::Duration,
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Error, Result,
    config::{Config, DatabaseTarget, MAX_JSON},
    consumer::{CanonicalRow, read_partition},
    database_process::Process,
    invalid,
    ledger::Ledger,
    model::{Evidence, ReleaseManifest},
    storage,
};

pub(crate) mod capacity;
use capacity::{BATCH, Capacity, IO_LIMIT, RESPONSE_LIMIT};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Row {
    dataset_id: String,
    release_id: String,
    contract_version: String,
    #[serde(flatten)]
    bar: CanonicalRow,
}

/// Dataset and release form the equality prefix of both database indexes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct RowKey {
    contract_version: String,
    date: chrono::NaiveDate,
    market: String,
    code: String,
    instrument_class: String,
}

impl Row {
    fn key(&self) -> RowKey {
        RowKey {
            contract_version: self.contract_version.clone(),
            date: self.bar.date,
            market: self.bar.market.clone(),
            code: self.bar.code.clone(),
            instrument_class: self.bar.instrument_class.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub target: DatabaseTarget,
    pub proof: Evidence,
}

struct Session<'a> {
    root: &'a Path,
    audit: Audit<'a>,
    release: &'a str,
    target: &'a DatabaseTarget,
    mongo: RefCell<Option<Process>>,
    max_workspace_bytes: u64,
    usage: Rc<RefCell<storage::TreeUsage>>,
}

enum Audit<'a> {
    Publication(&'a Ledger),
    Consumer {
        directory: PathBuf,
        sequence: Rc<Cell<u64>>,
    },
}

/// Independent, bounded readback evidence. It never appends to the publication ledger.
pub(crate) struct QueryAudit {
    directory: PathBuf,
    sequence: Rc<Cell<u64>>,
    usage: Rc<RefCell<storage::TreeUsage>>,
    max_bytes: u64,
}

impl QueryAudit {
    pub(crate) fn new(root: &Path, max_bytes: u64) -> Result<Self> {
        let parent = root.join("consumer-audit");
        storage::private_directory(&parent)?;
        let directory = parent.join(format!(
            "q-{}-{}",
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .ok_or_else(|| invalid("timestamp overflow"))?,
            std::process::id()
        ));
        // Never share a namespace, even if two callers obtain the same timestamp.
        std::fs::create_dir(&directory)?;
        storage::private_directory(&directory)?;
        let bytes = serde_json::to_vec(&json!({
            "schema_version":1,"kind":"consumer_readback","max_bytes":max_bytes,
            "max_requests":IO_LIMIT,"started_at":crate::now()
        }))?;
        if bytes.len() as u64 + RESPONSE_LIMIT * 7 + 8192 > max_bytes {
            return Err(invalid(
                "consumer audit budget cannot cover a bounded response",
            ));
        }
        storage::write_new(&directory.join("session.json"), &bytes)?;
        Ok(Self {
            directory,
            sequence: Rc::new(Cell::new(0)),
            usage: Rc::new(RefCell::new(storage::TreeUsage {
                bytes: bytes.len() as u64,
                entries: 1,
            })),
            max_bytes,
        })
    }

    pub(crate) fn finish(&self) -> Result<()> {
        let actual = storage::tree_usage(&self.directory)?;
        let bytes = serde_json::to_vec(&json!({
            "schema_version":1,"readback_verified_at":crate::now(),
            "requests":self.sequence.get(),"evidence_bytes":actual.bytes
        }))?;
        if actual.bytes.saturating_add(bytes.len() as u64) > self.max_bytes
            || actual.entries + 1 > storage::MAX_TREE_ENTRIES
        {
            return Err(invalid("consumer audit budget exhausted at reconciliation"));
        }
        storage::write_new(&self.directory.join("verified.json"), &bytes)?;
        Ok(())
    }
}

fn check() -> Result<()> {
    rustdx_complete::operation::check().map_err(Error::from)
}

fn credential(name: &str) -> Result<String> {
    std::env::var(name)
        .map_err(|_| invalid(format!("missing publisher credential environment {name}")))
}

impl Session<'_> {
    fn preflight(&self) -> Result<()> {
        if let DatabaseTarget::Clickhouse { table, .. } = self.target {
            let (database, name) = table
                .split_once('.')
                .ok_or_else(|| invalid("invalid table"))?;
            let bytes = self.exchange(&json!({"sql":format!(
                "SELECT engine,sorting_key,create_table_query FROM system.tables WHERE database='{database}' AND name='{name}' FORMAT JSONEachRow")}),false)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            if value["engine"] != "MergeTree"
                || value["sorting_key"]
                    != "dataset_id, release_id, contract_version, date, market, code, instrument_class"
                || value["create_table_query"]
                    .as_str()
                    .is_none_or(|s| s.contains(" TTL "))
            {
                return Err(invalid(
                    "target must be provisioned as a full-key MergeTree without TTL",
                ));
            }
        } else if let DatabaseTarget::Mongodb {
            database,
            collection,
            ..
        } = self.target
        {
            self.exchange(
                &json!({
                    "operation":"preflight","database":database,"collection":collection
                }),
                false,
            )?;
        }
        Ok(())
    }

    fn finish(&self) -> Result<()> {
        if let Some(mut process) = self.mongo.borrow_mut().take() {
            process.finish()?;
        }
        // Reconcile a stationary evidence tree at the session boundary.
        let directory = match &self.audit {
            Audit::Publication(_) => self.root.join("database").join(self.release),
            Audit::Consumer { directory, .. } => directory.clone(),
        };
        let actual = storage::tree_usage(&directory)?;
        if actual.bytes > self.max_workspace_bytes {
            return Err(invalid(
                "database evidence budget exhausted at reconciliation",
            ));
        }
        *self.usage.borrow_mut() = actual;
        Ok(())
    }

    fn exchange(&self, payload: &Value, write: bool) -> Result<Vec<u8>> {
        check()?;
        let sequence = match &self.audit {
            Audit::Publication(ledger) => ledger.connection.query_row(
                "SELECT coalesce(max(sequence),0)+1 FROM target_io WHERE release=?1 AND target=?2",
                params![self.release, self.target.id()],
                |r| r.get::<_, i64>(0),
            )?,
            Audit::Consumer { sequence, .. } => {
                let next = sequence.get().saturating_add(1);
                if next > IO_LIMIT {
                    return Err(invalid("consumer session I/O record limit exceeded"));
                }
                sequence.set(next);
                next as i64
            }
        };
        if !(1..=IO_LIMIT as i64).contains(&sequence) {
            return Err(invalid("database I/O record limit exceeded"));
        }
        let directory = match &self.audit {
            Audit::Publication(_) => self.root.join("database").join(self.release),
            Audit::Consumer { directory, .. } => directory.join(self.release),
        }
        .join(self.target.id());
        storage::private_directory(&directory)?;
        let usage = *self.usage.borrow();
        if usage.bytes.saturating_add(RESPONSE_LIMIT * 7 + 4096) > self.max_workspace_bytes
            || usage.entries.saturating_add(3) > storage::MAX_TREE_ENTRIES
        {
            return Err(invalid("database evidence budget exhausted"));
        }
        let base = directory.join(format!(
            "{sequence}-{}-{}",
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .ok_or_else(|| invalid("timestamp overflow"))?,
            std::process::id()
        ));
        let namespace = match &self.audit {
            Audit::Publication(_) => self.release,
            Audit::Consumer { directory, .. } => directory
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| invalid("invalid audit namespace"))?,
        };
        let query_id = format!("{namespace}-{}-{sequence}", self.target.id());
        let request = json!({"query_id":query_id,"is_write":write,"payload":payload});
        let bytes = serde_json::to_vec(&request)?;
        if bytes.len() as u64 > RESPONSE_LIMIT {
            return Err(invalid("database request limit exceeded"));
        }
        let request_path = base.with_extension("request.json");
        storage::write_new(&request_path, &bytes)?;
        {
            let mut usage = self.usage.borrow_mut();
            usage.bytes = usage.bytes.saturating_add(bytes.len() as u64);
            usage.entries += 1;
        }
        let request_evidence = storage::evidence(self.root, &request_path, RESPONSE_LIMIT)?;
        // This durable intent precedes spawn/transmit. A crash cannot silently turn into a retry.
        if let Audit::Publication(ledger) = &self.audit {
            ledger.connection.execute(
                "INSERT INTO target_io VALUES(?1,?2,?3,?4,?5,NULL)",
                params![
                    self.release,
                    self.target.id(),
                    sequence,
                    write,
                    serde_json::to_string(&request_evidence)?
                ],
            )?;
        } else if write {
            return Err(invalid("consumer audit cannot transmit writes"));
        }
        let response_path = base.with_extension("response");
        let response = match self.target {
            DatabaseTarget::Clickhouse {
                url,
                user_env,
                password_env,
                ..
            } => {
                let sql = payload["sql"]
                    .as_str()
                    .ok_or_else(|| invalid("missing SQL"))?;
                let mut body = sql.as_bytes().to_vec();
                if let Some(data) = payload.get("rows") {
                    body.push(b'\n');
                    for row in data.as_array().ok_or_else(|| invalid("invalid rows"))? {
                        serde_json::to_writer(&mut body, row)?;
                        body.push(b'\n');
                    }
                }
                let mut operation = rustdx_cli::http::HttpOperation::new(Duration::from_secs(60))?;
                let mut response = rustdx_cli::http::post(url)
                    .query("query_id", &query_id)
                    .query("wait_end_of_query", "1")
                    .query("async_insert", "0")
                    .header("X-ClickHouse-User", credential(user_env)?)
                    .header("X-ClickHouse-Key", credential(password_env)?)
                    .config()
                    .http_status_as_error(false)
                    .max_redirects(0)
                    .timeout_global(Some(operation.timeout()))
                    .build()
                    .send(body.as_slice())
                    .map_err(|_| {
                        invalid("ClickHouse transport failed; durable request retained")
                    })?;
                let status = response.status().as_u16();
                let mut bytes = Vec::new();
                rustdx_complete::operation::CheckedIo(response.body_mut().as_reader())
                    .take(RESPONSE_LIMIT + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() as u64 > RESPONSE_LIMIT {
                    return Err(invalid("database response limit exceeded"));
                }
                // Preserve complete bounded body and relevant response identity, even HTTP errors.
                let envelope = json!({"status":status,"query_id":query_id,
                    "server_query_id":response.headers().get("X-ClickHouse-Query-Id").and_then(|h|h.to_str().ok()),
                    "body":String::from_utf8(bytes.clone()).map_err(|_| invalid("non UTF8 database response"))?});
                storage::write_new(&response_path, &serde_json::to_vec(&envelope)?)?;
                if envelope["server_query_id"]
                    .as_str()
                    .is_some_and(|id| id != query_id)
                {
                    return Err(invalid("ClickHouse response query_id differs from request"));
                }
                if status != 200 || bytes.windows(14).any(|w| w == b"DB::Exception:") {
                    return Err(invalid(
                        "ClickHouse rejected request; response evidence retained",
                    ));
                }
                operation.complete(bytes.is_empty())?;
                bytes
            }
            DatabaseTarget::Mongodb {
                mongosh, uri_env, ..
            } => {
                let output = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&response_path)?;
                let mut process = self.mongo.borrow_mut();
                if process.is_none() {
                    let mut command = Command::new(mongosh);
                    command
                        .env_clear()
                        .env("PATH", "/usr/bin:/bin")
                        .env("RUSTDX_DB_URI", credential(uri_env)?)
                        .env("NODE_OPTIONS", "--max-old-space-size=128")
                        .args([
                            "--nodb",
                            "--quiet",
                            "--norc",
                            "--eval",
                            include_str!("database_mongo.js"),
                        ]);
                    *process = Some(Process::spawn(&mut command, RESPONSE_LIMIT)?);
                }
                let outcome = process
                    .as_mut()
                    .ok_or_else(|| invalid("missing MongoDB session"))?
                    .exchange(bytes, output);
                File::open(&response_path)?.sync_all()?;
                storage::sync_directory(&directory)?;
                let bytes = outcome?;
                let value: Value = serde_json::from_slice(&bytes)?;
                if value["ok"] != true || value["query_id"] != query_id {
                    return Err(invalid("MongoDB acknowledgement or query identity differs"));
                }
                bytes
            }
        };
        let evidence = storage::evidence(self.root, &response_path, RESPONSE_LIMIT * 6 + 4096)?;
        {
            let mut usage = self.usage.borrow_mut();
            usage.bytes = usage.bytes.saturating_add(evidence.bytes);
            usage.entries += 1;
        }
        if let Audit::Publication(ledger) = &self.audit {
            ledger.connection.execute(
                "UPDATE target_io SET response=?4 WHERE release=?1 AND target=?2 AND sequence=?3",
                params![
                    self.release,
                    self.target.id(),
                    sequence,
                    serde_json::to_string(&evidence)?
                ],
            )?;
        } else {
            let receipt = serde_json::to_vec(&json!({
                "schema_version":1,"release":self.release,"target":self.target.id(),
                "sequence":sequence,"request":request_evidence,"response":evidence
            }))?;
            storage::write_new(&base.with_extension("receipt.json"), &receipt)?;
            let mut usage = self.usage.borrow_mut();
            usage.bytes = usage.bytes.saturating_add(receipt.len() as u64);
            usage.entries += 1;
        }
        Ok(response)
    }

    fn insert(&self, rows: &[Row]) -> Result<()> {
        let payload = match self.target {
            DatabaseTarget::Clickhouse { table, .. } => json!({
                "sql":format!("INSERT INTO {table} FORMAT JSONEachRow"),"rows":rows}),
            DatabaseTarget::Mongodb {
                database,
                collection,
                ..
            } => json!({
                "operation":"insert","database":database,"collection":collection,"rows":rows}),
        };
        let bytes = self.exchange(&payload, true)?;
        if matches!(self.target, DatabaseTarget::Mongodb { .. })
            && serde_json::from_slice::<Value>(&bytes)?["inserted"].as_u64()
                != Some(rows.len() as u64)
        {
            return Err(invalid(
                "MongoDB inserted count differs from durable request",
            ));
        }
        Ok(())
    }

    fn fetch(
        &self,
        manifest: &ReleaseManifest,
        after: Option<&RowKey>,
        date: Option<chrono::NaiveDate>,
        count: usize,
    ) -> Result<Vec<Row>> {
        let payload = match self.target {
            DatabaseTarget::Clickhouse { table, .. } => {
                let day = date.map_or_else(String::new, |d| format!(" AND date='{d}'"));
                let cursor = after.map_or_else(String::new, |key| format!(
                    " AND (contract_version,date,market,code,instrument_class) > ('{}',toDate('{}'),'{}','{}','{}')",
                    key.contract_version,key.date,key.market,key.code,key.instrument_class));
                json!({"sql":format!(
                    "SELECT dataset_id,release_id,contract_version,date,market,code,instrument_class,open,high,low,close,amount_cny,volume_shares FROM {table} WHERE dataset_id='{}' AND release_id='{}'{day}{cursor} ORDER BY contract_version,date,market,code,instrument_class LIMIT {count} FORMAT JSONEachRow",
                    manifest.dataset_id,manifest.release_id)})
            }
            DatabaseTarget::Mongodb {
                database,
                collection,
                ..
            } => json!({
                "operation":"fetch","database":database,"collection":collection,
                "dataset":manifest.dataset_id,"release":manifest.release_id,
                "after":after,"date":date,"count":count}),
        };
        let bytes = self.exchange(&payload, false)?;
        match self.target {
            DatabaseTarget::Clickhouse { .. } => {
                let text =
                    std::str::from_utf8(&bytes).map_err(|_| invalid("invalid row encoding"))?;
                text.lines()
                    .map(|line| serde_json::from_str(line).map_err(Into::into))
                    .collect()
            }
            DatabaseTarget::Mongodb { .. } => Ok(serde_json::from_value(
                serde_json::from_slice::<Value>(&bytes)?["rows"].clone(),
            )?),
        }
    }

    fn count(&self, manifest: &ReleaseManifest) -> Result<u64> {
        let payload = match self.target {
            DatabaseTarget::Clickhouse { table, .. } => json!({"sql":format!(
                "SELECT count() FROM {table} WHERE dataset_id='{}' AND release_id='{}' FORMAT TabSeparated",
                manifest.dataset_id,manifest.release_id)}),
            DatabaseTarget::Mongodb {
                database,
                collection,
                ..
            } => json!({
                "operation":"count","database":database,"collection":collection,
                "dataset":manifest.dataset_id,"release":manifest.release_id}),
        };
        let bytes = self.exchange(&payload, false)?;
        match self.target {
            DatabaseTarget::Clickhouse { .. } => std::str::from_utf8(&bytes)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .ok_or_else(|| invalid("invalid release count")),
            DatabaseTarget::Mongodb { .. } => serde_json::from_slice::<Value>(&bytes)?["count"]
                .as_u64()
                .ok_or_else(|| invalid("invalid release count")),
        }
    }

    fn verify(
        &self,
        manifest: &ReleaseManifest,
        date: Option<chrono::NaiveDate>,
        mut visitor: impl FnMut(CanonicalRow) -> Result<()>,
    ) -> Result<u64> {
        let verified = self.verify_partition_rows(manifest, date, &mut visitor)?;
        self.verify_total(manifest)?;
        Ok(verified)
    }

    fn verify_partition_rows(
        &self,
        manifest: &ReleaseManifest,
        date: Option<chrono::NaiveDate>,
        mut visitor: impl FnMut(CanonicalRow) -> Result<()>,
    ) -> Result<u64> {
        let mut batch = Vec::with_capacity(BATCH);
        let mut after = None;
        let mut verified = 0;
        let mut verify = |rows: &mut Vec<Row>| -> Result<()> {
            if self.fetch(manifest, after.as_ref(), date, rows.len())? != *rows {
                return Err(invalid(
                    "database full key or exact field readback differs from sealed release",
                ));
            }
            after = rows.last().map(Row::key);
            verified += rows.len() as u64;
            for row in rows.drain(..) {
                visitor(row.bar)?;
            }
            Ok(())
        };
        for partition in manifest.partitions.values() {
            if date.is_some_and(|date| date != partition.date) {
                continue;
            }
            read_partition(self.root, partition, |bar| {
                batch.push(Row {
                    dataset_id: manifest.dataset_id.clone(),
                    release_id: manifest.release_id.clone(),
                    contract_version: manifest.contract_version.clone(),
                    bar,
                });
                if batch.len() == BATCH {
                    verify(&mut batch)?;
                }
                Ok(())
            })?;
        }
        if !batch.is_empty() {
            verify(&mut batch)?;
        }
        if !self.fetch(manifest, after.as_ref(), date, 1)?.is_empty() {
            return Err(invalid(
                "release has missing, duplicate or extra database keys",
            ));
        }
        Ok(verified)
    }

    fn verify_total(&self, manifest: &ReleaseManifest) -> Result<()> {
        let total = manifest
            .partitions
            .values()
            .try_fold(0_u64, |n, p| n.checked_add(p.rows))
            .ok_or_else(|| invalid("release row total overflow"))?;
        if self.count(manifest)? != total {
            return Err(invalid(
                "release has missing, duplicate or extra database keys",
            ));
        }
        Ok(())
    }
}

pub fn prepare(
    root: &Path,
    ledger: &Ledger,
    manifest: &ReleaseManifest,
    config: &Config,
    recovery: bool,
) -> Result<()> {
    let state: String = ledger.connection.query_row(
        "SELECT state FROM releases WHERE id=?1",
        [&manifest.release_id],
        |r| r.get(0),
    )?;
    if state != "sealed" {
        return Err(invalid(
            "only an uncommitted sealed release can prepare targets",
        ));
    }
    // Forecast every target before recording unknown state or transmitting any
    // request. Validate the sealed bytes once; never estimate from max_rows.
    let directory = root.join("database").join(&manifest.release_id);
    let existing = if directory.exists() {
        storage::tree_usage(&directory)?
    } else {
        storage::TreeUsage::default()
    };
    let usage = Rc::new(RefCell::new(existing));
    if !config.database_targets.is_empty() {
        let mut rows = 0_u64;
        let mut row_bytes = 0_u64;
        for partition in manifest.partitions.values() {
            read_partition(root, partition, |bar| {
                let row = Row {
                    dataset_id: manifest.dataset_id.clone(),
                    release_id: manifest.release_id.clone(),
                    contract_version: manifest.contract_version.clone(),
                    bar,
                };
                rows = rows
                    .checked_add(1)
                    .ok_or_else(|| invalid("row forecast overflow"))?;
                row_bytes = row_bytes
                    .checked_add(serde_json::to_vec(&row)?.len() as u64 + 1)
                    .ok_or_else(|| invalid("row byte forecast overflow"))?;
                Ok(())
            })?;
        }
        let capacity = Capacity::estimate(rows, row_bytes)?;
        let mut forecast = existing;
        for target in &config.database_targets {
            let used: u64 = ledger.connection.query_row(
                "SELECT coalesce(max(sequence),0) FROM target_io WHERE release=?1 AND target=?2",
                params![manifest.release_id, target.id()],
                |r| crate::ledger::unsigned(r, 0),
            )?;
            capacity.check(used, forecast, config.max_workspace_bytes)?;
            forecast.bytes = forecast.bytes.saturating_add(capacity.evidence_bytes);
            forecast.entries = forecast.entries.saturating_add(capacity.evidence_entries);
        }
        if storage::free_bytes(root)?
            < capacity
                .evidence_bytes
                .saturating_mul(config.database_targets.len() as u64)
                .saturating_add(config.min_free_bytes)
        {
            return Err(invalid("database capacity: insufficient disk reserve"));
        }
        storage::private_directory(&directory)?;
        let path = directory.join(format!(
            "capacity-{}.json",
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .ok_or_else(|| invalid("timestamp overflow"))?
        ));
        let bytes = serde_json::to_vec(&json!({
            "schema_version":1,"release":manifest.release_id,"per_target":capacity,
            "targets":config.database_targets.len(),"recovery":recovery,
            "reserve":"publication and one recovery; consumer readbacks use independent query audits",
            "checked_at":crate::now(),
        }))?;
        storage::write_new(&path, &bytes)?;
        let mut usage = usage.borrow_mut();
        usage.bytes = usage.bytes.saturating_add(bytes.len() as u64);
        usage.entries += 1;
    }
    for target in &config.database_targets {
        let spec = serde_json::to_string(target)?;
        let prior: Option<(String, String)> = ledger
            .connection
            .query_row(
                "SELECT spec,state FROM targets WHERE release=?1 AND id=?2",
                params![manifest.release_id, target.id()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let session = Session {
            root,
            audit: Audit::Publication(ledger),
            release: &manifest.release_id,
            target,
            mongo: RefCell::new(None),
            max_workspace_bytes: config.max_workspace_bytes,
            usage: Rc::clone(&usage),
        };
        match prior {
            Some((old, _)) if old != spec => return Err(invalid("frozen database target changed")),
            Some((_, state)) if state == "unknown" => {
                return Err(invalid(
                    "unknown database write: inspect durable requests, establish server quiescence, quarantine and create a new release",
                ));
            }
            None if recovery => {
                return Err(invalid(
                    "target not yet written: quarantine and register a new revision; recovery never starts writes",
                ));
            }
            None => {
                ledger.connection.execute(
                    "INSERT INTO targets VALUES(?1,?2,?3,'unknown',NULL)",
                    params![manifest.release_id, target.id(), spec],
                )?;
                session.preflight()?;
                // New immutable namespace must start empty; no upsert/FINAL hides duplicates.
                if session.count(manifest)? != 0 {
                    return Err(invalid("new release namespace is not empty"));
                }
                let mut batch = Vec::with_capacity(BATCH);
                for partition in manifest.partitions.values() {
                    read_partition(root, partition, |bar| {
                        batch.push(Row {
                            dataset_id: manifest.dataset_id.clone(),
                            release_id: manifest.release_id.clone(),
                            contract_version: manifest.contract_version.clone(),
                            bar,
                        });
                        if batch.len() == BATCH {
                            session.insert(&batch)?;
                            batch.clear();
                        }
                        Ok(())
                    })?;
                }
                if !batch.is_empty() {
                    session.insert(&batch)?;
                }
                ledger.connection.execute(
                    "UPDATE targets SET state='acknowledged' WHERE release=?1 AND id=?2",
                    params![manifest.release_id, target.id()],
                )?;
            }
            Some((_, state)) if state == "verified" => {
                // Recheck content but retain the immutable receipt. An orphan
                // catalog written before a crash must remain byte-compatible.
                receipts(root, ledger, &manifest.release_id)?;
                session.preflight()?;
                session.verify(manifest, None, |_| Ok(()))?;
                session.finish()?;
                continue;
            }
            Some((_, state)) if state == "acknowledged" => {
                session.preflight()?;
            }
            Some(_) => return Err(invalid("invalid database target state")),
        }
        let rows = session.verify(manifest, None, |_| Ok(()))?;
        session.finish()?;
        let missing: i64 = ledger.connection.query_row(
            "SELECT count(*) FROM target_io WHERE release=?1 AND target=?2 AND is_write=1 AND response IS NULL",
            params![manifest.release_id,target.id()],|r|r.get(0))?;
        if missing != 0 {
            return Err(invalid("write acknowledgements remain unknown"));
        }
        let proof_path = root
            .join("database")
            .join(&manifest.release_id)
            .join(target.id())
            .join(format!(
                "verified-{}.json",
                chrono::Utc::now()
                    .timestamp_nanos_opt()
                    .ok_or_else(|| invalid("timestamp overflow"))?
            ));
        let mut query = ledger.connection.prepare(
            "SELECT request,response FROM target_io WHERE release=?1 AND target=?2 ORDER BY sequence")?;
        let io = query
            .query_map(params![manifest.release_id, target.id()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let proof_bytes = serde_json::to_vec(&json!({
            "schema_version":1,"target":target,"release":manifest.release_id,"rows":rows,
            "verified_at":crate::now(),"io":io}))?;
        if proof_bytes.len() as u64 > MAX_JSON {
            return Err(invalid(
                "database verification proof exceeds JSON byte limit",
            ));
        }
        storage::write_new(&proof_path, &proof_bytes)?;
        {
            let mut usage = usage.borrow_mut();
            usage.bytes = usage.bytes.saturating_add(proof_bytes.len() as u64);
            usage.entries += 1;
        }
        let proof = storage::evidence(root, &proof_path, MAX_JSON)?;
        ledger.connection.execute(
            "UPDATE targets SET state='verified',proof=?3 WHERE release=?1 AND id=?2",
            params![
                manifest.release_id,
                target.id(),
                serde_json::to_string(&proof)?
            ],
        )?;
    }
    Ok(())
}

pub fn receipts(root: &Path, ledger: &Ledger, release: &str) -> Result<Vec<Receipt>> {
    let mut query = ledger
        .connection
        .prepare("SELECT spec,state,proof FROM targets WHERE release=?1 ORDER BY id")?;
    let mut rows = query.query([release])?;
    let mut receipts = Vec::new();
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? != "verified" {
            return Err(invalid("target not verified"));
        }
        let proof: Evidence = serde_json::from_str(&row.get::<_, String>(2)?)?;
        storage::verify(root, &proof)?;
        let value: Value = serde_json::from_slice(&storage::read_limited(
            &storage::safe_join(root, &proof.path)?,
            MAX_JSON,
        )?)?;
        if value["schema_version"] != 1
            || value["release"] != release
            || value["target"] != serde_json::from_str::<Value>(&row.get::<_, String>(0)?)?
        {
            return Err(invalid("database proof identity differs from ledger"));
        }
        for io in value["io"]
            .as_array()
            .ok_or_else(|| invalid("invalid database proof"))?
        {
            for item in io.as_array().ok_or_else(|| invalid("invalid I/O proof"))? {
                if let Some(text) = item.as_str() {
                    storage::verify(root, &serde_json::from_str::<Evidence>(text)?)?;
                }
            }
        }
        receipts.push(Receipt {
            target: serde_json::from_str(&row.get::<_, String>(0)?)?,
            proof,
        });
    }
    Ok(receipts)
}

/// Consumer fixes a release and compares every returned field with its sealed files.
/// The visitor must stage results until this function returns successfully.
pub fn read_verified(
    root: &Path,
    ledger: &Ledger,
    manifest: &ReleaseManifest,
    target_id: &str,
    visitor: impl FnMut(CanonicalRow) -> Result<()>,
) -> Result<u64> {
    let receipt = receipts(root, ledger, &manifest.release_id)?
        .into_iter()
        .find(|r| r.target.id() == target_id)
        .ok_or_else(|| invalid("target absent from released version"))?;
    let audit = QueryAudit::new(root, readback_budget(root, manifest)?)?;
    let session = ReadSession::new(root, manifest, &receipt, &audit);
    let rows = session
        .session
        .verify_partition_rows(manifest, None, visitor)?;
    session.finish()?;
    audit.finish()?;
    Ok(rows)
}

pub fn read_verified_partition(
    root: &Path,
    ledger: &Ledger,
    manifest: &ReleaseManifest,
    date: chrono::NaiveDate,
    target_id: &str,
    visitor: impl FnMut(CanonicalRow) -> Result<()>,
) -> Result<u64> {
    if !manifest.partitions.contains_key(&date) {
        return Err(invalid("date absent from release"));
    }
    let receipt = receipts(root, ledger, &manifest.release_id)?
        .into_iter()
        .find(|r| r.target.id() == target_id)
        .ok_or_else(|| invalid("target absent from released version"))?;
    let audit = QueryAudit::new(root, readback_budget(root, manifest)?)?;
    let session = ReadSession::new(root, manifest, &receipt, &audit);
    let rows = session.read_partition(date, visitor)?;
    session.finish()?;
    audit.finish()?;
    Ok(rows)
}

pub(crate) fn readback_budget(root: &Path, manifest: &ReleaseManifest) -> Result<u64> {
    Ok(Config::load(
        &root
            .join("releases")
            .join(&manifest.release_id)
            .join("job.json"),
    )?
    .max_workspace_bytes)
}

pub(crate) struct ReadSession<'a> {
    session: Session<'a>,
    manifest: &'a ReleaseManifest,
}

impl<'a> ReadSession<'a> {
    pub(crate) fn new(
        root: &'a Path,
        manifest: &'a ReleaseManifest,
        receipt: &'a Receipt,
        audit: &QueryAudit,
    ) -> Self {
        Self {
            session: Session {
                root,
                audit: Audit::Consumer {
                    directory: audit.directory.clone(),
                    sequence: Rc::clone(&audit.sequence),
                },
                release: &manifest.release_id,
                target: &receipt.target,
                mongo: RefCell::new(None),
                max_workspace_bytes: audit.max_bytes,
                usage: Rc::clone(&audit.usage),
            },
            manifest,
        }
    }

    pub(crate) fn read_partition(
        &self,
        date: chrono::NaiveDate,
        visitor: impl FnMut(CanonicalRow) -> Result<()>,
    ) -> Result<u64> {
        if !self.manifest.partitions.contains_key(&date) {
            return Err(invalid("date absent from release"));
        }
        self.session
            .verify_partition_rows(self.manifest, Some(date), visitor)
    }

    pub(crate) fn finish(&self) -> Result<()> {
        self.session.verify_total(self.manifest)?;
        // QueryAudit reconciles the shared tree once, after every release has finished.
        if let Some(mut process) = self.session.mongo.borrow_mut().take() {
            process.finish()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_key_readback_crosses_batch_and_partition_boundaries() {
        use crate::model::Partition;
        use std::{
            collections::BTreeMap,
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        let release = "r";
        let directory = root.join("releases/r");
        storage::private_directory(&directory).unwrap();
        let mut partitions = BTreeMap::new();
        let mut all = Vec::new();
        for day in ["2026-07-06", "2026-07-07"] {
            let date = day.parse().unwrap();
            let path = directory.join(format!("{day}.csv"));
            let mut writer = csv::Writer::from_path(&path).unwrap();
            for index in 0..600 {
                let row: Row = serde_json::from_value(json!({
                    "dataset_id":"d","release_id":release,"contract_version":crate::config::CONTRACT,
                    "date":day,"market":"sh","code":format!("{}", 600000 + index),
                    "instrument_class":"a_share","open":10.0,"high":12.0,"low":9.0,
                    "close":10.0,"amount_cny":12345.5,"volume_shares":101.0,
                })).unwrap();
                writer.serialize(&row.bar).unwrap();
                all.push(row);
            }
            writer.flush().unwrap();
            partitions.insert(
                date,
                Partition {
                    date,
                    release_id: release.into(),
                    file: storage::evidence(&directory, &path, MAX_JSON).unwrap(),
                    rows: 600,
                    exemptions: 0,
                    source_revision: 1,
                    source_as_of: date,
                },
            );
        }
        let manifest = ReleaseManifest {
            schema_version: 1,
            dataset_id: "d".into(),
            release_id: release.into(),
            attempt_id: "a".into(),
            cli_run_id: "c".into(),
            collector_executed: true,
            contract_version: crate::config::CONTRACT.into(),
            calendar_version: "test".into(),
            history_version: "test".into(),
            universe_version: "test".into(),
            from: "2026-07-06".parse().unwrap(),
            through: "2026-07-07".parse().unwrap(),
            source_revision: 1,
            source_as_of: "2026-07-07".parse().unwrap(),
            created_at: crate::now(),
            files: Vec::new(),
            partitions,
        };
        let mut ledger = Ledger::open(&root.join("ledger.sqlite")).unwrap();
        ledger.register("j", "d", "{}").unwrap();
        ledger.begin_attempt("j", "a", &directory).unwrap();
        ledger
            .seal(
                release,
                "a",
                "d",
                &Evidence {
                    path: "test".into(),
                    sha256: "0".repeat(64),
                    bytes: 0,
                },
                "c",
            )
            .unwrap();
        ledger
            .connection
            .execute(
                "INSERT INTO targets VALUES('r','t','{}','acknowledged',NULL)",
                [],
            )
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let target = DatabaseTarget::Clickhouse {
            id: "t".into(),
            url: format!("http://{}", listener.local_addr().unwrap()),
            table: "test.bars".into(),
            user_env: "PATH".into(),
            password_env: "PATH".into(),
        };
        let mut expected_csv = csv::Writer::from_writer(Vec::new());
        for row in &all {
            expected_csv.serialize(&row.bar).unwrap();
        }
        let expected_hash = storage::sha256(&expected_csv.into_inner().unwrap());
        // Independently expect cursor positions, rather than parsing our SQL
        // to generate its own oracle. Day two starts at its own first key.
        let server = std::thread::spawn(move || {
            let cases = [
                (0, 512, None, None),
                (512, 512, Some(511), None),
                (1024, 176, Some(1023), None),
                (1200, 0, Some(1199), None),
                (1200, 0, None, None),
                (600, 512, None, Some("2026-07-07")),
                (1112, 88, Some(1111), Some("2026-07-07")),
                (1200, 0, Some(1199), Some("2026-07-07")),
                (1200, 0, None, None),
                // Baseline: each day finishes its own session and release count.
                (0, 512, None, Some("2026-07-06")),
                (512, 88, Some(511), Some("2026-07-06")),
                (600, 0, Some(599), Some("2026-07-06")),
                (1200, 0, None, None),
                (600, 512, None, Some("2026-07-07")),
                (1112, 88, Some(1111), Some("2026-07-07")),
                (1200, 0, Some(1199), Some("2026-07-07")),
                (1200, 0, None, None),
                // A single consumer session: two partition tails, one release count.
                (0, 512, None, Some("2026-07-06")),
                (512, 88, Some(511), Some("2026-07-06")),
                (600, 0, Some(599), Some("2026-07-06")),
                (600, 512, None, Some("2026-07-07")),
                (1112, 88, Some(1111), Some("2026-07-07")),
                (1200, 0, Some(1199), Some("2026-07-07")),
                (1200, 0, None, None),
                // An extra tail key must fail, even when every expected row matched.
                (0, 512, None, Some("2026-07-06")),
                (512, 88, Some(511), Some("2026-07-06")),
                (599, 1, Some(599), Some("2026-07-06")),
            ];
            for (start, n, after, date) in cases {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let sql = String::from_utf8(body).unwrap();
                assert!(!sql.contains("OFFSET"));
                let response = if sql.starts_with("SELECT count()") {
                    assert!(after.is_none());
                    "1200\n".to_owned()
                } else {
                    assert!(
                        sql.contains("ORDER BY contract_version,date,market,code,instrument_class")
                    );
                    if let Some(index) = after {
                        let key = all[index].key();
                        assert!(sql.contains(&format!(
                            "> ('{}',toDate('{}'),'{}','{}','{}')",
                            key.contract_version,
                            key.date,
                            key.market,
                            key.code,
                            key.instrument_class
                        )));
                    } else {
                        assert!(!sql.contains(") > ("));
                    }
                    if let Some(date) = date {
                        assert!(sql.contains(&format!(" AND date='{date}'")));
                    }
                    all[start..start + n]
                        .iter()
                        .map(|r| serde_json::to_string(r).unwrap() + "\n")
                        .collect()
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).unwrap();
            }
        });
        let session = Session {
            root,
            audit: Audit::Publication(&ledger),
            release,
            target: &target,
            mongo: RefCell::new(None),
            max_workspace_bytes: 256 * 1024 * 1024,
            usage: Rc::new(RefCell::new(storage::TreeUsage::default())),
        };
        assert_eq!(session.verify(&manifest, None, |_| Ok(())).unwrap(), 1200);
        session.finish().unwrap();
        assert_eq!(
            session
                .verify(&manifest, Some(manifest.through), |_| Ok(()))
                .unwrap(),
            600
        );
        session.finish().unwrap();
        ledger
            .connection
            .execute("UPDATE target_io SET sequence=50000 WHERE sequence=9", [])
            .unwrap();
        let receipt = Receipt {
            target: target.clone(),
            proof: evidence_for_test(),
        };
        let baseline_start = std::time::Instant::now();
        let mut baseline_bytes = 0;
        for date in [manifest.from, manifest.through] {
            let audit = QueryAudit::new(root, 256 * 1024 * 1024).unwrap();
            let reader = ReadSession::new(root, &manifest, &receipt, &audit);
            reader.read_partition(date, |_| Ok(())).unwrap();
            reader.finish().unwrap();
            audit.finish().unwrap();
            assert_eq!(audit.sequence.get(), 4);
            baseline_bytes += storage::tree_bytes(&audit.directory).unwrap();
        }
        let baseline_elapsed = baseline_start.elapsed();
        let reuse_start = std::time::Instant::now();
        let audit = QueryAudit::new(root, 256 * 1024 * 1024).unwrap();
        let reader = ReadSession::new(root, &manifest, &receipt, &audit);
        let mut exported = Vec::new();
        for date in [manifest.from, manifest.through] {
            assert_eq!(
                reader
                    .read_partition(date, |bar| {
                        exported.push(bar);
                        Ok(())
                    })
                    .unwrap(),
                600
            );
        }
        reader.finish().unwrap();
        audit.finish().unwrap();
        let reuse_elapsed = reuse_start.elapsed();
        assert_eq!(exported.len(), 1200);
        let mut actual_csv = csv::Writer::from_writer(Vec::new());
        for row in &exported {
            actual_csv.serialize(row).unwrap();
        }
        assert_eq!(
            storage::sha256(&actual_csv.into_inner().unwrap()),
            expected_hash
        );
        assert_eq!(audit.sequence.get(), 7);
        assert!(audit.directory.join("verified.json").exists());
        println!(
            "readback_rows=1200 dates=2 baseline_requests=8 reuse_requests=7 \
            baseline_bytes={baseline_bytes} reuse_bytes={} baseline_micros={} reuse_micros={} csv_sha256={expected_hash}",
            storage::tree_bytes(&audit.directory).unwrap(),
            baseline_elapsed.as_micros(),
            reuse_elapsed.as_micros()
        );
        let failed_audit = QueryAudit::new(root, 256 * 1024 * 1024).unwrap();
        let reader = ReadSession::new(root, &manifest, &receipt, &failed_audit);
        assert!(reader.read_partition(manifest.from, |_| Ok(())).is_err());
        assert!(!failed_audit.directory.join("verified.json").exists());
        server.join().unwrap();
        assert_eq!(
            ledger
                .connection
                .query_row(
                    "SELECT count(*) FROM target_io WHERE response IS NOT NULL",
                    [],
                    |r| crate::ledger::unsigned(r, 0)
                )
                .unwrap(),
            9
        );
        let exhausted = QueryAudit::new(root, 256 * 1024 * 1024).unwrap();
        exhausted.sequence.set(IO_LIMIT);
        let reader = ReadSession::new(root, &manifest, &receipt, &exhausted);
        assert!(reader.read_partition(manifest.from, |_| Ok(())).is_err());
        assert!(QueryAudit::new(root, RESPONSE_LIMIT).is_err());
    }

    fn evidence_for_test() -> Evidence {
        Evidence {
            path: "unused".into(),
            bytes: 0,
            sha256: "0".repeat(64),
        }
    }

    #[test]
    fn full_contract_row_roundtrips_and_requires_every_field() {
        let value = json!({"dataset_id":"d","release_id":"r","contract_version":crate::config::CONTRACT,
            "date":"2026-07-06","market":"sh","code":"600000","instrument_class":"a_share",
            "open":10.0,"high":12.0,"low":9.0,"close":10.0,"amount_cny":12345.5,"volume_shares":101.0});
        let row: Row = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(row).unwrap(), value);
        let mut missing = value;
        missing.as_object_mut().unwrap().remove("volume_shares");
        assert!(serde_json::from_value::<Row>(missing).is_err());
    }
}
