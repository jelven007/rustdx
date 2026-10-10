use eyre::Result;
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::TradingCalendar,
    data::{Bar, Symbol, validate_ohlcv},
    file::day,
};
use serde::Serialize;
use std::{fs::File, path::Path};

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
}

impl QualityRow for day::Day {
    fn date(&self) -> Result<chrono::NaiveDate> {
        Ok(rustdx_complete::data::date_from_u32(self.date)?)
    }
    fn validate(&self) -> Result<()> {
        Bar::from_day(self)?;
        Ok(())
    }
}

impl QualityRow for day::fq::Day {
    fn date(&self) -> Result<chrono::NaiveDate> {
        Ok(chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")?)
    }
    fn validate(&self) -> Result<()> {
        self.code.parse::<Symbol>()?;
        validate_ohlcv(
            self.open,
            self.high,
            self.low,
            self.close,
            self.vol,
            self.amount,
        )?;
        if !self.factor.is_finite()
            || self.factor <= 0.0
            || !self.preclose.is_finite()
            || self.preclose <= 0.0
        {
            eyre::bail!("前收或累计收益因子非法");
        }
        Ok(())
    }
}

pub fn inspect<T: QualityRow>(source: String, code: String, rows: &[T]) -> FileReport {
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
        report.errors.push("文件无日线记录".into());
    }
    let mut previous = None;
    for (index, row) in rows.iter().enumerate() {
        if let Err(e) = row.validate() {
            report.errors.push(format!("第{}行: {e}", index + 1));
        }
        let date = match row.date() {
            Ok(date) => date,
            Err(e) => {
                report.errors.push(format!("第{}行: {e}", index + 1));
                continue;
            }
        };
        if report.first_date.is_none() {
            report.first_date = Some(date.to_string());
        }
        report.last_date = Some(date.to_string());
        if let Some(prev) = previous {
            if date <= prev {
                report
                    .errors
                    .push(format!("日期重复或未升序: {prev} -> {date}"));
            } else {
                match TradingCalendar::trading_days_between(prev, date) {
                    Ok(days) => {
                        let missing = days.into_iter().filter(|d| *d > prev && *d < date).count();
                        if missing > 0 {
                            report.warnings.push(format!(
                                "{prev} -> {date}: 缺少{missing}个交易日，原因待核实"
                            ));
                        }
                    }
                    Err(_) => {
                        if !report.warnings.iter().any(|w| w.contains("日历")) {
                            report
                                .warnings
                                .push("内置日历仅覆盖2024–2026，部分日期未检查连续性".into());
                        }
                    }
                }
            }
        }
        previous = Some(date);
    }
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
    pub status: &'static str,
    pub files: Vec<FileReport>,
}

impl CollectionReport {
    pub fn new(output: &str) -> Self {
        Self {
            schema_version: 1,
            project_version: env!("CARGO_PKG_VERSION"),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: String::new(),
            output: output.to_owned(),
            volume_unit: "lots_100_shares",
            factor_semantics: "cumulative_return_index",
            status: "running",
            files: vec![],
        }
    }
    pub fn write(&mut self, path: &Path, success: bool) -> Result<()> {
        self.status = if success { "complete" } else { "failed" };
        self.finished_at = chrono::Utc::now().to_rfc3339();
        let (atomic, mut file): (_, File) = AtomicFile::create(path)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        drop(file);
        atomic.commit()?;
        Ok(())
    }
}
