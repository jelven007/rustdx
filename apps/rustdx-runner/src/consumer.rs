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
        let mut sessions = BTreeMap::new();
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
            for release in releases {
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
                sessions.insert(
                    release,
                    crate::database::ReadSession::new(&self.root, manifest, receipt, &audit),
                );
            }
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
            if target.is_some() {
                sessions
                    .get(partition.release_id.as_str())
                    .ok_or_else(|| invalid("missing release read session"))?
                    .read_partition(partition.date, &mut visit)?;
            } else {
                read_partition(&self.root, partition, &mut visit)?;
            }
        }
        for session in sessions.values() {
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
