//! Rebuildable metric projections. Raw observations remain the audit source.
use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::{Result, invalid, ledger::unsigned};

#[derive(Default)]
pub(crate) struct Totals {
    counters: BTreeMap<(String, String), u64>,
    stages: BTreeMap<(String, String), (u64, u64, f64)>,
}

fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| invalid("observation metric exceeds SQLite integer range"))
}

impl Totals {
    pub(crate) fn contribution(owner: &str, record: &Value) -> Result<Self> {
        if !matches!(owner, "worker" | "controller") || record["schema_version"] != 1 {
            return Err(invalid("invalid observation owner or schema"));
        }
        let status = record["status"]
            .as_str()
            .filter(|s| matches!(*s, "complete" | "failed" | "unavailable"))
            .ok_or_else(|| invalid("invalid observation status"))?;
        let mut result = Self::default();
        result
            .counters
            .insert((owner.into(), format!("records_{status}")), 1);
        if status == "unavailable" {
            return Ok(result);
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
            integer(count)?;
            result.counters.insert((owner.into(), field.into()), count);
        }
        if let Some(stages) = record["observations"]["stages"].as_object() {
            for (stage, value) in stages {
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
                let calls = value["calls"]
                    .as_u64()
                    .ok_or_else(|| invalid("invalid stage calls"))?;
                let failures = value["failures"]
                    .as_u64()
                    .ok_or_else(|| invalid("invalid stage failures"))?;
                integer(calls)?;
                integer(failures)?;
                let seconds = value["completed_span_ms"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .ok_or_else(|| invalid("invalid stage duration"))?
                    / 1000.0;
                result
                    .stages
                    .insert((owner.into(), stage.clone()), (calls, failures, seconds));
            }
        }
        Ok(result)
    }

    /// Called inside the transaction which inserts the corresponding raw record.
    pub(crate) fn apply(&self, connection: &Connection) -> Result<()> {
        for ((owner, field), count) in &self.counters {
            let old = connection
                .query_row(
                    "SELECT total FROM observation_counters WHERE owner=?1 AND field=?2",
                    params![owner, field],
                    |r| unsigned(r, 0),
                )
                .optional()?
                .unwrap_or(0);
            let sum = old
                .checked_add(*count)
                .ok_or_else(|| invalid("metric count overflow"))?;
            connection.execute(
                "INSERT INTO observation_counters VALUES(?1,?2,?3)
                 ON CONFLICT(owner,field) DO UPDATE SET total=excluded.total",
                params![owner, field, integer(sum)?],
            )?;
        }
        for ((owner, stage), (calls, failures, seconds)) in &self.stages {
            let old = connection.query_row(
                "SELECT calls,failures,completed_seconds FROM observation_stages WHERE owner=?1 AND stage=?2",
                params![owner, stage], |r| Ok((unsigned(r, 0)?, unsigned(r, 1)?, r.get::<_, f64>(2)?)),
            ).optional()?.unwrap_or_default();
            let calls = old
                .0
                .checked_add(*calls)
                .ok_or_else(|| invalid("stage calls overflow"))?;
            let failures = old
                .1
                .checked_add(*failures)
                .ok_or_else(|| invalid("stage failures overflow"))?;
            let seconds = old.2 + seconds;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(invalid("stage duration overflow"));
            }
            connection.execute(
                "INSERT INTO observation_stages VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(owner,stage) DO UPDATE SET calls=excluded.calls,
                     failures=excluded.failures,completed_seconds=excluded.completed_seconds",
                params![owner, stage, integer(calls)?, integer(failures)?, seconds],
            )?;
        }
        Ok(())
    }

    pub(crate) fn load(connection: &Connection) -> Result<Self> {
        let mut result = Self::default();
        let mut query = connection
            .prepare("SELECT owner,field,total FROM observation_counters ORDER BY owner,field")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            result
                .counters
                .insert((row.get(0)?, row.get(1)?), unsigned(row, 2)?);
        }
        let mut query = connection.prepare(
            "SELECT owner,stage,calls,failures,completed_seconds FROM observation_stages ORDER BY owner,stage",
        )?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let seconds: f64 = row.get(4)?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(invalid("invalid persisted metric duration"));
            }
            result.stages.insert(
                (row.get(0)?, row.get(1)?),
                (unsigned(row, 2)?, unsigned(row, 3)?, seconds),
            );
        }
        Ok(result)
    }

    pub(crate) fn render(&self) -> String {
        let mut text = String::from(
            "# Persisted per-process records; unavailable workers are not zero-network samples.\n",
        );
        for ((owner, field), count) in &self.counters {
            text.push_str(&format!(
                "rustdx_observed_{field}_total{{owner=\"{owner}\"}} {count}\n"
            ));
        }
        for ((owner, stage), (calls, failures, seconds)) in &self.stages {
            text.push_str(&format!(
                "rustdx_stage_calls_total{{owner=\"{owner}\",stage=\"{stage}\"}} {calls}\n\
                 rustdx_stage_failures_total{{owner=\"{owner}\",stage=\"{stage}\"}} {failures}\n\
                 rustdx_stage_completed_seconds_total{{owner=\"{owner}\",stage=\"{stage}\"}} {seconds}\n"
            ));
        }
        text
    }
}

pub(crate) fn rebuild(connection: &Connection) -> Result<()> {
    connection
        .execute_batch("DELETE FROM observation_counters; DELETE FROM observation_stages;")?;
    let mut query = connection
        .prepare("SELECT owner,payload FROM attempt_observations ORDER BY created,attempt,owner")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        let owner: String = row.get(0)?;
        let text: String = row.get(1)?;
        if text.len() > 1024 * 1024 {
            return Err(invalid("oversized persisted observation"));
        }
        Totals::contribution(&owner, &serde_json::from_str(&text)?)?.apply(connection)?;
    }
    Ok(())
}
