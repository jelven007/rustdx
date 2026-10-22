use eyre::Result;
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::{BuiltinCalendar, CalendarProvider},
    data::{Symbol, validate_bar_values, validate_ohlcv},
    file::day,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub fn manifest_path(output: &str, requested: Option<&Path>) -> PathBuf {
    requested
        .map(Path::to_owned)
        .unwrap_or_else(|| PathBuf::from(format!("{output}.manifest.json")))
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Read and hash exactly the snapshot subsequently passed to the parser.
pub fn source_snapshot(path: &Path, limit: u64) -> Result<(SourceEvidence, Vec<u8>)> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        eyre::bail!("来源文件超出{limit}字节限制: {}", path.display());
    }
    Ok((
        SourceEvidence {
            source: path.display().to_string(),
            sha256: sha256(&bytes),
            bytes: bytes.len() as u64,
        },
        bytes,
    ))
}

#[derive(Debug, Serialize)]
pub struct SourceEvidence {
    pub source: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct OutputEvidence {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows: usize,
    pub logical_keys: usize,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub state: &'static str,
}

pub fn tracked_calendar(
    path: Option<&Path>,
    report: &mut CollectionReport,
) -> Result<Box<dyn CalendarProvider>> {
    let Some(path) = path else {
        return Ok(Box::new(BuiltinCalendar));
    };
    let (evidence, bytes) = source_snapshot(path, 1024 * 1024)?;
    report.sources.push(evidence);
    let dataset: rustdx_complete::calendar::CalendarDataset = serde_json::from_slice(&bytes)?;
    Ok(Box::new(
        rustdx_complete::calendar::HolidayCalendar::try_from(dataset)?,
    ))
}

#[derive(Debug, Serialize)]
pub struct FileReport {
    pub source: String,
    pub code: String,
    pub rows: usize,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub trait QualityRow {
    fn date(&self) -> Result<chrono::NaiveDate>;
    fn validate(&self) -> Result<()>;
    fn is_a_share(&self) -> bool;
    fn warnings(&self) -> Vec<String> {
        Vec::new()
    }
}

impl QualityRow for day::Day {
    fn date(&self) -> Result<chrono::NaiveDate> {
        Ok(rustdx_complete::data::date_from_u32(self.date)?)
    }
    fn validate(&self) -> Result<()> {
        let symbol = self.code.parse::<Symbol>()?;
        validate_bar_values(
            self.open,
            self.high,
            self.low,
            self.close,
            self.vol as f64 / 100.0,
            self.amount,
        )?;
        if symbol.is_a_share() {
            validate_ohlcv(
                self.open,
                self.high,
                self.low,
                self.close,
                self.vol as f64 / 100.0,
                self.amount,
            )?;
        }
        Ok(())
    }
    fn is_a_share(&self) -> bool {
        self.code
            .parse::<Symbol>()
            .is_ok_and(|symbol| symbol.is_a_share())
    }
    fn warnings(&self) -> Vec<String> {
        relaxed_ohlc_warning(
            &self.code,
            self.open,
            self.high,
            self.low,
            self.close,
            self.vol as f64 / 100.0,
            self.amount,
        )
    }
}

impl QualityRow for day::fq::Day {
    fn date(&self) -> Result<chrono::NaiveDate> {
        Ok(chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")?)
    }
    fn validate(&self) -> Result<()> {
        let symbol = self.code.parse::<Symbol>()?;
        validate_bar_values(
            self.open,
            self.high,
            self.low,
            self.close,
            self.vol,
            self.amount,
        )?;
        if symbol.is_a_share() {
            validate_ohlcv(
                self.open,
                self.high,
                self.low,
                self.close,
                self.vol,
                self.amount,
            )?;
        }
        if !self.factor.is_finite()
            || self.factor <= 0.0
            || !self.preclose.is_finite()
            || self.preclose <= 0.0
        {
            eyre::bail!("前收或累计收益因子非法");
        }
        Ok(())
    }
    fn is_a_share(&self) -> bool {
        self.code
            .parse::<Symbol>()
            .is_ok_and(|symbol| symbol.is_a_share())
    }
    fn warnings(&self) -> Vec<String> {
        relaxed_ohlc_warning(
            &self.code,
            self.open,
            self.high,
            self.low,
            self.close,
            self.vol,
            self.amount,
        )
    }
}

fn relaxed_ohlc_warning(
    code: &str,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    amount: f64,
) -> Vec<String> {
    let Ok(symbol) = code.parse::<Symbol>() else {
        return Vec::new();
    };
    if !symbol.is_a_share() && validate_ohlcv(open, high, low, close, volume, amount).is_err() {
        vec!["非A股品种的源OHLC区间异常，已保留原始记录".into()]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
pub fn inspect<T: QualityRow>(source: String, code: String, rows: &[T]) -> FileReport {
    inspect_with_calendar(source, code, rows, &BuiltinCalendar)
}

pub fn inspect_with_calendar<T: QualityRow>(
    source: String,
    code: String,
    rows: &[T],
    calendar: &dyn CalendarProvider,
) -> FileReport {
    let mut report = FileReport {
        source,
        code,
        rows: rows.len(),
        first_date: None,
        last_date: None,
        errors: vec![],
        warnings: vec![],
    };
    if rows.is_empty() {
        report.warnings.push("文件无日线记录".into());
    }
    let mut previous = None;
    let mut first_date = None;
    let mut last_date = None;
    for (index, row) in rows.iter().enumerate() {
        if let Err(e) = row.validate() {
            report.errors.push(format!("第{}行: {e}", index + 1));
        }
        for warning in row.warnings() {
            report
                .warnings
                .push(format!("第{}行: {warning}", index + 1));
        }
        let date = match row.date() {
            Ok(date) => date,
            Err(e) => {
                report.errors.push(format!("第{}行: {e}", index + 1));
                continue;
            }
        };
        first_date = Some(first_date.map_or(date, |first: chrono::NaiveDate| first.min(date)));
        last_date = Some(last_date.map_or(date, |last: chrono::NaiveDate| last.max(date)));
        match calendar.is_trading_day(date) {
            Ok(false) => report
                .warnings
                .push(format!("{date}: 记录落在非交易日，源口径待核实")),
            Err(error) if !report.warnings.iter().any(|w| w.contains("日历")) => report
                .warnings
                .push(format!("日历覆盖不足，部分日期未检查连续性: {error}")),
            _ => {}
        }
        if let Some(prev) = previous {
            if date <= prev {
                let issue = format!("日期重复或未升序: {prev} -> {date}");
                if row.is_a_share() {
                    report.errors.push(issue);
                } else {
                    report.warnings.push(format!("{issue}，输出将按日期规范化"));
                }
            } else {
                match calendar.trading_days_between(prev, date) {
                    Ok(days) => {
                        let missing = days.into_iter().filter(|d| *d > prev && *d < date).count();
                        if missing > 0 {
                            report.warnings.push(format!(
                                "{prev} -> {date}: 缺少{missing}个交易日，原因待核实"
                            ));
                        }
                    }
                    Err(error) => {
                        if !report.warnings.iter().any(|w| w.contains("日历")) {
                            report
                                .warnings
                                .push(format!("日历覆盖不足，部分日期未检查连续性: {error}"));
                        }
                    }
                }
            }
        }
        previous = Some(date);
    }
    report.first_date = first_date.map(|date| date.to_string());
    report.last_date = last_date.map(|date| date.to_string());
    report
}

#[derive(Serialize)]
pub struct CollectionReport {
    pub schema_version: u32,
    pub project_version: &'static str,
    pub started_at: String,
    pub finished_at: String,
    pub output: String,
    pub volume_unit: &'static str,
    pub factor_semantics: &'static str,
    pub calendar: serde_json::Value,
    pub status: &'static str,
    pub errors: Vec<String>,
    pub files: Vec<FileReport>,
    pub run_id: String,
    pub stage: &'static str,
    pub sources: Vec<SourceEvidence>,
    pub artifact: Option<OutputEvidence>,
    pub database_status: &'static str,
    pub database_table: Option<String>,
    pub query_id: Option<String>,
    pub run_version: u64,
    pub database_version_policy: &'static str,
    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    ranges: std::collections::HashMap<String, Vec<(String, String)>>,
    #[serde(skip)]
    output_rows: usize,
}

impl CollectionReport {
    pub fn new(output: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let version = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .max(0) as u64;
        Self {
            schema_version: 2,
            project_version: env!("CARGO_PKG_VERSION"),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: String::new(),
            output: output.to_owned(),
            volume_unit: "lots_100_shares",
            factor_semantics: "cumulative_return_index",
            calendar: serde_json::json!({
                "version": BuiltinCalendar.metadata().version,
                "source": BuiltinCalendar.metadata().source,
                "start": BuiltinCalendar.metadata().start,
                "end": BuiltinCalendar.metadata().end,
            }),
            status: "running",
            errors: vec![],
            files: vec![],
            run_id: format!(
                "{version}-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            stage: "parsing",
            sources: vec![],
            artifact: None,
            database_status: "not_started",
            database_table: None,
            query_id: None,
            run_version: version,
            database_version_policy: "clickhouse_server_nanoseconds_per_row; mongodb_upsert",
            path: manifest_path(output, None),
            ranges: Default::default(),
            output_rows: 0,
        }
    }
    pub fn start(output: &str, requested: Option<&Path>) -> Result<Self> {
        let mut report = Self::new(output);
        report.path = manifest_path(output, requested);
        report.checkpoint()?;
        Ok(report)
    }

    /// Reject possibly overlapping inputs with O(source files) memory.
    pub fn record_rows(&mut self, code: &str, rows: usize, first: &str, last: &str) -> Result<()> {
        let ranges = self.ranges.entry(code.to_owned()).or_default();
        if ranges
            .iter()
            .any(|(start, end)| first <= end.as_str() && last >= start.as_str())
        {
            eyre::bail!("{code}多个来源日期区间重叠，无法确认(date,code)唯一性");
        }
        ranges.push((first.to_owned(), last.to_owned()));
        self.output_rows = self
            .output_rows
            .checked_add(rows)
            .ok_or_else(|| eyre::eyre!("输出行数溢出"))?;
        Ok(())
    }

    pub fn stage_artifact(&mut self, path: &Path) -> Result<()> {
        let mut reader = File::open(path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut bytes = 0;
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            bytes += n as u64;
        }
        self.artifact = Some(OutputEvidence {
            path: self.output.clone(),
            sha256: format!("{:x}", hash.finalize()),
            bytes,
            rows: self.output_rows,
            logical_keys: self.output_rows,
            first_date: self
                .ranges
                .values()
                .flatten()
                .map(|(a, _)| a)
                .min()
                .cloned(),
            last_date: self
                .ranges
                .values()
                .flatten()
                .map(|(_, b)| b)
                .max()
                .cloned(),
            state: "publication_pending",
        });
        self.stage = "publishing";
        self.checkpoint()
    }

    pub fn verify_csv(&self, path: &Path) -> Result<()> {
        let mut reader = csv::Reader::from_path(path)?;
        let headers = reader.headers()?;
        let date = headers
            .iter()
            .position(|h| h == "date")
            .ok_or_else(|| eyre::eyre!("CSV缺少date"))?;
        let code = headers
            .iter()
            .position(|h| h == "code")
            .ok_or_else(|| eyre::eyre!("CSV缺少code"))?;
        let mut count = 0usize;
        let mut first: Option<String> = None;
        let mut last: Option<String> = None;
        for record in reader.records() {
            let record = record?;
            let date = record.get(date).ok_or_else(|| eyre::eyre!("CSV日期缺失"))?;
            let code = record.get(code).ok_or_else(|| eyre::eyre!("CSV代码缺失"))?;
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
            if !self.ranges.get(code).is_some_and(|ranges| {
                ranges
                    .iter()
                    .any(|(a, b)| date >= a.as_str() && date <= b.as_str())
            }) {
                eyre::bail!("CSV键超出已验证来源区间: {date},{code}");
            }
            if first.as_deref().is_none_or(|d| date < d) {
                first = Some(date.to_owned());
            }
            if last.as_deref().is_none_or(|d| date > d) {
                last = Some(date.to_owned());
            }
            count += 1;
        }
        let expected_first = self
            .ranges
            .values()
            .flatten()
            .map(|(a, _)| a.as_str())
            .min();
        let expected_last = self
            .ranges
            .values()
            .flatten()
            .map(|(_, b)| b.as_str())
            .max();
        if count != self.output_rows
            || first.as_deref() != expected_first
            || last.as_deref() != expected_last
        {
            eyre::bail!("CSV回读行数或日期边界与来源不一致");
        }
        Ok(())
    }

    pub fn published(&mut self) -> Result<()> {
        if let Some(artifact) = &mut self.artifact {
            artifact.state = "published";
        }
        self.checkpoint()
    }

    pub fn begin_database(&mut self, table: &str) -> Result<()> {
        self.stage = "database";
        self.database_table = Some(table.to_owned());
        self.query_id = Some(format!("rustdx-{}", self.run_id));
        // Set before sending: termination or network errors cannot prove absence of writes.
        self.database_status = "unknown";
        self.checkpoint()
    }

    pub fn finish(&mut self, result: Result<()>) -> Result<()> {
        self.files
            .sort_by(|a, b| (&a.source, &a.code).cmp(&(&b.source, &b.code)));
        self.sources.sort_by(|a, b| a.source.cmp(&b.source));
        if let Err(e) = &result {
            self.errors.push(format!("{e:#}"));
        }
        self.status = if result.is_ok() { "complete" } else { "failed" };
        self.finished_at = chrono::Utc::now().to_rfc3339();
        self.checkpoint().map_err(|e| {
            eyre::eyre!(
                "运行结果={:?}；状态持久化失败: {e:#}",
                result.as_ref().err()
            )
        })?;
        result
    }

    pub fn checkpoint(&self) -> Result<()> {
        let archive = PathBuf::from(format!("{}.{}.json", self.path.display(), self.run_id));
        self.save(&archive)?;
        self.save(&self.path)
    }

    fn save(&self, path: &Path) -> Result<()> {
        let (atomic, mut file) = AtomicFile::create(path)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        drop(file);
        atomic.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_preserve_each_run_and_verify_published_evidence() {
        let dir = std::env::temp_dir().join(format!("rustdx-evidence-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.day");
        std::fs::write(&source, b"abc").unwrap();
        let (evidence, snapshot) = source_snapshot(&source, 3).unwrap();
        assert_eq!(snapshot, b"abc");
        assert_eq!(
            evidence.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(source_snapshot(&source, 2).is_err());
        let output = dir.join("bars.csv");
        let mut report = CollectionReport::start(output.to_str().unwrap(), None).unwrap();
        let id = report.run_id.clone();
        report.sources.push(evidence);
        report
            .record_rows("sh600000", 1, "2026-10-09", "2026-10-09")
            .unwrap();
        assert!(
            report
                .record_rows("sh600000", 1, "2026-10-09", "2026-10-10")
                .is_err()
        );
        std::fs::write(&output, "date,code\n2026-10-09,sh600000\n").unwrap();
        report.verify_csv(&output).unwrap();
        report.stage_artifact(&output).unwrap();
        report.published().unwrap();
        report.finish(Ok(())).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report.path).unwrap()).unwrap();
        assert_eq!(
            saved["artifact"]["sha256"],
            sha256(&std::fs::read(&output).unwrap())
        );
        assert_eq!(saved["artifact"]["rows"], 1);
        std::fs::write(
            &output,
            "date,code\n2026-10-09,sh600000\n2026-10-09,sh600000\n",
        )
        .unwrap();
        assert!(report.verify_csv(&output).is_err());
        let next = CollectionReport::start(output.to_str().unwrap(), None).unwrap();
        assert_ne!(next.run_id, id);
        let archive = PathBuf::from(format!("{}.{}.json", report.path.display(), id));
        let prior: serde_json::Value =
            serde_json::from_slice(&std::fs::read(archive).unwrap()).unwrap();
        assert_eq!(prior["status"], "complete");
        let latest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(next.path).unwrap()).unwrap();
        assert_eq!(latest["status"], "running");
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn row(code: &str) -> day::Day {
        day::Day {
            date: 20250106,
            code: code.into(),
            open: 10.0,
            high: 10.5,
            low: 9.5,
            close: 10.6,
            amount: 1000.0,
            vol: 100,
        }
    }

    #[test]
    fn equities_are_strict_and_indexes_keep_source_anomalies_as_warnings() {
        let stock = inspect("stock.day".into(), "sz000001".into(), &[row("sz000001")]);
        assert!(!stock.errors.is_empty());
        let index = inspect("index.day".into(), "sh000001".into(), &[row("sh000001")]);
        assert!(index.errors.is_empty());
        assert!(
            index
                .warnings
                .iter()
                .any(|warning| warning.contains("OHLC"))
        );
        let mut first = row("sh000001");
        first.date = 20250107;
        let mut second = row("sh000001");
        second.date = 20250106;
        second.open = 0.0;
        second.high = 0.0;
        second.low = 0.0;
        second.close = 0.0;
        let index = inspect("index.day".into(), "sh000001".into(), &[first, second]);
        assert!(index.errors.is_empty());
        assert_eq!(index.first_date.as_deref(), Some("2025-01-06"));
        assert_eq!(index.last_date.as_deref(), Some("2025-01-07"));
        assert!(
            index
                .warnings
                .iter()
                .any(|warning| warning.contains("规范化"))
        );
        let empty: FileReport = inspect::<day::Day>("empty.day".into(), "sz131804".into(), &[]);
        assert!(empty.errors.is_empty());
        assert_eq!(empty.warnings, vec!["文件无日线记录"]);
    }
}
