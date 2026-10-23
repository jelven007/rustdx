//! Strict, credential-free collection configuration and versioned point-in-time inputs.
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{Result, invalid, storage};

pub const CONTRACT: &str = "a-share/day/raw/cny/shares/v1";
pub const MAX_JSON: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub root: PathBuf,
    pub cli: PathBuf,
    pub dataset_id: String,
    pub contract_version: String,
    pub policy_revision: u64,
    pub source_revision: u64,
    pub source_as_of: NaiveDate,
    pub planned_date: NaiveDate,
    pub from: NaiveDate,
    pub through: NaiveDate,
    /// Explicit local .day directories; unattended download fallbacks are excluded.
    pub day_directories: Vec<PathBuf>,
    pub calendar: PathBuf,
    pub calendar_source_sha256: String,
    pub security_history: PathBuf,
    pub universe: PathBuf,
    /// Local copies of announcements, named by their lowercase SHA-256.
    pub facts_directory: PathBuf,
    pub timeout_seconds: u64,
    pub publish_reserve_seconds: u64,
    pub cancel_grace_seconds: u64,
    pub workers: usize,
    pub max_source_bytes: u64,
    pub max_workspace_bytes: u64,
    pub max_rows: u64,
    pub min_free_bytes: u64,
    /// Publisher-only endpoints and environment variable names; never secret values.
    #[serde(default)]
    pub database_targets: Vec<DatabaseTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DatabaseTarget {
    Clickhouse {
        id: String,
        url: String,
        table: String,
        user_env: String,
        password_env: String,
    },
    Mongodb {
        id: String,
        database: String,
        collection: String,
        uri_env: String,
        mongosh: PathBuf,
    },
}

impl DatabaseTarget {
    pub fn id(&self) -> &str {
        match self {
            Self::Clickhouse { id, .. } | Self::Mongodb { id, .. } => id,
        }
    }

    fn validate(&self) -> Result<()> {
        identifier(self.id())?;
        let env = |name: &str| -> Result<()> {
            if name.is_empty()
                || name.len() > 96
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                || !name.starts_with("RUSTDX_")
            {
                return Err(invalid(
                    "credential environment names must start with RUSTDX_",
                ));
            }
            Ok(())
        };
        let sql_name = |name: &str| -> Result<()> {
            if name.is_empty()
                || name.len() > 64
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(invalid(
                    "database identifiers require ASCII letters, digits or _",
                ));
            }
            Ok(())
        };
        match self {
            Self::Clickhouse {
                url,
                table,
                user_env,
                password_env,
                ..
            } => {
                let uri: ureq::http::Uri =
                    url.parse().map_err(|_| invalid("invalid target URL"))?;
                if !matches!(uri.scheme_str(), Some("https" | "http"))
                    || uri.authority().is_none_or(|a| a.as_str().contains('@'))
                    || uri.path_and_query().is_some_and(|p| p.as_str() != "/")
                {
                    return Err(invalid("target URL must be a credential-free HTTP origin"));
                }
                let names: Vec<_> = table.split('.').collect();
                if names.len() != 2 {
                    return Err(invalid("table must be database.table"));
                }
                for name in names {
                    sql_name(name)?;
                }
                env(user_env)?;
                env(password_env)?;
            }
            Self::Mongodb {
                database,
                collection,
                uri_env,
                mongosh,
                ..
            } => {
                sql_name(database)?;
                sql_name(collection)?;
                env(uri_env)?;
                if !mongosh.is_absolute() {
                    return Err(invalid("mongosh must be an absolute executable path"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Universe {
    pub version: String,
    pub checked_through: NaiveDate,
    pub source_sha256: String,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub code: String,
    pub from: NaiveDate,
    pub through: NaiveDate,
}

pub fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(invalid(
            "identifier must contain 1–96 ASCII letters, digits, _ or -",
        ));
    }
    Ok(())
}

pub fn digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("expected lowercase SHA-256"));
    }
    Ok(())
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let config: Self = serde_json::from_slice(&storage::read_limited(path, MAX_JSON)?)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        identifier(&self.dataset_id)?;
        digest(&self.calendar_source_sha256)?;
        if self.schema_version != 1 || self.contract_version != CONTRACT {
            return Err(invalid("unsupported config schema or data contract"));
        }
        if self.from > self.through
            || (self.through - self.from).num_days() > 3660
            || self.source_as_of < self.through
            || self.planned_date < self.source_as_of
            || self.policy_revision == 0
            || self.source_revision == 0
            || self.source_revision > i64::MAX as u64
            || !(1..=64).contains(&self.workers)
            || self.timeout_seconds > 86400
            || self.publish_reserve_seconds == 0
            || self.timeout_seconds <= self.publish_reserve_seconds
            || self.cancel_grace_seconds > 60
            || self.max_source_bytes == 0
            || self.max_workspace_bytes < self.max_source_bytes
            || self.max_rows == 0
            || self.max_rows > i64::MAX as u64
            || self.day_directories.is_empty()
            || self.day_directories.len() > 64
        {
            return Err(invalid("invalid date, revision or resource budget"));
        }
        if self.database_targets.len() > 4 {
            return Err(invalid("at most four database targets"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for target in &self.database_targets {
            target.validate()?;
            if !ids.insert(target.id()) {
                return Err(invalid("duplicate target id"));
            }
        }
        for path in [
            &self.root,
            &self.cli,
            &self.calendar,
            &self.security_history,
            &self.universe,
            &self.facts_directory,
        ]
        .into_iter()
        .chain(&self.day_directories)
        {
            if !path.is_absolute() {
                return Err(invalid("all configuration paths must be absolute"));
            }
        }
        Ok(())
    }

    /// Excludes deployment paths and budgets: these do not define a new business job.
    pub fn job_id(&self) -> Result<String> {
        let identity = serde_json::json!({
            "dataset": self.dataset_id, "contract": self.contract_version,
            "policy": self.policy_revision, "revision": self.source_revision,
            "as_of": self.source_as_of, "planned": self.planned_date,
            "from": self.from, "through": self.through, "source": "tdx-local-day",
        });
        Ok(storage::sha256(&serde_json::to_vec(&identity)?))
    }
}

impl Universe {
    pub fn validate(&self, config: &Config) -> Result<()> {
        if self.version.trim().is_empty()
            || self.checked_through < config.through
            || self.checked_through > config.planned_date
            || self.members.is_empty()
            || self.members.len() > 100_000
        {
            return Err(invalid("universe version, coverage or size invalid"));
        }
        digest(&self.source_sha256)?;
        let mut last: Option<&str> = None;
        for member in &self.members {
            let symbol = member
                .code
                .parse::<rustdx_complete::data::Symbol>()
                .map_err(|e| invalid(e.to_string()))?;
            if !symbol.is_a_share()
                || member.from > member.through
                || member.through > self.checked_through
                || last.is_some_and(|code| code >= member.code.as_str())
            {
                return Err(invalid(
                    "universe must have sorted, unique A-share members with valid intervals",
                ));
            }
            last = Some(&member.code);
        }
        Ok(())
    }
}
