//! Reservation-based windows. Workers return counts, never queued decoded rows.
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

use rustdx_complete::{
    operation::OperationContext,
    tcp::{TcpConfig, stock::Client},
};
use serde::{Deserialize, Serialize};

use super::{
    JSON_LIMIT, Page, Receipt, Request, SOURCE_CONTRACT, Symbol, daily_row, date, invalid, packed,
    read_limited, replay_security, sources, write_new,
};
use crate::tcp_evidence::FrameArchive;

// A range checks its byte budget after a page and possible empty confirmation.
// Reserve both maximum wire-declared decompressed bodies before starting it.
const FINAL_PAGE_RESERVE: u64 = 2 * u16::MAX as u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub max_parallel: usize,
    /// Covers probes, collection, replay and report; cannot extend a parent deadline.
    pub timeout_seconds: u64,
    /// Raw rows include out-of-range rows and duplicate page boundaries.
    pub max_raw_rows: u64,
    /// Uncompressed page bodies, including repeated empty confirmations.
    pub max_payload_bytes: u64,
}

impl Schedule {
    pub fn validate(&self) -> io::Result<()> {
        if !(1..=8).contains(&self.max_parallel)
            || !(1..=86_400).contains(&self.timeout_seconds)
            || !(1..=1_000_000_000).contains(&self.max_raw_rows)
            || self.max_payload_bytes == 0
        {
            return Err(invalid("invalid TCP acquisition schedule"));
        }
        Ok(())
    }

    pub(super) fn for_request(request: &Request) -> Self {
        request.schedule.clone().unwrap_or(Self {
            max_parallel: 1,
            timeout_seconds: 300,
            max_raw_rows: request.source.max_raw_rows as u64 * request.codes.len() as u64,
            max_payload_bytes: (request.source.max_payload_bytes as u64 + FINAL_PAGE_RESERVE)
                * request.codes.len() as u64,
        })
    }
}

#[derive(Debug, Default, Serialize)]
pub(super) struct Summary {
    pub rows: u64,
    pub securities: usize,
    pub raw_rows: u64,
    pub payload_bytes: u64,
    pub archive_bytes: u64,
    pub wire_records: usize,
    pub max_parallel: usize,
}

#[derive(Debug)]
struct Grant {
    archive: u64,
    wire: usize,
}

struct Budget {
    raw: u64,
    payload: u64,
    archive: u64,
    wire: usize,
}

impl Budget {
    fn slot_limits(&self, request: &Request, count: usize) -> BTreeMap<&'static str, usize> {
        let source = &request.source;
        BTreeMap::from([
            ("requested", count),
            (
                "raw_rows",
                (self.raw / source.max_raw_rows as u64).min(8) as usize,
            ),
            (
                "payload_bytes",
                (self.payload / (source.max_payload_bytes as u64 + FINAL_PAGE_RESERVE)).min(8)
                    as usize,
            ),
            (
                "archive_bytes",
                (self.archive / (receipt_reserve(request) + 1024)).min(8) as usize,
            ),
            ("wire_records", self.wire / 4),
        ])
    }

    fn window(&self, request: &Request, count: usize) -> io::Result<Vec<Grant>> {
        let limits = self.slot_limits(request, count);
        let slots = limits.values().copied().min().unwrap_or(0);
        if slots == 0 {
            return Err(invalid(
                "TCP total acquisition budget cannot reserve another security",
            ));
        }
        Ok((0..slots)
            .map(|_| Grant {
                archive: self.archive / slots as u64,
                wire: self.wire / slots,
            })
            .collect())
    }

    fn charge(&mut self, used: &Summary) -> io::Result<()> {
        self.raw = self
            .raw
            .checked_sub(used.raw_rows)
            .ok_or_else(|| invalid("TCP total raw row budget exceeded"))?;
        self.payload = self
            .payload
            .checked_sub(used.payload_bytes)
            .ok_or_else(|| invalid("TCP total payload budget exceeded"))?;
        self.archive = self
            .archive
            .checked_sub(used.archive_bytes)
            .ok_or_else(|| invalid("TCP total archive budget exceeded"))?;
        self.wire = self
            .wire
            .checked_sub(used.wire_records)
            .ok_or_else(|| invalid("TCP total wire record budget exceeded"))?;
        Ok(())
    }
}

fn receipt_reserve(request: &Request) -> u64 {
    (request.source.max_raw_rows as u64 * 512 + 65536).min(JSON_LIMIT)
}

/// The same initial reservation calculation used by collection. No network or disk writes.
#[derive(Debug, Serialize)]
pub struct ReservationPlan {
    pub max_parallel: usize,
    pub effective_parallel: usize,
    pub slot_limits: BTreeMap<&'static str, usize>,
    pub limiting_budgets: Vec<&'static str>,
    pub securities: usize,
    pub raw_rows_per_security: u64,
    pub payload_reserve_per_security: u64,
    pub archive_reserve_per_security: u64,
    pub minimum_collection_wire_records: u64,
    pub maximum_collection_requests: u64,
    pub worst_case_raw_rows: u64,
    pub worst_case_payload_bytes: u64,
    pub worst_case_budget_gaps: Vec<&'static str>,
    pub deadline_seconds: u64,
}

pub fn reservation_plan(request: &Request, connection_limit: usize) -> io::Result<ReservationPlan> {
    request.validate()?;
    let schedule = Schedule::for_request(request);
    let budget = Budget {
        raw: schedule.max_raw_rows,
        payload: schedule.max_payload_bytes,
        archive: request.max_archive_bytes,
        wire: request.source.max_wire_records,
    };
    let mut slot_limits = budget.slot_limits(request, schedule.max_parallel);
    slot_limits.insert("connections", connection_limit);
    slot_limits.insert("securities", request.codes.len());
    let effective_parallel = slot_limits.values().copied().min().unwrap_or(0);
    let limiting_budgets = slot_limits
        .iter()
        .filter_map(|(label, count)| (*count == effective_parallel).then_some(*label))
        .collect();
    let securities = request.codes.len() as u64;
    let raw_rows_per_security = request.source.max_raw_rows as u64;
    let payload_reserve_per_security = request.source.max_payload_bytes as u64 + FINAL_PAGE_RESERVE;
    let archive_reserve_per_security = receipt_reserve(request) + 1024;
    let worst_case_raw_rows = securities * raw_rows_per_security;
    let worst_case_payload_bytes = securities * payload_reserve_per_security;
    let mut worst_case_budget_gaps = Vec::new();
    if worst_case_raw_rows > budget.raw {
        worst_case_budget_gaps.push("raw_rows");
    }
    if worst_case_payload_bytes > budget.payload {
        worst_case_budget_gaps.push("payload_bytes");
    }
    if securities * archive_reserve_per_security > budget.archive {
        worst_case_budget_gaps.push("receipt_archive_reservations");
    }
    let maximum_collection_requests = securities * (1 + 2 * request.source.max_pages as u64);
    if maximum_collection_requests * 2 > budget.wire as u64 {
        worst_case_budget_gaps.push("wire_records");
    }
    Ok(ReservationPlan {
        max_parallel: schedule.max_parallel,
        effective_parallel,
        slot_limits,
        limiting_budgets,
        securities: request.codes.len(),
        raw_rows_per_security,
        payload_reserve_per_security,
        archive_reserve_per_security,
        minimum_collection_wire_records: securities * 4,
        maximum_collection_requests,
        worst_case_raw_rows,
        worst_case_payload_bytes,
        worst_case_budget_gaps,
        deadline_seconds: schedule.timeout_seconds,
    })
}

fn fetch(
    raw: &Path,
    request: &Request,
    config: &TcpConfig,
    code: &str,
    grant: &Grant,
) -> io::Result<Summary> {
    rustdx_complete::operation::check()?;
    let symbol: Symbol = code.parse().map_err(|e| invalid(format!("{e}")))?;
    let market = symbol
        .market()
        .tdx_market()
        .map_err(|e| invalid(format!("{e}")))?;
    let directory = raw.join(code);
    fs::create_dir(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    let reserve = receipt_reserve(request);
    let archive = Arc::new(FrameArchive::create(
        &directory.join("wire"),
        grant.archive - reserve,
        grant.wire,
    )?);
    let result = (|| {
        let mut client = Client::with_config_and_observer(config, archive.clone())?;
        client.tcp.set_heartbeat_interval(None)?;
        client.daily_bars_range_with_options(
            market,
            symbol.code(),
            Some(packed(request.from)),
            Some(packed(request.through)),
            request.source.options(),
        )
    })();
    // Seal partial failures as well. No worker remains active when its archive is sealed.
    let sealed = archive.finish();
    let collection = result?;
    sealed?;
    if !collection.complete || collection.pages.iter().any(|p| !p.accepted) {
        return Err(invalid(format!(
            "incomplete TCP source enumeration: {:?}",
            collection.termination
        )));
    }
    for timestamp in [collection.started_at, collection.fetched_at] {
        if timestamp
            .with_timezone(&rustdx_complete::data::exchange_timezone())
            .date_naive()
            != request.acquired_on
        {
            return Err(invalid(
                "TCP daily acquisition crossed the observation date",
            ));
        }
    }
    let pages = collection
        .pages
        .iter()
        .map(|p| {
            Ok(Page {
                start: p.start,
                requested_count: p.requested_count,
                raw_count: p.raw_count,
                payload_bytes: p.payload_bytes,
                requests: p.requests,
                oldest: p.oldest.map(date).transpose()?,
                newest: p.newest.map(date).transpose()?,
                duplicate_rows: p.duplicate_rows,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let receipt = Receipt {
        schema_version: super::REQUEST_SCHEMA_VERSION,
        source_contract: SOURCE_CONTRACT.into(),
        code: code.into(),
        endpoint: collection.source.endpoint,
        connection_id: collection.source.connection_id,
        connection_generation: collection.source.connection_generation,
        observed_date: collection.source.observed_date,
        started_at: collection.started_at,
        fetched_at: collection.fetched_at,
        from: request.from,
        through: request.through,
        volume_evidence_sha256: request.source.volume_evidence_sha256.clone(),
        closed_basis: "prior_exchange_date_local_clock".into(),
        termination: format!("{:?}", collection.termination),
        raw_rows: collection.raw_rows,
        payload_bytes: collection.payload_bytes,
        pages,
        rows: collection
            .records
            .iter()
            .map(|bar| daily_row(bar, code))
            .collect(),
    };
    let bytes = serde_json::to_vec(&receipt)?;
    if bytes.len() as u64 > reserve {
        return Err(invalid("TCP receipt exceeds reserved bytes"));
    }
    write_new(&directory.join("receipt.json"), &bytes)?;
    let mut files = Vec::new();
    let archive_bytes = sources(&directory, grant.archive, &mut files)?;
    Ok(Summary {
        rows: receipt.rows.len() as u64,
        securities: 1,
        raw_rows: receipt.raw_rows as u64,
        payload_bytes: receipt.payload_bytes as u64,
        archive_bytes,
        wire_records: files.len() - 2,
        max_parallel: 1,
    })
}

struct BoundedCsv {
    file: File,
    remaining: u64,
}

impl Write for BoundedCsv {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        rustdx_complete::operation::check().map_err(io::Error::other)?;
        if bytes.len() as u64 > self.remaining {
            return Err(invalid("TCP CSV byte budget exceeded"));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(super) fn collect(
    raw: &Path,
    output: &Path,
    request: &Request,
    config: &TcpConfig,
) -> io::Result<Summary> {
    let schedule = Schedule::for_request(request);
    let local = OperationContext::new(Duration::from_secs(schedule.timeout_seconds), None)?;
    let _guard = local.enter();
    let inherited =
        OperationContext::current().ok_or_else(|| invalid("missing acquisition context"))?;
    let mut budget = Budget {
        raw: schedule.max_raw_rows,
        payload: schedule.max_payload_bytes,
        archive: request.max_archive_bytes,
        wire: request.source.max_wire_records,
    };
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(BoundedCsv {
            file: OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?,
            remaining: request.max_csv_bytes,
        });
    writer.write_record([
        "date", "code", "open", "high", "low", "close", "amount", "vol",
    ])?;
    let parallel = schedule
        .max_parallel
        .min(rustdx_complete::tcp::network_policy().max_connections);
    let mut summary = Summary::default();
    while summary.securities < request.codes.len() {
        rustdx_complete::operation::check()?;
        let start = summary.securities;
        let grants = budget.window(request, parallel.min(request.codes.len() - start))?;
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(grants.len());
            let launched = (|| {
                for (index, grant) in grants.iter().enumerate() {
                    let inherited = inherited.clone();
                    let local = &local;
                    let code = &request.codes[start + index];
                    handles.push(
                        std::thread::Builder::new()
                            .name("rustdx-day".into())
                            .spawn_scoped(scope, move || {
                                let _guard = inherited.enter();
                                let result = fetch(raw, request, config, code, grant);
                                if result.is_err() {
                                    local.cancel();
                                }
                                result
                            })?,
                    );
                }
                Ok::<_, io::Error>(())
            })();
            if launched.is_err() {
                local.cancel();
            }
            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                match handle.join() {
                    Ok(result) => results.push(result),
                    Err(_) => {
                        local.cancel();
                        results.push(Err(io::Error::other("TCP acquisition worker panicked")));
                    }
                }
            }
            launched.map(|_| results)
        })?;
        // All reservations belong to this window; unused capacity is released only after join.
        for (index, result) in results.into_iter().enumerate() {
            let used = result?;
            budget.charge(&used)?;
            if summary
                .rows
                .checked_add(used.rows)
                .is_none_or(|n| n > request.max_rows)
            {
                local.cancel();
                return Err(invalid("TCP total row budget exceeded"));
            }
            for row in replay_security(
                &raw.join(&request.codes[start + index]),
                request,
                &request.codes[start + index],
            )? {
                writer.serialize(row)?;
            }
            writer.flush()?;
            summary.rows += used.rows;
            summary.securities += 1;
            summary.raw_rows += used.raw_rows;
            summary.payload_bytes += used.payload_bytes;
            summary.archive_bytes += used.archive_bytes;
            summary.wire_records += used.wire_records;
            summary.max_parallel = summary.max_parallel.max(grants.len());
        }
    }
    writer.flush()?;
    writer.get_ref().file.sync_all()?;
    Ok(summary)
}

/// Independent total-budget validation for callers replaying a complete archive.
/// Per-security identity and fields must additionally pass `replay_security`.
pub fn verify_archive_budget(raw: &Path, request: &Request) -> io::Result<()> {
    request.validate()?;
    let schedule = Schedule::for_request(request);
    let mut raw_rows = 0_u64;
    let mut payload = 0_u64;
    for code in &request.codes {
        rustdx_complete::operation::check()?;
        let receipt: Receipt = serde_json::from_slice(&read_limited(
            &raw.join(code).join("receipt.json"),
            JSON_LIMIT,
        )?)?;
        raw_rows = raw_rows
            .checked_add(receipt.raw_rows as u64)
            .ok_or_else(|| invalid("TCP raw row count overflow"))?;
        payload = payload
            .checked_add(receipt.payload_bytes as u64)
            .ok_or_else(|| invalid("TCP payload count overflow"))?;
    }
    if raw_rows > schedule.max_raw_rows || payload > schedule.max_payload_bytes {
        return Err(invalid("TCP archived totals exceed frozen schedule"));
    }
    Ok(())
}
