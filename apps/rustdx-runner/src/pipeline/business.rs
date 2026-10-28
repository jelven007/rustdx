//! Explicit job scope; read-only business signals beside the general ledger exporter.
use std::collections::BTreeSet;

use chrono::{DateTime, FixedOffset};
use rusqlite::OptionalExtension;
use rustdx_complete::{calendar::CalendarProvider, limit::TradingState};

use super::facts;
use crate::{Result, config::Config, consumer::PinnedDataset, invalid, ledger::Ledger, storage};

fn gauge(text: &mut String, name: &str, value: impl std::fmt::Display) {
    text.push_str(&format!("# TYPE {name} gauge\n{name} {value}\n"));
}

/// `publish_by` and the independent audit quota are supplied by the operator;
/// absent values are observable gaps, not invented production defaults.
pub fn business_metrics(
    config: &Config,
    publish_by: Option<DateTime<FixedOffset>>,
    audit_max_bytes: Option<u64>,
    now: DateTime<FixedOffset>,
) -> Result<String> {
    config.validate()?;
    if audit_max_bytes == Some(0) {
        return Err(invalid("consumer audit quota must be positive"));
    }
    let mut text = String::new();
    let today = now
        .with_timezone(&FixedOffset::east_opt(8 * 3600).expect("UTC+8"))
        .date_naive();
    let rules_through: chrono::NaiveDate = rustdx_complete::limit::RULE_CHECKED_THROUGH
        .parse()
        .map_err(|_| invalid("invalid rule horizon"))?;
    text.push_str(&format!(
        "rustdx_business_fact_remaining_days{{kind=\"rules\"}} {}\n",
        (rules_through - today).num_days()
    ));
    gauge(
        &mut text,
        "rustdx_business_metrics_timestamp_seconds",
        now.timestamp(),
    );
    gauge(
        &mut text,
        "rustdx_business_deadline_configured",
        u8::from(publish_by.is_some()),
    );
    if let Some(deadline) = publish_by {
        gauge(
            &mut text,
            "rustdx_business_publish_by_timestamp_seconds",
            deadline.timestamp(),
        );
    }
    gauge(
        &mut text,
        "rustdx_consumer_audit_quota_configured",
        u8::from(audit_max_bytes.is_some()),
    );
    if let Some(limit) = audit_max_bytes {
        gauge(&mut text, "rustdx_consumer_audit_limit_bytes", limit);
    }
    let audit = config.root.join("consumer-audit");
    let audit_bytes = match std::fs::symlink_metadata(&audit) {
        Ok(metadata) if metadata.is_dir() => storage::tree_bytes(&audit)?,
        Ok(_) => return Err(invalid("consumer audit must be a regular directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    gauge(&mut text, "rustdx_consumer_audit_bytes", audit_bytes);
    let ledger = if config.root.join("ledger.sqlite").exists() {
        Some(Ledger::open_existing_readonly(
            &config.root.join("ledger.sqlite"),
        )?)
    } else {
        None
    };
    let job = match &ledger {
        Some(ledger) => ledger
            .connection
            .query_row(
                "SELECT state,spec FROM jobs WHERE id=?1",
                [config.job_id()?],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?,
        None => None,
    };
    let released = job.as_ref().is_some_and(|(state, _)| state == "released");
    gauge(
        &mut text,
        "rustdx_business_job_released",
        u8::from(released),
    );
    let unfinished: u64 = match &ledger {
        Some(ledger) => ledger.connection.query_row(
            "SELECT count(*) FROM attempts WHERE job=?1 AND state IN ('running','evaluated','reconciliation_required')",
            [config.job_id()?], |r| crate::ledger::unsigned(r, 0),
        )?,
        None => 0,
    };
    gauge(&mut text, "rustdx_business_unfinished_attempts", unfinished);
    if let Some((_, spec)) = job.filter(|_| released) {
        let collected: Config = serde_json::from_str(&spec)?;
        let source = if collected.tcp_day.is_some() {
            "requested_tcp"
        } else {
            "local_day"
        };
        text.push_str(&format!("rustdx_business_source{{kind=\"{source}\"}} 1\n"));
    } else {
        text.push_str("rustdx_business_source{kind=\"unpublished\"} 1\n");
    }
    let facts = match facts::load_config(config) {
        Ok(facts) => facts,
        Err(_) => {
            gauge(&mut text, "rustdx_business_facts_available", 0);
            gauge(&mut text, "rustdx_business_required", -1);
            gauge(&mut text, "rustdx_business_key_check_available", 0);
            return Ok(text);
        }
    };
    for (kind, through) in [
        ("calendar", facts.calendar.metadata().end),
        ("history", facts.history.dataset().checked_through),
        ("universe", facts.universe.checked_through),
    ] {
        text.push_str(&format!(
            "rustdx_business_fact_remaining_days{{kind=\"{kind}\"}} {}\n",
            (through - today).num_days()
        ));
    }
    let dates = facts
        .calendar
        .trading_days_between(config.from, config.through)
        .map_err(|e| invalid(e.to_string()))?;
    gauge(
        &mut text,
        "rustdx_business_required",
        u8::from(!dates.is_empty()),
    );
    let mut count = 0_u64;
    if !dates.is_empty()
        && facts::visit_expected(config, &facts, |_, _, _| {
            count += 1;
            Ok(())
        })
        .is_err()
    {
        gauge(&mut text, "rustdx_business_facts_available", 0);
        gauge(&mut text, "rustdx_business_key_check_available", 0);
        return Ok(text);
    }
    // Validate announcement bytes; coverage alone does not establish their source authenticity.
    let mut digests = BTreeSet::from([
        config.calendar_source_sha256.clone(),
        facts.universe.source_sha256.clone(),
    ]);
    for interval in &facts.history.dataset().intervals {
        digests.insert(interval.announcement.sha256.to_ascii_lowercase());
    }
    for digest in digests {
        if storage::hash_file(
            &storage::safe_join(&config.facts_directory, &digest)?,
            config.max_source_bytes,
        )?
        .0 != digest
        {
            return Err(invalid("business fact announcement bytes differ"));
        }
    }
    gauge(&mut text, "rustdx_business_facts_available", 1);
    gauge(&mut text, "rustdx_business_expected_keys", count);
    let pinned = match &ledger {
        Some(ledger) if ledger.pointer(&config.dataset_id, None)?.is_some() => {
            Some(PinnedDataset::open(&config.root, &config.dataset_id, None)?)
        }
        _ => None,
    };
    let (mut missing, mut extra, mut missing_partitions, mut revision_gaps, mut actual_rows) =
        (0, 0, 0, 0, 0_u64);
    for date in dates {
        let mut expected = BTreeSet::new();
        for member in &facts.universe.members {
            if member.from <= date && date <= member.through {
                let state = facts
                    .history
                    .state_at(&member.code, date)
                    .ok_or_else(|| invalid("business security state unavailable"))?;
                if state.state == TradingState::Trading {
                    expected.insert(member.code.clone());
                }
            }
        }
        if let Some(partition) = pinned
            .as_ref()
            .and_then(|p| p.catalog().partitions.get(&date))
        {
            if partition.source_revision < config.source_revision
                || partition.source_as_of < config.source_as_of
            {
                revision_gaps += 1;
            }
            crate::consumer::read_partition(&config.root, partition, |row| {
                actual_rows += 1;
                if actual_rows > config.max_rows {
                    return Err(invalid("business readback row budget exceeded"));
                }
                if !expected.remove(&format!("{}{}", row.market, row.code)) {
                    extra += 1;
                }
                Ok(())
            })?;
        } else {
            missing_partitions += 1;
        }
        missing += expected.len();
    }
    gauge(&mut text, "rustdx_business_key_check_available", 1);
    gauge(&mut text, "rustdx_business_missing_keys", missing);
    gauge(&mut text, "rustdx_business_extra_keys", extra);
    gauge(
        &mut text,
        "rustdx_business_missing_partitions",
        missing_partitions,
    );
    gauge(&mut text, "rustdx_business_revision_gaps", revision_gaps);
    // This exporter does not qualify independent source semantics or PassivePush.
    Ok(text)
}
