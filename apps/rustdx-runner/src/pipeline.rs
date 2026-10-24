//! Frozen inputs -> bounded normalization -> exact coverage -> sealed release -> catalog.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::NaiveDate;
use rusqlite::{Connection, params};
use rustdx_complete::{
    calendar::{CalendarDataset, CalendarProvider, HolidayCalendar},
    limit::TradingState,
    operation::OperationContext,
};
use serde::Deserialize;

use crate::{
    Error, Result,
    config::{Config, MAX_JSON},
    invalid,
    ledger::Ledger,
    model::{Catalog, Evidence, Partition, Pointer, ReleaseManifest},
    storage, supervisor,
};

mod facts;
use facts::{Facts, load as facts, visit_expected};

fn check() -> Result<()> {
    rustdx_complete::operation::check().map_err(Error::from)
}

fn snapshot(config: &Config, directory: &Path) -> Result<Facts> {
    storage::private_directory(&directory.join("raw"))?;
    storage::private_directory(&directory.join("facts"))?;
    for (source, name) in [
        (&config.calendar, "calendar.json"),
        (&config.security_history, "history.json"),
        (&config.universe, "universe.json"),
    ] {
        storage::copy_new(source, &directory.join(name), MAX_JSON)?;
    }
    storage::write_new(
        &directory.join("job.json"),
        &serde_json::to_vec_pretty(config)?,
    )?;
    let facts = facts(config, directory)?;
    // Refuse Unknown before binary copying or collection.
    visit_expected(config, &facts, |_, _, _| Ok(()))?;
    if config.tcp_day.is_some() {
        let today = chrono::Utc::now()
            .with_timezone(&rustdx_complete::data::exchange_timezone())
            .date_naive();
        if config.planned_date != today || config.through >= today {
            return Err(invalid(
                "TCP acquisition requires today's planned_date and prior-date closed daily scope",
            ));
        }
        let request = tcp_request(config, &facts, config.max_source_bytes)?;
        request.validate()?;
    }
    let mut digests = BTreeSet::from([
        config.calendar_source_sha256.clone(),
        facts.universe.source_sha256.clone(),
    ]);
    for interval in &facts.history.dataset().intervals {
        digests.insert(interval.announcement.sha256.to_ascii_lowercase());
    }
    if let Some(source) = &config.tcp_day {
        digests.insert(source.volume_evidence_sha256.clone());
    }
    let mut total = 0_u64;
    for digest in digests {
        crate::config::digest(&digest)?;
        let (actual, bytes) = storage::copy_new(
            &config.facts_directory.join(&digest),
            &directory.join("facts").join(&digest),
            config.max_source_bytes.saturating_sub(total),
        )?;
        if actual != digest {
            return Err(invalid("announcement content differs from declared digest"));
        }
        total += bytes;
    }
    for member in &facts.universe.members {
        check()?;
        if member.through < config.from || member.from > config.through {
            continue;
        }
        let mut found: Option<PathBuf> = None;
        for source in &config.day_directories {
            let path = source.join(format!("{}.day", member.code));
            if path.exists() {
                if found.is_some() {
                    return Err(invalid(format!("ambiguous source for {}", member.code)));
                }
                found = Some(path);
            }
        }
        if let Some(path) = found {
            let (_, bytes) = storage::copy_new(
                &path,
                &directory.join("raw").join(format!("{}.day", member.code)),
                config.max_source_bytes.saturating_sub(total),
            )?;
            total += bytes;
        }
    }
    // Archive and execute the same binary, not a path that can change between hash and spawn.
    storage::copy_new(
        &config.cli,
        &directory.join("collector.bin"),
        config
            .max_workspace_bytes
            .saturating_sub(storage::tree_bytes(directory)?),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            directory.join("collector.bin"),
            fs::Permissions::from_mode(0o500),
        )?;
    }
    if storage::tree_bytes(directory)? > config.max_workspace_bytes {
        return Err(invalid("input snapshot exceeds workspace budget"));
    }
    if config.tcp_day.is_some() {
        let request = tcp_request(
            config,
            &facts,
            config.max_source_bytes.saturating_sub(total),
        )?;
        storage::write_new(
            &directory.join("tcp-request.json"),
            &serde_json::to_vec(&request)?,
        )?;
    }
    Ok(facts)
}

fn tcp_request(
    config: &Config,
    facts: &Facts,
    archive_bytes: u64,
) -> Result<rustdx_cli::tcp_day::Request> {
    let codes: Vec<_> = facts
        .universe
        .members
        .iter()
        .filter(|m| m.through >= config.from && m.from <= config.through)
        .map(|m| m.code.clone())
        .collect();
    let source = config
        .tcp_day
        .clone()
        .ok_or_else(|| invalid("missing TCP source"))?;
    let schedule = config
        .tcp_day_schedule
        .clone()
        .unwrap_or(rustdx_cli::tcp_day::Schedule {
            max_parallel: config.workers.min(8),
            timeout_seconds: config.timeout_seconds - config.publish_reserve_seconds,
            max_raw_rows: (source.max_raw_rows as u64 * codes.len() as u64).min(1_000_000_000),
            max_payload_bytes: config.max_source_bytes,
        });
    Ok(rustdx_cli::tcp_day::Request {
        schema_version: rustdx_cli::tcp_day::REQUEST_SCHEMA_VERSION,
        source_contract: rustdx_cli::tcp_day::SOURCE_CONTRACT.into(),
        source,
        from: config.from,
        through: config.through,
        acquired_on: config.planned_date,
        codes,
        max_archive_bytes: archive_bytes,
        max_csv_bytes: config.max_workspace_bytes,
        max_rows: config.max_rows,
        schedule: Some(schedule),
    })
}

#[derive(Deserialize)]
struct CliReport {
    schema_version: u32,
    status: String,
    errors: Vec<String>,
    run_id: String,
    volume_unit: String,
    database_status: String,
    artifact: CliArtifact,
    sources: Vec<CliSource>,
    #[serde(default)]
    source_contract: Option<String>,
    #[serde(default = "collector_default")]
    collector_executed: bool,
}

fn collector_default() -> bool {
    true
}

#[derive(Deserialize)]
struct CliArtifact {
    sha256: String,
    bytes: u64,
    rows: u64,
    logical_keys: u64,
    state: String,
}

#[derive(Deserialize)]
struct CliSource {
    source: PathBuf,
    sha256: String,
    bytes: u64,
}

fn report(config: &Config, directory: &Path) -> Result<CliReport> {
    let report: CliReport = serde_json::from_slice(&storage::read_limited(
        &directory.join("collection.json"),
        MAX_JSON,
    )?)?;
    let tcp = config.tcp_day.is_some();
    if !(if tcp {
        report.schema_version == 8
    } else {
        matches!(report.schema_version, 6 | 7)
    }) || !report.collector_executed
        || report.status != "complete"
        || !report.errors.is_empty()
        || report.volume_unit != if tcp { "shares" } else { "lots_100_shares" }
        || (tcp && report.source_contract.as_deref() != Some(rustdx_cli::tcp_day::SOURCE_CONTRACT))
        || report.database_status != "not_started"
        || report.artifact.state != "published"
        || report.run_id.is_empty()
        || report.artifact.rows != report.artifact.logical_keys
    {
        return Err(invalid(
            "CLI report did not complete the credential-free collection contract",
        ));
    }
    let mut paths = Vec::new();
    enumerate(directory, Path::new("raw"), &mut paths)?;
    if let Some(source) = &config.tcp_day {
        let records = paths
            .iter()
            .filter(|p| {
                p.parent().is_some_and(|parent| parent.ends_with("wire"))
                    && p.file_name().is_some_and(|name| name != "inventory.json")
            })
            .count();
        if records > source.max_wire_records {
            return Err(invalid("TCP total wire record budget exceeded"));
        }
    }
    let mut expected: BTreeSet<_> = paths.into_iter().map(|p| directory.join(p)).collect();
    if tcp {
        expected.insert(directory.join("tcp-request.json"));
        expected.insert(directory.join("tcp-probes.json"));
    } else {
        expected.insert(directory.join("calendar.json"));
    }
    for source in &report.sources {
        if !expected.remove(&source.source) {
            return Err(invalid(
                "CLI source is duplicate or outside frozen snapshot",
            ));
        }
        let (hash, bytes) = storage::hash_file(&source.source, config.max_source_bytes)?;
        if hash != source.sha256 || bytes != source.bytes {
            return Err(invalid("CLI parsed bytes differ from archived input"));
        }
    }
    if !expected.is_empty() {
        return Err(invalid("CLI omitted frozen sources"));
    }
    let artifact = storage::evidence(
        directory,
        &directory.join("collected.csv"),
        config.max_workspace_bytes,
    )?;
    if artifact.sha256 != report.artifact.sha256 || artifact.bytes != report.artifact.bytes {
        return Err(invalid("CLI CSV differs from final manifest"));
    }
    Ok(report)
}

fn exempt_only(
    config: &Config,
    directory: &Path,
    facts: &Facts,
    attempt: &str,
) -> Result<Option<CliReport>> {
    let mut trading = 0_u64;
    visit_expected(config, facts, |_, _, state| {
        if state == TradingState::Trading {
            trading += 1;
        }
        Ok(())
    })?;
    if trading != 0 || fs::read_dir(directory.join("raw"))?.next().is_some() {
        return Ok(None);
    }
    storage::write_new(
        &directory.join("collected.csv"),
        b"date,code,open,high,low,close,amount,vol\n",
    )?;
    let artifact = storage::evidence(
        directory,
        &directory.join("collected.csv"),
        config.max_workspace_bytes,
    )?;
    let run_id = format!("facts-only-{attempt}");
    storage::write_new(
        &directory.join("collection.json"),
        &serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version":1,"collector_executed":false,"run_id":run_id,
            "state":"facts_only","reason":"every expected key has an explicit non-trading fact and no day source is present",
            "artifact":artifact,"rows":0
        }))?,
    )?;
    Ok(Some(CliReport {
        schema_version: 1,
        status: "complete".into(),
        errors: Vec::new(),
        run_id,
        volume_unit: "lots_100_shares".into(),
        database_status: "not_started".into(),
        artifact: CliArtifact {
            sha256: artifact.sha256,
            bytes: artifact.bytes,
            rows: 0,
            logical_keys: 0,
            state: "published".into(),
        },
        sources: Vec::new(),
        source_contract: None,
        collector_executed: false,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CsvBar {
    date: NaiveDate,
    code: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    amount: f64,
    vol: f64,
}

const HEADER: [&str; 10] = [
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

fn normalize(
    config: &Config,
    directory: &Path,
    facts: &Facts,
    report: &CliReport,
    release: &str,
) -> Result<BTreeMap<NaiveDate, Partition>> {
    let database = directory.join("normalization.sqlite");
    let mut db = Connection::open(&database)?;
    db.execute_batch(
        "PRAGMA journal_mode=DELETE; PRAGMA cache_size=-8192; PRAGMA temp_store=FILE;
        CREATE TABLE expected(date TEXT,code TEXT,state TEXT,PRIMARY KEY(date,code)) WITHOUT ROWID;
        CREATE TABLE bars(date TEXT,code TEXT,market TEXT,o REAL,h REAL,l REAL,c REAL,a REAL,v REAL,
            PRIMARY KEY(date,code)) WITHOUT ROWID;",
    )?;
    let tx = db.transaction()?;
    visit_expected(config, facts, |date, code, state| {
        let state = match state {
            TradingState::Trading => "trading",
            TradingState::Suspended => "suspended",
            TradingState::Delisted => "delisted",
        };
        tx.execute(
            "INSERT INTO expected VALUES(?1,?2,?3)",
            params![date.to_string(), code, state],
        )?;
        Ok(())
    })?;
    let mut reader = csv::ReaderBuilder::new().from_path(directory.join("collected.csv"))?;
    let mut count = 0_u64;
    for row in reader.deserialize::<CsvBar>() {
        check()?;
        count += 1;
        if count > config.max_rows {
            return Err(invalid("collected CSV exceeds row budget"));
        }
        let row = row?;
        if row.date < config.from || row.date > config.through {
            if config.tcp_day.is_some() {
                return Err(invalid("TCP CSV contains a row outside frozen date scope"));
            }
            continue;
        }
        let symbol = row
            .code
            .parse::<rustdx_complete::data::Symbol>()
            .map_err(|e| invalid(e.to_string()))?;
        if !symbol.is_a_share() {
            return Err(invalid("non A-share in complete dataset"));
        }
        rustdx_complete::data::validate_ohlcv(
            row.open, row.high, row.low, row.close, row.vol, row.amount,
        )
        .map_err(|e| invalid(e.to_string()))?;
        let volume = if config.tcp_day.is_some() {
            row.vol
        } else {
            (row.vol * 100.0).round()
        };
        if !volume.is_finite()
            || volume.fract() != 0.0
            || (config.tcp_day.is_none() && (volume > u32::MAX as f64 || volume / 100.0 != row.vol))
            || volume > 9_007_199_254_740_991.0
        {
            return Err(invalid(
                "source volume conversion is not an exact integral share count",
            ));
        }
        tx.execute(
            "INSERT INTO bars VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                row.date.to_string(),
                row.code,
                &row.code[..2],
                row.open,
                row.high,
                row.low,
                row.close,
                row.amount,
                volume
            ],
        )?;
        if count.is_multiple_of(4096)
            && storage::tree_bytes(directory)? > config.max_workspace_bytes
        {
            return Err(invalid("normalization exceeds workspace budget"));
        }
    }
    if count != report.artifact.rows {
        return Err(invalid("independent CSV row count differs from manifest"));
    }
    // Independently replay the frozen binary records, including the original u32
    // share count. Every in-scope CSV field must exactly reproduce those bytes.
    let mut source_rows = 0_u64;
    let mut lookup = tx.prepare("SELECT o,h,l,c,a,v FROM bars WHERE date=?1 AND code=?2")?;
    if config.tcp_day.is_some() && report.collector_executed {
        let request = rustdx_cli::tcp_day::Request::load(&directory.join("tcp-request.json"))?;
        let expected = tcp_request(config, facts, request.max_archive_bytes)?;
        if serde_json::to_value(&request)? != serde_json::to_value(expected)?
            || request.max_archive_bytes
                != config
                    .max_source_bytes
                    .saturating_sub(storage::tree_bytes(&directory.join("facts"))?)
            || storage::tree_bytes(&directory.join("raw"))? > request.max_archive_bytes
        {
            return Err(invalid("TCP frozen request or archive budget differs"));
        }
        let expected_codes: BTreeSet<_> = request.codes.iter().cloned().collect();
        let actual_codes = fs::read_dir(directory.join("raw"))?
            .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        if actual_codes != expected_codes {
            return Err(invalid("TCP archived security scope differs"));
        }
        rustdx_cli::tcp_day::verify_archive_budget(&directory.join("raw"), &request)?;
        for code in &request.codes {
            for row in rustdx_cli::tcp_day::replay_security(
                &directory.join("raw").join(code),
                &request,
                code,
            )? {
                check()?;
                source_rows += 1;
                let actual = lookup.query_row(params![row.date.to_string(), code], |r| {
                    Ok([
                        r.get::<_, f64>(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ])
                })?;
                if actual != [row.open, row.high, row.low, row.close, row.amount, row.vol] {
                    return Err(invalid(format!(
                        "CSV field differs from archived TCP response: {},{code}",
                        row.date
                    )));
                }
            }
        }
    } else {
        for entry in fs::read_dir(directory.join("raw"))? {
            let path = entry?.path();
            let code = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("invalid frozen source name"))?;
            let mut source = fs::File::open(&path)?;
            let length = source.metadata()?.len();
            if !length.is_multiple_of(32) {
                return Err(invalid("truncated frozen day source"));
            }
            let mut buffer = [0_u8; 4096 * 32];
            let mut remaining = length;
            while remaining != 0 {
                check()?;
                let size = remaining.min(buffer.len() as u64) as usize;
                source.read_exact(&mut buffer[..size])?;
                for chunk in buffer[..size].as_chunks::<32>().0 {
                    let bar = rustdx_complete::file::day::Day::try_from_bytes(code, chunk)
                        .map_err(|e| invalid(e.to_string()))?;
                    let date = bar.try_ymd().map_err(|e| invalid(e.to_string()))?;
                    if date < config.from || date > config.through {
                        continue;
                    }
                    source_rows += 1;
                    let actual = lookup.query_row(params![date.to_string(), code], |row| {
                        Ok([
                            row.get::<_, f64>(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ])
                    })?;
                    if actual
                        != [
                            bar.open,
                            bar.high,
                            bar.low,
                            bar.close,
                            bar.amount,
                            bar.vol as f64,
                        ]
                    {
                        return Err(invalid(format!(
                            "CSV field differs from frozen day record: {date},{code}"
                        )));
                    }
                }
                remaining -= size as u64;
            }
        }
    }
    drop(lookup);
    let normalized_rows = tx.query_row("SELECT count(*) FROM bars", [], |r| {
        crate::ledger::unsigned(r, 0)
    })?;
    if source_rows != normalized_rows {
        return Err(invalid(
            "normalized rows do not match full frozen source scope",
        ));
    }
    let missing: u64 = tx.query_row(
        "SELECT count(*) FROM expected e LEFT JOIN bars b USING(date,code)
         WHERE e.state='trading' AND b.code IS NULL",
        [],
        |r| crate::ledger::unsigned(r, 0),
    )?;
    let extra: u64 = tx.query_row(
        "SELECT count(*) FROM bars b LEFT JOIN expected e USING(date,code)
         WHERE e.code IS NULL OR e.state!='trading'",
        [],
        |r| crate::ledger::unsigned(r, 0),
    )?;
    if missing != 0 || extra != 0 {
        return Err(invalid(format!(
            "coverage mismatch: missing={missing}, unexpected={extra}"
        )));
    }
    tx.commit()?;
    storage::private_directory(&directory.join("partitions"))?;
    let mut coverage = csv::Writer::from_writer(fs::File::create(directory.join("coverage.csv"))?);
    coverage.write_record(["date", "code", "state"])?;
    let mut query = db.prepare("SELECT date,code,state FROM expected ORDER BY date,code")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        check()?;
        coverage.write_record([row.get::<_, String>(0)?, row.get(1)?, row.get(2)?])?;
    }
    coverage.flush()?;
    drop(rows);
    drop(query);
    let mut partitions = BTreeMap::new();
    for date in facts
        .calendar
        .trading_days_between(config.from, config.through)
        .map_err(|e| invalid(e.to_string()))?
    {
        check()?;
        let path = directory.join("partitions").join(format!("{date}.csv"));
        let mut writer = csv::Writer::from_writer(fs::File::create(&path)?);
        writer.write_record(HEADER)?;
        let mut query =
            db.prepare("SELECT code,market,o,h,l,c,a,v FROM bars WHERE date=?1 ORDER BY code")?;
        let mut rows = query.query([date.to_string()])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            check()?;
            writer.write_record([
                date.to_string(),
                row.get::<_, String>(1)?,
                row.get::<_, String>(0)?[2..].into(),
                "a_share".into(),
                row.get::<_, f64>(2)?.to_string(),
                row.get::<_, f64>(3)?.to_string(),
                row.get::<_, f64>(4)?.to_string(),
                row.get::<_, f64>(5)?.to_string(),
                row.get::<_, f64>(6)?.to_string(),
                row.get::<_, f64>(7)?.to_string(),
            ])?;
            count += 1;
        }
        writer.flush()?;
        let exemptions: u64 = db.query_row(
            "SELECT count(*) FROM expected WHERE date=?1 AND state!='trading'",
            [date.to_string()],
            |r| crate::ledger::unsigned(r, 0),
        )?;
        let file = storage::evidence(directory, &path, config.max_workspace_bytes)?;
        partitions.insert(
            date,
            Partition {
                date,
                release_id: release.into(),
                file,
                rows: count,
                exemptions,
                source_revision: config.source_revision,
                source_as_of: config.source_as_of,
            },
        );
    }
    drop(db);
    fs::remove_file(database)?;
    Ok(partitions)
}

fn enumerate(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        check()?;
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(root.join(&path))?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("cannot seal symlinks"));
        }
        if metadata.is_dir() {
            enumerate(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(path);
            if files.len() > 200_000 {
                return Err(invalid("release file limit exceeded"));
            }
        } else {
            return Err(invalid("cannot seal special files"));
        }
    }
    Ok(())
}

fn seal(
    config: &Config,
    directory: &Path,
    attempt: &str,
    facts: &Facts,
    cli: &CliReport,
    partitions: BTreeMap<NaiveDate, Partition>,
) -> Result<Evidence> {
    if storage::tree_bytes(directory)? > config.max_workspace_bytes {
        return Err(invalid("candidate exceeds final byte budget"));
    }
    let mut paths = Vec::new();
    enumerate(directory, Path::new(""), &mut paths)?;
    paths.sort();
    let files = paths
        .iter()
        .map(|p| storage::evidence(directory, &directory.join(p), config.max_workspace_bytes))
        .collect::<Result<Vec<_>>>()?;
    let manifest = ReleaseManifest {
        schema_version: 1,
        dataset_id: config.dataset_id.clone(),
        release_id: attempt.into(),
        attempt_id: attempt.into(),
        cli_run_id: cli.run_id.clone(),
        collector_executed: cli.collector_executed,
        contract_version: config.contract_version.clone(),
        calendar_version: facts.calendar.metadata().version.into(),
        history_version: facts.history.dataset().version.clone(),
        universe_version: facts.universe.version.clone(),
        from: config.from,
        through: config.through,
        source_revision: config.source_revision,
        source_as_of: config.source_as_of,
        created_at: crate::now(),
        files,
        partitions,
    };
    storage::write_new(
        &directory.join("release.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    let destination = config.root.join("releases").join(attempt);
    fs::rename(directory, &destination)?;
    // macOS requires write permission on a directory while moving its parent.
    // Freeze only after the private rename; ledger registration still follows
    // all file/directory fsyncs, so this path is not yet consumer-visible.
    storage::freeze_tree(&destination)?;
    storage::sync_directory(&config.root.join("attempts"))?;
    storage::sync_directory(&config.root.join("releases"))?;
    storage::evidence(&config.root, &destination.join("release.json"), MAX_JSON)
}

/// Verify the ledger-bound manifest without reading unrelated raw archives.
pub fn release_manifest(root: &Path, evidence: &Evidence) -> Result<ReleaseManifest> {
    storage::verify(root, evidence)?;
    let manifest: ReleaseManifest = serde_json::from_slice(&storage::read_limited(
        &storage::safe_join(root, &evidence.path)?,
        MAX_JSON,
    )?)?;
    crate::config::identifier(&manifest.release_id)?;
    if manifest.schema_version != 1
        || manifest.contract_version != crate::config::CONTRACT
        || manifest.partitions.is_empty()
    {
        return Err(invalid("invalid release manifest"));
    }
    for (date, partition) in &manifest.partitions {
        if *date != partition.date
            || partition.release_id != manifest.release_id
            || *date < manifest.from
            || *date > manifest.through
            || !manifest.files.contains(&partition.file)
        {
            return Err(invalid("partition not covered by sealed release"));
        }
    }
    Ok(manifest)
}

pub fn load_release(root: &Path, evidence: &Evidence) -> Result<ReleaseManifest> {
    let manifest = release_manifest(root, evidence)?;
    let directory = root.join("releases").join(&manifest.release_id);
    for file in &manifest.files {
        storage::verify(&directory, file)?;
    }
    Ok(manifest)
}

pub fn fixed_catalog(
    root: &Path,
    ledger: &Ledger,
    dataset: &str,
    version: Option<u64>,
) -> Result<Option<Catalog>> {
    crate::config::identifier(dataset)?;
    let Some(pointer) = ledger.pointer(dataset, version)? else {
        return Ok(None);
    };
    storage::verify(root, &pointer.manifest)?;
    let catalog: Catalog = serde_json::from_slice(&storage::read_limited(
        &storage::safe_join(root, &pointer.manifest.path)?,
        MAX_JSON,
    )?)?;
    if catalog.schema_version != 1
        || catalog.version != pointer.version
        || catalog.dataset_id != dataset
        || catalog.contract_version != crate::config::CONTRACT
    {
        return Err(invalid("catalog identity mismatch"));
    }
    Ok(Some(catalog))
}

pub fn publish(root: &Path, ledger: &mut Ledger, release: &str) -> Result<Pointer> {
    check()?;
    let manifest = load_release(root, &ledger.release_evidence(release)?)?;
    let previous = fixed_catalog(root, ledger, &manifest.dataset_id, None)?;
    let expected = previous.as_ref().map_or(0, |c| c.version);
    let mut partitions = previous.map_or_else(BTreeMap::new, |c| c.partitions);
    let old_scope: BTreeSet<_> = partitions
        .range(manifest.from..=manifest.through)
        .map(|(d, _)| *d)
        .collect();
    if !old_scope.is_subset(&manifest.partitions.keys().copied().collect()) {
        return Err(invalid(
            "revision would omit a previously published partition",
        ));
    }
    for (date, partition) in &manifest.partitions {
        if let Some(old) = partitions.get(date)
            && (partition.source_revision <= old.source_revision
                || partition.source_as_of < old.source_as_of)
        {
            return Err(invalid(
                "replacement must advance explicit source revision without older source cutoff",
            ));
        }
        partitions.insert(*date, partition.clone());
    }
    // A complete catalog cannot silently acquire holes when extending history.
    let calendar: CalendarDataset = serde_json::from_slice(&storage::read_limited(
        &root.join("releases").join(release).join("calendar.json"),
        MAX_JSON,
    )?)?;
    let calendar = HolidayCalendar::try_from(calendar).map_err(|e| invalid(e.to_string()))?;
    let first = *partitions
        .first_key_value()
        .ok_or_else(|| invalid("empty catalog"))?
        .0;
    let last = *partitions
        .last_key_value()
        .ok_or_else(|| invalid("empty catalog"))?
        .0;
    let required: BTreeSet<_> = calendar
        .trading_days_between(first, last)
        .map_err(|e| invalid(format!("catalog calendar must cover full history: {e}")))?
        .into_iter()
        .collect();
    if required != partitions.keys().copied().collect() {
        return Err(invalid(
            "catalog would contain missing or non-trading date partitions",
        ));
    }
    let mut database_releases = BTreeMap::new();
    for partition in partitions.values() {
        if !database_releases.contains_key(&partition.release_id) {
            let receipts = crate::database::receipts(root, ledger, &partition.release_id)?;
            let config = Config::load(
                &root
                    .join("releases")
                    .join(&partition.release_id)
                    .join("job.json"),
            )?;
            if receipts.len() != config.database_targets.len() {
                return Err(invalid(
                    "configured database target has no verified receipt",
                ));
            }
            database_releases.insert(partition.release_id.clone(), receipts);
        }
    }
    let catalog = Catalog {
        schema_version: 1,
        dataset_id: manifest.dataset_id.clone(),
        version: expected + 1,
        previous_version: expected,
        contract_version: manifest.contract_version,
        created_at: crate::now(),
        partitions,
        database_releases,
    };
    let path = root
        .join("catalogs")
        .join(format!("{release}-{}.json", catalog.version));
    let bytes = serde_json::to_vec_pretty(&catalog)?;
    // Recovery may reuse an already durable catalog only if its content is identical.
    if path.exists() {
        let mut existing: Catalog =
            serde_json::from_slice(&storage::read_limited(&path, MAX_JSON)?)?;
        existing.created_at = catalog.created_at.clone();
        if serde_json::to_vec(&existing)? != serde_json::to_vec(&catalog)? {
            return Err(invalid("orphan catalog content mismatch"));
        }
    } else {
        storage::write_new(&path, &bytes)?;
    }
    let pointer = Pointer {
        version: catalog.version,
        manifest: storage::evidence(root, &path, MAX_JSON)?,
    };
    check()?;
    ledger.publish(&catalog.dataset_id, expected, release, &pointer)?;
    Ok(pointer)
}

pub fn execute(config: &Config, context: &OperationContext) -> Result<Option<Pointer>> {
    config.validate()?;
    let _lock = storage::RootLock::acquire(&config.root)?;
    for name in ["attempts", "releases", "catalogs"] {
        storage::private_directory(&config.root.join(name))?;
    }
    let mut ledger = Ledger::open(&config.root.join("ledger.sqlite"))?;
    if !ledger.unfinished()?.is_empty() {
        return Err(invalid(
            "unfinished attempts require recover before starting another worker",
        ));
    }
    if storage::free_bytes(&config.root)?
        < config
            .min_free_bytes
            .saturating_add(config.max_workspace_bytes)
    {
        return Err(invalid("insufficient free space for reserved workspace"));
    }
    let job = config.job_id()?;
    if ledger.register(&job, &config.dataset_id, &serde_json::to_string(config)?)? == "released" {
        return Ok(None);
    }
    let attempt = format!(
        "a-{}-{}",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .ok_or_else(|| invalid("timestamp overflow"))?,
        std::process::id()
    );
    let directory = config.root.join("attempts").join(&attempt);
    fs::create_dir(&directory)?;
    ledger.begin_attempt(&job, &attempt, &directory)?;
    let observations = rustdx_cli::observation::Observations::default();
    let network_before = rustdx_complete::tcp::network_stats();
    let _observations = observations.enter();
    use rustdx_cli::observation::{Stage, measure};
    let mut observations_recorded = false;
    let result = (|| {
        check()?;
        let facts = measure(Stage::Snapshot, || snapshot(config, &directory))
            .map_err(|e| e.at("snapshot"))?;
        let cli = if let Some(report) = exempt_only(config, &directory, &facts, &attempt)? {
            report
        } else {
            let status = measure(Stage::Source, || {
                supervisor::collect(config, &directory, &attempt, &ledger, context)
            })
            .map_err(|e| e.at("worker"))?;
            if !status.success() {
                return Err(invalid(format!("CLI collection exited with {status}")));
            }
            report(config, &directory).map_err(|e| e.at("CLI report"))?
        };
        let partitions = measure(Stage::Parse, || {
            normalize(config, &directory, &facts, &cli, &attempt)
        })
        .map_err(|e| e.at("normalization"))?;
        let evidence = measure(Stage::Write, || {
            seal(config, &directory, &attempt, &facts, &cli, partitions)
        })
        .map_err(|e| e.at("seal"))?;
        ledger
            .seal(
                &attempt,
                &attempt,
                &config.dataset_id,
                &evidence,
                &cli.run_id,
            )
            .map_err(|e| e.at("ledger seal"))?;
        let manifest = load_release(&config.root, &evidence)?;
        measure(Stage::Readback, || {
            crate::database::prepare(&config.root, &ledger, &manifest, config, false)
        })
        .map_err(|e| e.at("database"))?;
        // Persist before the sole visibility transaction; an observation write
        // cannot turn an already published catalog into an apparent failure.
        let mut statistics =
            rustdx_cli::observation::process_statistics_since(&observations, true, &network_before);
        statistics["phase"] = serde_json::json!("before_publication");
        statistics["business_status_source"] = serde_json::json!("ledger_attempt_and_publication");
        ledger.record_observations(&attempt, "controller", &statistics)?;
        observations_recorded = true;
        publish(&config.root, &mut ledger, &attempt).map_err(|e| e.at("publish"))
    })();
    match result {
        Ok(pointer) => Ok(Some(pointer)),
        Err(error) => {
            if !observations_recorded {
                ledger.record_observations(
                    &attempt,
                    "controller",
                    &rustdx_cli::observation::process_statistics_since(
                        &observations,
                        false,
                        &network_before,
                    ),
                )?;
            }
            let state = if config.root.join("releases").join(&attempt).exists()
                || ledger
                    .unfinished()?
                    .iter()
                    .any(|a| a.attempt_id == attempt && a.pid.is_some())
            {
                "reconciliation_required"
            } else if error.is_cancelled() {
                "cancelled"
            } else {
                "quarantined"
            };
            ledger.finish(&attempt, state, &format!("{}: {error}", error.category()))?;
            Err(error)
        }
    }
}

/// Returns actions, retaining live/ambiguous workers instead of risking PID reuse.
pub fn recover(root: &Path) -> Result<Vec<String>> {
    let _lock = storage::RootLock::acquire(root)?;
    let mut ledger = Ledger::open(&root.join("ledger.sqlite"))?;
    let mut actions = Vec::new();
    for attempt in ledger.unfinished()? {
        if attempt.pid.is_some_and(supervisor::process_group_alive) {
            actions.push(format!(
                "{}: process group still exists; stop it through the service manager",
                attempt.attempt_id
            ));
            continue;
        }
        match ledger.release_evidence(&attempt.attempt_id) {
            Ok(evidence) => {
                let manifest = load_release(root, &evidence)?;
                let config = Config::load(
                    &root
                        .join("releases")
                        .join(&attempt.attempt_id)
                        .join("job.json"),
                )?;
                crate::database::prepare(root, &ledger, &manifest, &config, true)?;
                let pointer = publish(root, &mut ledger, &attempt.attempt_id)?;
                actions.push(format!(
                    "{}: published catalog {}",
                    attempt.attempt_id, pointer.version
                ));
            }
            Err(Error::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
                ledger.finish(
                    &attempt.attempt_id,
                    "quarantined",
                    "interrupted before durable release registration; inspect archived inputs",
                )?;
                actions.push(format!(
                    "{}: quarantined without replaying collection",
                    attempt.attempt_id
                ));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(actions)
}

/// Explicitly abandon an uncommitted file candidate after investigation.
/// Evidence remains retained and no published pointer is changed.
pub fn quarantine(root: &Path, attempt_id: &str, reason: &str) -> Result<()> {
    crate::config::identifier(attempt_id)?;
    if reason.trim().is_empty() || reason.len() > 4096 {
        return Err(invalid("quarantine requires a 1–4096 byte reason"));
    }
    let _lock = storage::RootLock::acquire(root)?;
    let mut ledger = Ledger::open(&root.join("ledger.sqlite"))?;
    let attempt = ledger
        .unfinished()?
        .into_iter()
        .find(|a| a.attempt_id == attempt_id)
        .ok_or_else(|| invalid("attempt is not unfinished"))?;
    if attempt.pid.is_some_and(supervisor::process_group_alive) {
        return Err(invalid(
            "cannot quarantine while the worker process group exists",
        ));
    }
    ledger.finish(attempt_id, "quarantined", reason)
}

pub fn operation(config: &Config, cancel_file: Option<PathBuf>) -> Result<OperationContext> {
    Ok(OperationContext::new(
        Duration::from_secs(config.timeout_seconds),
        cancel_file,
    )?)
}
