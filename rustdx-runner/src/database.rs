//! Insert-only versioned targets. A missing write acknowledgement is never retried.
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Error, Result,
    config::{Config, DatabaseTarget, MAX_JSON},
    consumer::{CanonicalRow, read_partition},
    invalid,
    ledger::Ledger,
    model::{Evidence, ReleaseManifest},
    storage,
};

const BATCH: usize = 512;
const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Row {
    dataset_id: String,
    release_id: String,
    contract_version: String,
    #[serde(flatten)]
    bar: CanonicalRow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub target: DatabaseTarget,
    pub proof: Evidence,
}

struct Session<'a> {
    root: &'a Path,
    ledger: &'a Ledger,
    release: &'a str,
    target: &'a DatabaseTarget,
}

fn check() -> Result<()> {
    rustdx_complete::operation::check().map_err(|_| Error::Cancelled)
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
        }
        Ok(())
    }

    fn exchange(&self, payload: &Value, write: bool) -> Result<Vec<u8>> {
        check()?;
        let config = Config::load(
            &self
                .root
                .join("releases")
                .join(self.release)
                .join("job.json"),
        )?;
        let sequence: i64 = self.ledger.connection.query_row(
            "SELECT coalesce(max(sequence),0)+1 FROM target_io WHERE release=?1 AND target=?2",
            params![self.release, self.target.id()],
            |r| r.get(0),
        )?;
        if !(1..=50_000).contains(&sequence) {
            return Err(invalid("database I/O record limit exceeded"));
        }
        let directory = self
            .root
            .join("database")
            .join(self.release)
            .join(self.target.id());
        storage::private_directory(&directory)?;
        if storage::tree_bytes(&self.root.join("database").join(self.release))?
            .saturating_add(RESPONSE_LIMIT * 6 + 4096)
            > config.max_workspace_bytes
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
        let query_id = format!("{}-{}-{sequence}", self.release, self.target.id());
        let request = json!({"query_id":query_id,"is_write":write,"payload":payload});
        let bytes = serde_json::to_vec(&request)?;
        if bytes.len() as u64 > RESPONSE_LIMIT {
            return Err(invalid("database request limit exceeded"));
        }
        let request_path = base.with_extension("request.json");
        storage::write_new(&request_path, &bytes)?;
        let request_evidence = storage::evidence(self.root, &request_path, RESPONSE_LIMIT)?;
        // This durable intent precedes spawn/transmit. A crash cannot silently turn into a retry.
        self.ledger.connection.execute(
            "INSERT INTO target_io VALUES(?1,?2,?3,?4,?5,NULL)",
            params![
                self.release,
                self.target.id(),
                sequence,
                write,
                serde_json::to_string(&request_evidence)?
            ],
        )?;
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
                let input = base.with_extension("input.json");
                storage::write_new(&input, &serde_json::to_vec(payload)?)?;
                let output = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&response_path)?;
                let mut command = Command::new(mongosh);
                command
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin")
                    .env("RUSTDX_DB_URI", credential(uri_env)?)
                    .env("RUSTDX_DB_REQUEST", input)
                    .env("NODE_OPTIONS", "--max-old-space-size=128")
                    .args([
                        "--nodb",
                        "--quiet",
                        "--norc",
                        "--eval",
                        include_str!("database_mongo.js"),
                    ])
                    .stdin(Stdio::null())
                    .stdout(output)
                    .stderr(Stdio::null());
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    command.process_group(0);
                }
                let timeout = rustdx_complete::operation::timeout(Duration::from_secs(60))?;
                let _permit = rustdx_complete::tcp::acquire_network(timeout)?;
                let mut worker = crate::supervisor::Worker {
                    child: command.spawn()?,
                };
                let start = Instant::now();
                let outcome = loop {
                    if let Err(e) = check() {
                        break Err(e);
                    }
                    if start.elapsed() >= timeout
                        || fs::metadata(&response_path)?.len() > RESPONSE_LIMIT
                    {
                        break Err(invalid(
                            "MongoDB process exceeded deadline or response limit",
                        ));
                    }
                    match worker.child.try_wait()? {
                        Some(status) if status.success() => break Ok(()),
                        Some(_) => {
                            break Err(invalid("MongoDB request failed; durable intent retained"));
                        }
                        None => std::thread::sleep(Duration::from_millis(20)),
                    }
                };
                drop(worker);
                File::open(&response_path)?.sync_all()?;
                storage::sync_directory(&directory)?;
                outcome?;
                let bytes = storage::read_limited(&response_path, RESPONSE_LIMIT)?;
                let value: Value = serde_json::from_slice(&bytes)?;
                if value["ok"] != true {
                    return Err(invalid("MongoDB acknowledgement missing"));
                }
                bytes
            }
        };
        let evidence = storage::evidence(self.root, &response_path, RESPONSE_LIMIT * 6 + 4096)?;
        self.ledger.connection.execute(
            "UPDATE target_io SET response=?4 WHERE release=?1 AND target=?2 AND sequence=?3",
            params![
                self.release,
                self.target.id(),
                sequence,
                serde_json::to_string(&evidence)?
            ],
        )?;
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
        self.exchange(&payload, true)?;
        Ok(())
    }

    fn fetch(&self, manifest: &ReleaseManifest, offset: u64, count: usize) -> Result<Vec<Row>> {
        let payload = match self.target {
            DatabaseTarget::Clickhouse { table, .. } => json!({"sql":format!(
                "SELECT dataset_id,release_id,contract_version,date,market,code,instrument_class,open,high,low,close,amount_cny,volume_shares FROM {table} WHERE dataset_id='{}' AND release_id='{}' ORDER BY date,market,code,contract_version,instrument_class LIMIT {count} OFFSET {offset} FORMAT JSONEachRow",
                manifest.dataset_id,manifest.release_id)}),
            DatabaseTarget::Mongodb {
                database,
                collection,
                ..
            } => json!({
                "operation":"fetch","database":database,"collection":collection,
                "dataset":manifest.dataset_id,"release":manifest.release_id,"offset":offset,"count":count}),
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
        let mut batch = Vec::with_capacity(BATCH);
        let initial = date.map_or(0, |date| {
            manifest.partitions.range(..date).map(|(_, p)| p.rows).sum()
        });
        let mut offset = initial;
        let mut verify = |rows: &mut Vec<Row>| -> Result<()> {
            if self.fetch(manifest, offset, rows.len())? != *rows {
                return Err(invalid(
                    "database full key or exact field readback differs from sealed release",
                ));
            }
            offset += rows.len() as u64;
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
        let total = manifest.partitions.values().map(|p| p.rows).sum::<u64>();
        if self.count(manifest)? != total || !self.fetch(manifest, total, 1)?.is_empty() {
            return Err(invalid(
                "release has missing, duplicate or extra database keys",
            ));
        }
        Ok(offset - initial)
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
            ledger,
            release: &manifest.release_id,
            target,
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
                continue;
            }
            Some((_, state)) if state == "acknowledged" => {
                session.preflight()?;
            }
            Some(_) => return Err(invalid("invalid database target state")),
        }
        let rows = session.verify(manifest, None, |_| Ok(()))?;
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
        storage::write_new(
            &proof_path,
            &serde_json::to_vec(&json!({
            "schema_version":1,"target":target,"release":manifest.release_id,"rows":rows,
            "verified_at":crate::now(),"io":io}))?,
        )?;
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
    Session {
        root,
        ledger,
        release: &manifest.release_id,
        target: &receipt.target,
    }
    .verify(manifest, None, visitor)
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
    Session {
        root,
        ledger,
        release: &manifest.release_id,
        target: &receipt.target,
    }
    .verify(manifest, Some(date), visitor)
}

#[cfg(test)]
mod tests {
    use super::*;
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
