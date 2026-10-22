use eyre::Result;
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::{BuiltinCalendar, CalendarProvider},
    data::{Symbol, validate_bar_values, validate_ohlcv},
    file::day,
};
use serde::Serialize;
use std::{fs::File, path::Path};

pub fn load_calendar(path: Option<&Path>) -> Result<Box<dyn CalendarProvider>> {
    use rustdx_complete::calendar::{CalendarDataset, HolidayCalendar};
    let Some(path) = path else {
        return Ok(Box::new(BuiltinCalendar));
    };
    if std::fs::metadata(path)?.len() > 1024 * 1024 {
        eyre::bail!("日历JSON超出1MiB限制");
    }
    let dataset: CalendarDataset = serde_json::from_reader(File::open(path)?)?;
    Ok(Box::new(HolidayCalendar::try_from(dataset)?))
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
            calendar: serde_json::json!({
                "version": BuiltinCalendar.metadata().version,
                "source": BuiltinCalendar.metadata().source,
                "start": BuiltinCalendar.metadata().start,
                "end": BuiltinCalendar.metadata().end,
            }),
            status: "running",
            errors: vec![],
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

#[cfg(test)]
mod tests {
    use super::*;

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
