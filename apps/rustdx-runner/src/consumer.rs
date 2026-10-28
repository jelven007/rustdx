//! A read session pins the catalog once. Export visibility is atomic and only
//! occurs after hashing the very bytes decoded, checking keys and counting rows.
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    Error, Result, invalid,
    ledger::Ledger,
    model::{Catalog, Partition, ReleaseManifest},
    pipeline, storage,
};

pub const COLUMNS: [&str; 10] = [
    "date",
    "market",
    "code",
    "instrument_class",
    "open",
    "high",
    "low",
    "close",
    "amount_cny",
    "volume_shares",
];

struct BoundedOutput {
    file: File,
    remaining: u64,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{config::CONTRACT, database::Receipt, model::Evidence};
    use serde_json::json;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
        time::Duration,
    };

    fn pinned(base: &Path) -> PinnedDataset {
        let root = base.join("live");
        storage::private_directory(&root).unwrap();
        let mock = base.join("mongosh");
        fs::write(&mock, include_bytes!("../tests/mock_mongosh.py")).unwrap();
        fs::set_permissions(&mock, fs::Permissions::from_mode(0o700)).unwrap();
        let mut catalog = Catalog {
            schema_version: 1,
            dataset_id: "fixture".into(),
            version: 9,
            previous_version: 8,
            contract_version: CONTRACT.into(),
            created_at: crate::now(),
            partitions: BTreeMap::new(),
            database_releases: BTreeMap::new(),
        };
        let mut manifests = BTreeMap::new();
        let mut oracle = Vec::new();
        let first: NaiveDate = "2026-07-06".parse().unwrap();
        for index in 0..9 {
            let release = format!("r{index}");
            let directory = root.join("releases").join(&release);
            storage::private_directory(&directory).unwrap();
            let mut partitions = BTreeMap::new();
            for offset in 0..11 {
                let date = first + chrono::Days::new(offset);
                let row = json!({"date":date,"market":"sh","code":"600000","instrument_class":"a_share",
                    "open":10.0,"high":12.0,"low":9.0,"close":10.0 + index as f64 / 10.0,
                    "amount_cny":12345.5,"volume_shares":101.0});
                let mut writer = csv::Writer::from_writer(Vec::new());
                writer
                    .serialize(serde_json::from_value::<CanonicalRow>(row.clone()).unwrap())
                    .unwrap();
                let path = directory.join(format!("{date}.csv"));
                fs::write(&path, writer.into_inner().unwrap()).unwrap();
                let partition = Partition {
                    date,
                    release_id: release.clone(),
                    file: storage::evidence(&directory, &path, 1024).unwrap(),
                    rows: 1,
                    exemptions: 0,
                    source_revision: index + 1,
                    source_as_of: first + chrono::Days::new(10),
                };
                let chosen = if (1..=8).contains(&offset) { offset } else { 0 };
                if index == chosen {
                    catalog.partitions.insert(date, partition.clone());
                }
                partitions.insert(date, partition);
                let mut row = row;
                row["dataset_id"] = json!("fixture");
                row["release_id"] = json!(release);
                row["contract_version"] = json!(CONTRACT);
                oracle.push(row);
            }
            fs::write(directory.join("job.json"), serde_json::to_vec(&json!({
                "schema_version":1,"root":root,"cli":mock,"dataset_id":"fixture","contract_version":CONTRACT,
                "policy_revision":1,"source_revision":index + 1,"source_as_of":"2026-07-16",
                "planned_date":"2026-07-16","from":"2026-07-06","through":"2026-07-16",
                "day_directories":[base],"calendar":base.join("calendar"),"calendar_source_sha256":"a".repeat(64),
                "security_history":base.join("history"),"universe":base.join("universe"),"facts_directory":base,
                "timeout_seconds":60,"publish_reserve_seconds":10,"cancel_grace_seconds":1,
                "workers":1,"max_source_bytes":1048576,"max_workspace_bytes":268435456,"max_rows":1000,"min_free_bytes":0
            })).unwrap()).unwrap();
            manifests.insert(
                release.clone(),
                ReleaseManifest {
                    schema_version: 1,
                    dataset_id: "fixture".into(),
                    release_id: release.clone(),
                    attempt_id: release.clone(),
                    cli_run_id: "fixture".into(),
                    collector_executed: true,
                    contract_version: CONTRACT.into(),
                    calendar_version: "fixture".into(),
                    history_version: "fixture".into(),
                    universe_version: "fixture".into(),
                    from: first,
                    through: first + chrono::Days::new(10),
                    source_revision: index + 1,
                    source_as_of: first + chrono::Days::new(10),
                    created_at: crate::now(),
                    files: Vec::new(),
                    partitions,
                },
            );
            catalog.database_releases.insert(
                release,
                vec![Receipt {
                    target: crate::config::DatabaseTarget::Mongodb {
                        id: "mongo".into(),
                        database: "fixture".into(),
                        collection: "bars".into(),
                        uri_env: "RUSTDX_CONSUMER_TEST_URI".into(),
                        mongosh: mock.clone(),
                    },
                    proof: Evidence {
                        path: "unused".into(),
                        sha256: "a".repeat(64),
                        bytes: 0,
                    },
                }],
            );
        }
        fs::write(
            base.join("oracle.json"),
            serde_json::to_vec(&json!({"rows":oracle,
                "process_root":std::env::var_os("RUSTDX_CONSUMER_PROCESS_ROOT")
                    .map(PathBuf::from).unwrap_or_else(|| base.to_path_buf())}))
            .unwrap(),
        )
        .unwrap();
        PinnedDataset {
            root,
            catalog,
            manifests,
        }
    }

    #[test]
    fn interleaved_mongo_releases_release_permits_and_preserve_atomic_output() {
        if let Some(base) = std::env::var_os("RUSTDX_CONSUMER_CHILD") {
            let base = PathBuf::from(base);
            let limit: usize = std::env::var("RUSTDX_CONSUMER_LIMIT")
                .unwrap()
                .parse()
                .unwrap();
            let permits = std::env::var_os("RUSTDX_CONSUMER_PERMITS")
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("permits"));
            rustdx_complete::tcp::configure_network(rustdx_complete::tcp::NetworkPolicy {
                max_connections: limit,
                shared: Some(rustdx_complete::tcp::SharedNetworkPolicy {
                    directory: permits,
                    max_connections: limit,
                }),
                ..Default::default()
            })
            .unwrap();
            let pinned = pinned(&base);
            let operation =
                rustdx_complete::operation::OperationContext::new(Duration::from_secs(15), None)
                    .unwrap();
            let _scope = operation.enter();
            let output = base.join(format!("output-{}.csv", std::process::id()));
            let from = "2026-07-06".parse().unwrap();
            let through = "2026-07-16".parse().unwrap();
            assert_eq!(
                pinned
                    .export_database(from, through, &output, 100, 10000, "mongo")
                    .unwrap(),
                11
            );
            let original = fs::read(&output).unwrap();
            for flag in ["extra_count", "bad_query_id"] {
                let mut oracle: serde_json::Value =
                    serde_json::from_slice(&fs::read(base.join("oracle.json")).unwrap()).unwrap();
                oracle[flag] = json!(true);
                fs::write(
                    base.join("oracle.json"),
                    serde_json::to_vec(&oracle).unwrap(),
                )
                .unwrap();
                assert!(
                    pinned
                        .export_database(from, through, &output, 100, 10000, "mongo")
                        .is_err()
                );
                assert_eq!(fs::read(&output).unwrap(), original);
                oracle[flag] = json!(false);
                fs::write(
                    base.join("oracle.json"),
                    serde_json::to_vec(&oracle).unwrap(),
                )
                .unwrap();
            }
            return;
        }
        for limit in [1, 8] {
            let temp = tempfile::tempdir().unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "consumer::tests::interleaved_mongo_releases_release_permits_and_preserve_atomic_output"])
                .env("RUSTDX_CONSUMER_CHILD", temp.path()).env("RUSTDX_CONSUMER_TEST_URI", temp.path())
                .env("RUSTDX_CONSUMER_LIMIT", limit.to_string()).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let state: serde_json::Value =
                serde_json::from_slice(&fs::read(temp.path().join("processes.json")).unwrap())
                    .unwrap();
            assert_eq!(state["peak"], 1);
        }
        let temp = tempfile::tempdir().unwrap();
        let mut children = Vec::new();
        for name in ["consumer-a", "consumer-b"] {
            let base = temp.path().join(name);
            fs::create_dir(&base).unwrap();
            children.push(Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "consumer::tests::interleaved_mongo_releases_release_permits_and_preserve_atomic_output"])
                .env("RUSTDX_CONSUMER_CHILD", &base).env("RUSTDX_CONSUMER_TEST_URI", &base)
                .env("RUSTDX_CONSUMER_LIMIT", "1")
                .env("RUSTDX_CONSUMER_PERMITS", temp.path().join("shared-permits"))
                .env("RUSTDX_CONSUMER_PROCESS_ROOT", temp.path())
                .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap());
        }
        for child in children {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(temp.path().join("processes.json")).unwrap()).unwrap();
        assert_eq!(state["peak"], 1);
    }
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        rustdx_complete::operation::check().map_err(std::io::Error::other)?;
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other(
                "consumer output byte budget exceeded",
            ));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CanonicalRow {
    pub date: NaiveDate,
    pub market: String,
    pub code: String,
    pub instrument_class: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub amount_cny: f64,
    pub volume_shares: f64,
}

impl CanonicalRow {
    pub fn validate(&self) -> Result<()> {
        let symbol = format!("{}{}", self.market, self.code)
            .parse::<rustdx_complete::data::Symbol>()
            .map_err(|e| invalid(e.to_string()))?;
        if !symbol.is_a_share()
            || self.instrument_class != "a_share"
            || self.volume_shares.fract() != 0.0
        {
            return Err(invalid("canonical identity or share volume invalid"));
        }
        rustdx_complete::data::validate_ohlcv(
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume_shares,
            self.amount_cny,
        )
        .map_err(|e| invalid(e.to_string()))?;
        Ok(())
    }
}

struct HashReader {
    file: File,
    hash: Sha256,
    bytes: u64,
    limit: u64,
}

impl Read for HashReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        rustdx_complete::operation::check().map_err(std::io::Error::other)?;
        let n = self.file.read(buffer)?;
        self.bytes = self.bytes.saturating_add(n as u64);
        if self.bytes > self.limit {
            return Err(std::io::Error::other("partition byte limit exceeded"));
        }
        self.hash.update(&buffer[..n]);
        Ok(n)
    }
}

pub fn read_partition(
    root: &Path,
    partition: &Partition,
    mut visitor: impl FnMut(CanonicalRow) -> Result<()>,
) -> Result<()> {
    crate::config::identifier(&partition.release_id)?;
    let directory = root.join("releases").join(&partition.release_id);
    let file = File::open(storage::safe_join(&directory, &partition.file.path)?)?;
    let hashing = HashReader {
        file,
        hash: Sha256::new(),
        bytes: 0,
        limit: partition.file.bytes,
    };
    let mut reader = csv::Reader::from_reader(hashing);
    let mut count = 0_u64;
    let mut previous: Option<(String, String)> = None;
    for row in reader.deserialize::<CanonicalRow>() {
        let row = row?;
        row.validate()?;
        let key = (row.market.clone(), row.code.clone());
        if row.date != partition.date || previous.as_ref().is_some_and(|prev| *prev >= key) {
            return Err(invalid(
                "partition has wrong date, duplicate or unordered logical key",
            ));
        }
        previous = Some(key);
        count += 1;
        if count > partition.rows {
            return Err(invalid("partition has extra logical keys"));
        }
        visitor(row)?;
    }
    let hashing = reader.into_inner();
    if count != partition.rows
        || hashing.bytes != partition.file.bytes
        || format!("{:x}", hashing.hash.finalize()) != partition.file.sha256
    {
        return Err(invalid(
            "partition bytes or row count differ from sealed manifest",
        ));
    }
    Ok(())
}

pub struct PinnedDataset {
    root: PathBuf,
    catalog: Catalog,
    manifests: BTreeMap<String, ReleaseManifest>,
}

impl PinnedDataset {
    pub fn open(root: &Path, dataset: &str, version: Option<u64>) -> Result<Self> {
        let ledger = Ledger::open_existing_readonly(&root.join("ledger.sqlite"))?;
        let catalog = pipeline::fixed_catalog(root, &ledger, dataset, version)?
            .ok_or_else(|| invalid("dataset/catalog has not been published"))?;
        let mut manifests: BTreeMap<String, ReleaseManifest> = BTreeMap::new();
        let mut checked_targets = std::collections::BTreeSet::new();
        for partition in catalog.partitions.values() {
            if !manifests.contains_key(&partition.release_id) {
                manifests.insert(
                    partition.release_id.clone(),
                    pipeline::release_manifest(
                        root,
                        &ledger.release_evidence(&partition.release_id)?,
                    )?,
                );
            }
            let manifest = manifests
                .get(&partition.release_id)
                .ok_or_else(|| invalid("missing release"))?;
            if manifest.dataset_id != dataset
                || serde_json::to_vec(
                    manifest
                        .partitions
                        .get(&partition.date)
                        .ok_or_else(|| invalid("missing release partition"))?,
                )? != serde_json::to_vec(partition)?
            {
                return Err(invalid(
                    "catalog partition does not match registered release",
                ));
            }
            if checked_targets.insert(partition.release_id.clone()) {
                let receipts = crate::database::receipts(root, &ledger, &partition.release_id)?;
                if serde_json::to_vec(&receipts)?
                    != serde_json::to_vec(
                        catalog
                            .database_releases
                            .get(&partition.release_id)
                            .unwrap_or(&Vec::new()),
                    )?
                {
                    return Err(invalid("database receipts differ from pinned catalog"));
                }
            }
        }
        Ok(Self {
            root: root.into(),
            catalog,
            manifests,
        })
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn export(
        &self,
        from: NaiveDate,
        through: NaiveDate,
        output: &Path,
        max_rows: u64,
        max_bytes: u64,
    ) -> Result<u64> {
        self.export_from(from, through, output, max_rows, max_bytes, None)
    }

    pub fn export_database(
        &self,
        from: NaiveDate,
        through: NaiveDate,
        output: &Path,
        max_rows: u64,
        max_bytes: u64,
        target: &str,
    ) -> Result<u64> {
        crate::config::identifier(target)?;
        self.export_from(from, through, output, max_rows, max_bytes, Some(target))
    }

    #[allow(clippy::too_many_arguments)]
    fn export_from(
        &self,
        from: NaiveDate,
        through: NaiveDate,
        output: &Path,
        max_rows: u64,
        max_bytes: u64,
        target: Option<&str>,
    ) -> Result<u64> {
        let first = self
            .catalog
            .partitions
            .first_key_value()
            .ok_or_else(|| invalid("empty catalog"))?
            .0;
        let last = self
            .catalog
            .partitions
            .last_key_value()
            .ok_or_else(|| invalid("empty catalog"))?
            .0;
        if from > through || from < *first || through > *last {
            return Err(invalid(
                "requested date range is outside the fixed complete catalog",
            ));
        }
        let parent = output
            .parent()
            .ok_or_else(|| invalid("missing output parent"))?
            .canonicalize()?;
        if parent.starts_with(self.root.canonicalize()?) {
            return Err(invalid(
                "consumer output must be outside the controller root",
            ));
        }
        let (atomic, file) = rustdx_complete::atomic_file::AtomicFile::create(output)?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(BoundedOutput {
                file,
                remaining: max_bytes,
            });
        writer.write_record(COLUMNS)?;
        let mut rows = 0_u64;
        // One live reader owns at most one network permit, even for A/B/A catalogs.
        let mut session: Option<(&str, crate::database::ReadSession<'_>)> = None;
        let audit = if let Some(target) = target {
            let releases: std::collections::BTreeSet<_> = self
                .catalog
                .partitions
                .range(from..=through)
                .map(|(_, p)| p.release_id.as_str())
                .collect();
            let mut budget = u64::MAX;
            for release in &releases {
                let manifest = self
                    .manifests
                    .get(*release)
                    .ok_or_else(|| invalid("missing pinned release"))?;
                // Fail before any database requests if a selected release lacks this target.
                self.catalog
                    .database_releases
                    .get(*release)
                    .and_then(|r| r.iter().find(|r| r.target.id() == target))
                    .ok_or_else(|| invalid("target absent from released version"))?;
                budget = budget.min(crate::database::readback_budget(&self.root, manifest)?);
            }
            let audit = crate::database::QueryAudit::new(&self.root, budget)?;
            Some(audit)
        } else {
            None
        };
        for partition in self
            .catalog
            .partitions
            .range(from..=through)
            .map(|(_, p)| p)
        {
            let mut visit = |row| {
                rows += 1;
                if rows > max_rows {
                    return Err(invalid("consumer row budget exceeded"));
                }
                writer.serialize(row)?;
                Ok(())
            };
            if let Some(target) = target {
                if session
                    .as_ref()
                    .is_none_or(|(id, _)| *id != partition.release_id)
                {
                    if let Some((_, previous)) = session.take() {
                        previous.finish()?;
                        // Drop the process/permit before opening the next release.
                        drop(previous);
                    }
                    let release = partition.release_id.as_str();
                    let manifest = self
                        .manifests
                        .get(release)
                        .ok_or_else(|| invalid("missing pinned release"))?;
                    let receipt = self
                        .catalog
                        .database_releases
                        .get(release)
                        .and_then(|r| r.iter().find(|r| r.target.id() == target))
                        .ok_or_else(|| invalid("missing pinned receipt"))?;
                    session = Some((
                        release,
                        crate::database::ReadSession::new(
                            &self.root,
                            manifest,
                            receipt,
                            audit
                                .as_ref()
                                .ok_or_else(|| invalid("missing query audit"))?,
                        ),
                    ));
                }
                session
                    .as_ref()
                    .ok_or_else(|| invalid("missing release read session"))?
                    .1
                    .read_partition(partition.date, &mut visit)?;
            } else {
                read_partition(&self.root, partition, &mut visit)?;
            }
        }
        if let Some((_, session)) = session.take() {
            session.finish()?;
        }
        if let Some(audit) = audit {
            audit.finish()?;
        }
        writer.flush()?;
        drop(writer);
        rustdx_complete::operation::check().map_err(Error::from)?;
        atomic.commit()?;
        Ok(rows)
    }
}
