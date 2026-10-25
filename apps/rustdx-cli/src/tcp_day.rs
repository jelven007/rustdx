//! Exact-share 052b daily acquisitions and legacy 052d offline replay.
//! Successful source enumeration does not prove business coverage or a historical vintage.
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

use chrono::{Datelike, NaiveDate, Utc};
use rustdx_complete::{
    data::{CanonicalBar, PriceBasis, Symbol, VolumeUnit, VolumeUnitBasis},
    tcp::{
        PACK1, decode_response,
        helper::DateTime,
        ip::IpPool,
        stock::{
            BarRangeOptions, DailyBar, DailyBarsRequest, KlineData, KlineRequest,
            decode_daily_bars, decode_stock_bars,
        },
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::tcp_evidence::visit_records;

mod acquisition;
pub use acquisition::{ReservationPlan, Schedule, reservation_plan, verify_archive_budget};

pub const SOURCE_CONTRACT: &str = "a-share/tcp-052b/day/raw/cny/integer-shares/v2";
pub const LEGACY_SOURCE_CONTRACT: &str = "a-share/tcp-052d/day/raw/cny/lots-100-shares/v1";
pub const REQUEST_SCHEMA_VERSION: u32 = 2;
const JSON_LIMIT: u64 = 32 * 1024 * 1024;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub endpoint: SocketAddr,
    /// Frozen caller-maintained specification/evidence. A hash is not proof of authenticity.
    pub volume_evidence_sha256: String,
    pub page_size: u16,
    pub max_pages: usize,
    pub max_raw_rows: usize,
    pub max_payload_bytes: usize,
    pub max_wire_records: usize,
}

impl Source {
    pub fn validate(&self) -> io::Result<()> {
        if self.endpoint.port() == 0
            || self.endpoint.ip().is_unspecified()
            || self.volume_evidence_sha256.len() != 64
            || !self
                .volume_evidence_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !(1..=800).contains(&self.page_size)
            || !(1..=1024).contains(&self.max_pages)
            || !(1..=65_536).contains(&self.max_raw_rows)
            || !(1..=64 * 1024 * 1024).contains(&self.max_payload_bytes)
            || !(4..=100_000).contains(&self.max_wire_records)
        {
            return Err(invalid("invalid TCP daily source or budget"));
        }
        Ok(())
    }
    fn options(&self) -> BarRangeOptions {
        BarRangeOptions {
            page_size: self.page_size,
            max_pages: self.max_pages,
            max_rows: self.max_raw_rows,
            max_bytes: self.max_payload_bytes,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub source_contract: String,
    pub source: Source,
    pub from: NaiveDate,
    pub through: NaiveDate,
    /// Acquisition date, distinct from the historical bar dates.
    pub acquired_on: NaiveDate,
    pub codes: Vec<String>,
    /// Total across every security, including receipts and wire inventories.
    pub max_archive_bytes: u64,
    pub max_csv_bytes: u64,
    pub max_rows: u64,
    /// Absent on frozen legacy requests: retains serial, at most 32 securities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
}

impl Request {
    pub fn validate(&self) -> io::Result<()> {
        self.source.validate()?;
        if let Some(schedule) = &self.schedule {
            schedule.validate()?;
            if self.schema_version != REQUEST_SCHEMA_VERSION {
                return Err(invalid(
                    "legacy requests cannot change acquisition scheduling",
                ));
            }
        }
        if !matches!(
            (self.schema_version, self.source_contract.as_str()),
            (1, LEGACY_SOURCE_CONTRACT) | (REQUEST_SCHEMA_VERSION, SOURCE_CONTRACT)
        ) || self.from > self.through
            || self.through >= self.acquired_on
            || self.from.year() < 1990
            || (self.through - self.from).num_days() > 3660
            || self.codes.is_empty()
            || self.codes.len() > if self.schedule.is_some() { 10_000 } else { 32 }
            || self.max_archive_bytes == 0
            || self.max_csv_bytes == 0
            || self.max_rows == 0
        {
            return Err(invalid("invalid TCP daily request"));
        }
        let mut previous = None;
        for code in &self.codes {
            let symbol: Symbol = code.parse().map_err(|e| invalid(format!("{e}")))?;
            if !symbol.is_a_share()
                || symbol.market().tdx_market().is_err()
                || previous.is_some_and(|p| p >= code)
            {
                return Err(invalid(
                    "TCP daily scope requires sorted unique Shanghai/Shenzhen A shares",
                ));
            }
            previous = Some(code);
        }
        Ok(())
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let request: Self = serde_json::from_slice(&read_limited(path, JSON_LIMIT)?)?;
        request.validate()?;
        Ok(request)
    }
}

pub fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid("TCP evidence requires regular files"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("TCP evidence byte budget exceeded"));
    }
    Ok(bytes)
}

fn date(value: DateTime) -> io::Result<NaiveDate> {
    rustdx_complete::data::date_from_u32(value.to_u32()).map_err(|e| invalid(format!("{e}")))
}
fn packed(value: NaiveDate) -> u32 {
    value.year() as u32 * 10000 + value.month() * 100 + value.day()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub date: NaiveDate,
    pub code: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub amount: f64,
    /// Already converted to shares; schema 8 explicitly declares this unit.
    pub vol: f64,
}

fn row(bar: &KlineData<'_>, request: &KlineRequest<'_>, evidence: &str) -> io::Result<Row> {
    let canonical = CanonicalBar::from_kline_request(
        bar,
        request,
        VolumeUnit::Lots,
        VolumeUnitBasis::Referenced(evidence.into()),
        PriceBasis::Raw,
    )
    .map_err(|e| invalid(format!("{e}")))?;
    let bar = canonical.bar;
    if bar.volume_shares.fract() != 0.0 || bar.volume_shares > 9_007_199_254_740_991.0 {
        return Err(invalid(
            "TCP volume is not an exactly representable integral share count",
        ));
    }
    Ok(Row {
        date: bar.datetime.date(),
        code: bar.symbol.to_string(),
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
        amount: bar.amount,
        vol: bar.volume_shares,
    })
}

fn daily_row(bar: &DailyBar, code: &str) -> Row {
    Row {
        date: bar.date,
        code: code.into(),
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
        amount: bar.amount,
        // Every u32 is exactly representable in f64.
        vol: f64::from(bar.volume_shares),
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    start: u16,
    requested_count: u16,
    raw_count: usize,
    payload_bytes: usize,
    requests: usize,
    oldest: Option<NaiveDate>,
    newest: Option<NaiveDate>,
    duplicate_rows: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    source_contract: String,
    code: String,
    endpoint: SocketAddr,
    connection_id: u64,
    connection_generation: u64,
    observed_date: u32,
    started_at: chrono::DateTime<Utc>,
    fetched_at: chrono::DateTime<Utc>,
    from: NaiveDate,
    through: NaiveDate,
    volume_evidence_sha256: String,
    closed_basis: String,
    termination: String,
    raw_rows: usize,
    payload_bytes: usize,
    pages: Vec<Page>,
    rows: Vec<Row>,
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("missing parent directory"))?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

#[derive(Debug, Serialize)]
struct FileSource {
    source: PathBuf,
    sha256: String,
    bytes: u64,
}

fn hash_file(path: &Path, limit: u64) -> io::Result<(String, u64)> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid("TCP output must be a regular file"));
    }
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        rustdx_complete::operation::check()?;
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        count = count.saturating_add(size as u64);
        if count > limit {
            return Err(invalid("TCP output byte budget exceeded"));
        }
        hash.update(&buffer[..size]);
    }
    Ok((format!("{:x}", hash.finalize()), count))
}

fn sources(root: &Path, max_bytes: u64, files: &mut Vec<FileSource>) -> io::Result<u64> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(invalid("source directory must not be a symlink"));
    }
    let mut total = 0;
    let mut paths = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        rustdx_complete::operation::check()?;
        if fs::symlink_metadata(&path)?.is_dir() {
            total += sources(&path, max_bytes.saturating_sub(total), files)?;
        } else {
            let bytes = read_limited(&path, max_bytes.saturating_sub(total).min(JSON_LIMIT))?;
            total += bytes.len() as u64;
            files.push(FileSource {
                source: path,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len() as u64,
            });
            if files.len() > 100_000 {
                return Err(invalid("source file budget exceeded"));
            }
        }
    }
    Ok(total)
}

/// Collect into a new private candidate. No database, automatic source fallback,
/// or business-completeness claim is made. Failures retain partial wire evidence.
pub fn collect(directory: &Path, request: &Request) -> io::Result<()> {
    request.validate()?;
    let schedule = Schedule::for_request(request);
    let context = rustdx_complete::operation::OperationContext::new(
        std::time::Duration::from_secs(schedule.timeout_seconds),
        None,
    )?;
    let _guard = context.enter();
    if request.schema_version != REQUEST_SCHEMA_VERSION {
        return Err(invalid(
            "legacy 052d daily requests support offline replay only",
        ));
    }
    let today = Utc::now()
        .with_timezone(&rustdx_complete::data::exchange_timezone())
        .date_naive();
    if request.through >= today || request.acquired_on != today {
        return Err(invalid(
            "TCP daily collection requires a prior exchange date",
        ));
    }
    for name in ["collected.csv", "collection.json"] {
        if directory.join(name).exists() {
            return Err(invalid("TCP candidate output already exists"));
        }
    }
    let raw = directory.join("raw");
    if !fs::symlink_metadata(&raw)?.is_dir() || fs::read_dir(&raw)?.next().is_some() {
        return Err(invalid(
            "TCP candidate requires an empty regular raw directory",
        ));
    }
    let basis = read_limited(
        &directory
            .join("facts")
            .join(&request.source.volume_evidence_sha256),
        request.max_archive_bytes,
    )?;
    if format!("{:x}", Sha256::digest(&basis)) != request.source.volume_evidence_sha256 {
        return Err(invalid("TCP volume evidence digest differs"));
    }
    let pool = IpPool::new(vec![request.source.endpoint])?;
    let daily = pool.refresh()?;
    write_new(
        &directory.join("tcp-probes.json"),
        &serde_json::to_vec(&daily)?,
    )?;
    if daily.valid_nodes != [request.source.endpoint] {
        let failure = daily
            .results
            .iter()
            .flat_map(|n| &n.rounds)
            .find_map(|r| r.failure.as_ref())
            .map(|f| format!("{:?}: {}", f.stage, f.message))
            .unwrap_or_else(|| "node absent from valid pool".into());
        return Err(invalid(format!(
            "explicit TCP source failed daily protocol probes: {failure}"
        )));
    }
    let config = rustdx_complete::tcp::TcpConfig {
        ip: Some(request.source.endpoint),
        ip_pool: Some(pool),
        recheck_empty: true,
        ..Default::default()
    };
    let output = directory.join("collected.csv");
    let summary = acquisition::collect(&raw, &output, request, &config)?;
    let (artifact_hash, artifact_bytes) = hash_file(&output, request.max_csv_bytes)?;
    let mut files = Vec::new();
    sources(&raw, request.max_archive_bytes, &mut files)?;
    for name in ["tcp-request.json", "tcp-probes.json"] {
        let path = directory.join(name);
        let bytes = read_limited(&path, JSON_LIMIT)?;
        files.push(FileSource {
            source: path,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
        });
    }
    let report = serde_json::to_vec(&serde_json::json!({
        "schema_version":8,"status":"complete","errors":[],"collector_executed":true,
        "run_id":format!("tcp-{}-{}",std::process::id(),Utc::now().timestamp_nanos_opt().ok_or_else(||invalid("timestamp overflow"))?),
        "source_contract":SOURCE_CONTRACT,"business_coverage":"unverified",
        "volume_unit":"shares","database_status":"not_started",
        "acquisition":summary,
        "artifact":{"sha256":artifact_hash,"bytes":artifact_bytes,
            "rows":summary.rows,"logical_keys":summary.rows,"state":"published"},
        "sources":files
    }))?;
    if report.len() as u64 > JSON_LIMIT {
        return Err(invalid("TCP report byte budget exceeded"));
    }
    rustdx_complete::operation::check()?;
    write_new(&directory.join("collection.json"), &report)
}

/// Independently decode every archived request/response pair and reproduce the
/// receipt's page counts, offsets, duplicate boundaries and all canonical fields.
pub fn replay_security(directory: &Path, request: &Request, code: &str) -> io::Result<Vec<Row>> {
    request.validate()?;
    if !request.codes.iter().any(|c| c == code) {
        return Err(invalid("TCP replay security outside frozen request"));
    }
    let receipt: Receipt =
        serde_json::from_slice(&read_limited(&directory.join("receipt.json"), JSON_LIMIT)?)?;
    let observed = receipt
        .started_at
        .with_timezone(&rustdx_complete::data::exchange_timezone())
        .date_naive();
    if receipt.schema_version != request.schema_version
        || receipt.source_contract != request.source_contract
        || receipt.code != code
        || receipt.endpoint != request.source.endpoint
        || receipt.connection_id == 0
        || receipt.connection_generation != 0
        || receipt.from != request.from
        || receipt.through != request.through
        || receipt.volume_evidence_sha256 != request.source.volume_evidence_sha256
        || receipt.closed_basis != "prior_exchange_date_local_clock"
        || receipt.through >= observed
        || observed != request.acquired_on
        || receipt.observed_date != packed(observed)
        || receipt.started_at > receipt.fetched_at
        || receipt
            .fetched_at
            .with_timezone(&rustdx_complete::data::exchange_timezone())
            .date_naive()
            != observed
        || receipt.pages.is_empty()
        || receipt.pages.len() > request.source.max_pages
        || receipt.raw_rows > request.source.max_raw_rows
        || receipt.payload_bytes > request.source.max_payload_bytes
    {
        return Err(invalid("TCP receipt identity, date or budget differs"));
    }
    let symbol: Symbol = code.parse().map_err(|e| invalid(format!("{e}")))?;
    let market = symbol
        .market()
        .tdx_market()
        .map_err(|e| invalid(format!("{e}")))?;
    let mut pending = None;
    let mut handshake = false;
    let mut page_index = 0;
    let mut request_index = 0;
    let mut page_bytes = 0;
    let mut raw_rows = 0;
    let mut payload_bytes = 0;
    let mut offset = 0_u32;
    let mut all = BTreeMap::<NaiveDate, Row>::new();
    let mut daily_all = BTreeMap::<NaiveDate, DailyBar>::new();
    let mut previous_oldest = None;
    let mut termination = None;
    let mut last_time = None;
    visit_records(
        &directory.join("wire"),
        request.max_archive_bytes,
        request.source.max_wire_records,
        |_, record| {
            let captured: chrono::DateTime<Utc> = record
                .captured_at
                .parse()
                .map_err(|_| invalid("invalid wire capture time"))?;
            if record.endpoint != receipt.endpoint
                || last_time.is_some_and(|t| t > captured)
                || captured > receipt.fetched_at
                || captured
                    .with_timezone(&rustdx_complete::data::exchange_timezone())
                    .date_naive()
                    != observed
            {
                return Err(invalid("TCP wire observation differs from receipt"));
            }
            last_time = Some(captured);
            match record.phase.as_str() {
                "request"
                    if pending.is_none()
                        && record.header.is_none()
                        && record.body.is_empty()
                        && record.error_kind.is_none() =>
                {
                    pending = Some(record.request);
                    return Ok(());
                }
                "response"
                    if pending.as_ref() == Some(&record.request) && record.error_kind.is_none() =>
                {
                    pending = None;
                }
                _ => return Err(invalid("TCP wire pairing or failed acquisition")),
            }
            let body = decode_response(
                &record.request,
                &record
                    .header
                    .ok_or_else(|| invalid("missing wire header"))?,
                &record.body,
                true,
            )?;
            if !handshake {
                if record.request != PACK1 {
                    return Err(invalid("TCP archive must begin with one handshake"));
                }
                handshake = true;
                return Ok(());
            }
            if captured < receipt.started_at || termination.is_some() {
                return Err(invalid("TCP page outside acquisition interval"));
            }
            let page = receipt
                .pages
                .get(page_index)
                .ok_or_else(|| invalid("TCP receipt omitted a page"))?;
            let count = usize::from(request.source.page_size)
                .min(request.source.max_raw_rows - raw_rows) as u16;
            let parser = KlineRequest::new(
                market,
                symbol.code(),
                9,
                u16::try_from(offset).map_err(|_| invalid("TCP offset overflow"))?,
                count,
            )?;
            let expected_request = if request.schema_version == REQUEST_SCHEMA_VERSION {
                let id = u32::from_le_bytes(
                    record
                        .request
                        .get(1..5)
                        .ok_or_else(|| invalid("short TCP daily request"))?
                        .try_into()
                        .map_err(|_| invalid("TCP request id"))?,
                );
                DailyBarsRequest::new(market, symbol.code(), parser.start(), count)?.encode(id)?
            } else {
                parser.encode().to_vec()
            };
            if record.request != expected_request
                || page.start != offset as u16
                || page.requested_count != count
                || (page.raw_count == 0 && page.requests != 2)
                || !(1..=2).contains(&page.requests)
            {
                return Err(invalid("TCP request or raw offset differs from receipt"));
            }
            let bars: Vec<Row> = if request.schema_version == REQUEST_SCHEMA_VERSION {
                let page = decode_daily_bars(
                    &DailyBarsRequest::new(market, symbol.code(), parser.start(), count)?,
                    &body,
                )?;
                for bar in &page.records {
                    if daily_all.get(&bar.date).is_some_and(|old| old != bar) {
                        return Err(invalid("052b duplicate date has conflicting raw fields"));
                    }
                    daily_all.insert(bar.date, bar.clone());
                }
                page.records
                    .iter()
                    .map(|bar| daily_row(bar, code))
                    .collect()
            } else {
                decode_stock_bars(&parser, &body)?
                    .iter()
                    .map(|bar| row(bar, &parser, &request.source.volume_evidence_sha256))
                    .collect::<io::Result<_>>()?
            };
            request_index += 1;
            page_bytes += body.len();
            if request_index < page.requests {
                if !bars.is_empty() {
                    return Err(invalid(
                        "TCP repeated request was not an empty confirmation",
                    ));
                }
                return Ok(());
            }
            let oldest = bars.first().map(|b| b.date);
            let newest = bars.last().map(|b| b.date);
            if bars.windows(2).any(|pair| pair[0].date >= pair[1].date) {
                return Err(invalid("TCP daily page dates are not strictly ascending"));
            }
            if !bars.is_empty() && previous_oldest.is_some_and(|p| oldest >= Some(p)) {
                return Err(invalid("TCP pages do not progress into history"));
            }
            let mut duplicates = 0;
            for bar in &bars {
                let current = bar.clone();
                if current.date > observed || current.date.year() < 1990 {
                    return Err(invalid(
                        "TCP source returned an invalid historical bar date",
                    ));
                }
                if let Some(prior) = all.get(&current.date) {
                    if prior != &current {
                        return Err(invalid("TCP duplicate date has conflicting fields"));
                    }
                    duplicates += 1;
                } else {
                    all.insert(current.date, current);
                }
            }
            let actual = Page {
                start: parser.start(),
                requested_count: count,
                raw_count: bars.len(),
                payload_bytes: page_bytes,
                requests: request_index,
                oldest,
                newest,
                duplicate_rows: duplicates,
            };
            if &actual != page {
                return Err(invalid("TCP page metadata differs from archived bytes"));
            }
            raw_rows += bars.len();
            payload_bytes += page_bytes;
            if bars.is_empty() {
                termination = Some("EmptyPageObserved");
            } else if oldest.is_some_and(|d| d < request.from) {
                termination = Some("LowerBoundReached");
            }
            previous_oldest = oldest;
            offset += bars.len() as u32;
            page_index += 1;
            request_index = 0;
            page_bytes = 0;
            Ok(())
        },
    )?;
    let rows: Vec<_> = all
        .into_iter()
        .filter(|(d, _)| *d >= request.from && *d <= request.through)
        .map(|(_, r)| r)
        .collect();
    if pending.is_some()
        || !handshake
        || request_index != 0
        || page_index != receipt.pages.len()
        || termination != Some(receipt.termination.as_str())
        || raw_rows != receipt.raw_rows
        || payload_bytes != receipt.payload_bytes
        || rows != receipt.rows
    {
        return Err(invalid(
            "TCP receipt does not reproduce full archived source scope",
        ));
    }
    Ok(rows)
}
