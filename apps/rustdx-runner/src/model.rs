use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Relative to the immutable release directory.
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Partition {
    pub date: NaiveDate,
    pub release_id: String,
    pub file: Evidence,
    pub rows: u64,
    pub exemptions: u64,
    pub source_revision: u64,
    pub source_as_of: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub dataset_id: String,
    pub release_id: String,
    pub attempt_id: String,
    pub cli_run_id: String,
    #[serde(default = "collector_default")]
    pub collector_executed: bool,
    pub contract_version: String,
    pub calendar_version: String,
    pub history_version: String,
    pub universe_version: String,
    pub from: NaiveDate,
    pub through: NaiveDate,
    pub source_revision: u64,
    pub source_as_of: NaiveDate,
    pub created_at: String,
    pub files: Vec<Evidence>,
    pub partitions: BTreeMap<NaiveDate, Partition>,
}

fn collector_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    pub dataset_id: String,
    pub version: u64,
    pub previous_version: u64,
    pub contract_version: String,
    pub created_at: String,
    pub partitions: BTreeMap<NaiveDate, Partition>,
    #[serde(default)]
    pub database_releases: BTreeMap<String, Vec<crate::database::Receipt>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pointer {
    pub version: u64,
    pub manifest: Evidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobStatus {
    pub job_id: String,
    pub dataset_id: String,
    pub state: String,
    pub attempts: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveryAttempt {
    pub attempt_id: String,
    pub job_id: String,
    pub state: String,
    pub directory: String,
    pub pid: Option<u32>,
}
