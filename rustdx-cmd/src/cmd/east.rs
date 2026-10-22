use argh::FromArgs;
use eyre::Result;
use rustdx_cli::eastmoney::*;
use rustdx_complete::atomic_file::AtomicFile;

/// 东方财富收盘快照；增量累计收益因子需要上一交易日基准。
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "east")]
pub struct EastCmd {
    /// 输出文件，默认eastmoney.csv；clickhouse表示导入数据库。
    #[argh(option, short = 'o', default = r#""eastmoney.csv".into()"#)]
    pub output: String,
    /// 前一交易日因子CSV，或clickhouse。
    #[argh(option, short = 'p')]
    pub previous: Option<std::path::PathBuf>,
    /// 旧参数兼容入口；数据校验不能被忽略，此参数已停用。
    #[argh(option, short = 'i')]
    pub ignore: Vec<String>,
    /// 查询数据库前因子时，保留factor.csv。
    #[argh(switch)]
    pub keep_factor: bool,
    /// 获取数量上限，默认全部。
    #[argh(option)]
    pub max: Option<u16>,
    /// 输出JSON数组；未指定-o时写eastmoney.json。
    #[argh(switch, short = 'j')]
    pub json: bool,
    /// 数据库表名，默认rustdx.tmp。
    #[argh(option, short = 't', default = "String::from(\"rustdx.tmp\")")]
    pub table: String,
    /// 采集来源、日期与质量报告JSON。
    #[argh(option)]
    pub report: Option<std::path::PathBuf>,
    /// 已核实的完整休市日历JSON。
    #[argh(option)]
    pub calendar: Option<std::path::PathBuf>,
}

impl EastCmd {
    fn output_path(&self) -> &str {
        if self.json && self.output == "eastmoney.csv" {
            "eastmoney.json"
        } else {
            &self.output
        }
    }

    pub fn run(&self) -> Result<()> {
        if self.json && self.output == "clickhouse" {
            eyre::bail!("JSON输出不能直接导入ClickHouse，请使用CSV");
        }
        if !self.ignore.is_empty() {
            eyre::bail!("ignore参数已停用，不能绕过行情质量校验");
        }
        let mut outputs = vec![std::path::PathBuf::from(self.output_path())];
        outputs.push(crate::quality::manifest_path(
            self.output_path(),
            self.report.as_deref(),
        ));
        if self.keep_factor && self.previous.as_deref() == Some(std::path::Path::new("clickhouse"))
        {
            outputs.push("factor.csv".into());
        }
        let mut inputs = crate::paths::previous_input(self.previous.as_deref())
            .into_iter()
            .collect::<Vec<_>>();
        inputs.extend(self.calendar.iter().cloned());
        crate::paths::validate(&outputs, &inputs)?;
        let _outputs = crate::paths::OutputLocks::acquire(&outputs)?;
        let _target = crate::io::sinks::target_lock(&self.output, &self.table)?;
        let mut report =
            crate::quality::CollectionReport::start(self.output_path(), self.report.as_deref())?;
        let result = (|| {
            if self.output == "clickhouse" {
                crate::io::setup_clickhouse(true, &self.table)?;
            }
            let calendar = crate::quality::tracked_calendar(self.calendar.as_deref(), &mut report)?;
            let data = fetch_with_calendar(self.max, calendar.as_ref())?;
            self.process_with_calendar(data, &mut report, calendar.as_ref())?;
            if self.output == "clickhouse" {
                crate::io::sinks::import_clickhouse(&self.output, &self.table, false, &mut report)?;
            }
            Ok(())
        })();
        report.finish(result)
    }

    #[cfg(test)]
    fn process(
        &self,
        data: EastMarket,
        report: &mut crate::quality::CollectionReport,
    ) -> Result<()> {
        self.process_with_calendar(data, report, &rustdx_complete::calendar::BuiltinCalendar)
    }

    fn process_with_calendar(
        &self,
        mut data: EastMarket,
        report: &mut crate::quality::CollectionReport,
        calendar: &dyn rustdx_complete::calendar::CalendarProvider,
    ) -> Result<()> {
        // Evidence describes the normalized fetched payload, not unavailable raw HTTP bytes.
        let snapshot = serde_json::to_vec(&data)?;
        report.sources.push(crate::quality::SourceEvidence {
            source: "eastmoney:normalized_payload".into(),
            sha256: crate::quality::sha256(&snapshot),
            bytes: snapshot.len() as u64,
        });
        let previous = if self.previous.is_some() {
            Some(crate::io::tracked_previous(
                &self.previous,
                &self.table,
                self.keep_factor,
                report,
            )?)
        } else {
            None
        };
        report.calendar = serde_json::to_value(calendar.metadata())?;
        let expected_date = calendar.market_trading_day_at(
            chrono::Utc::now()
                .with_timezone(&rustdx_complete::data::exchange_timezone())
                .naive_local(),
        )?;
        let mut rows = Vec::new();
        for mut row in data.data.diff.drain(..) {
            rustdx_complete::operation::check()?;
            if row.close.is_none() {
                report.files.push(crate::quality::FileReport {
                    source: "eastmoney".into(),
                    code: row.code,
                    rows: 0,
                    first_date: Some(row.date.clone()),
                    last_date: Some(row.date),
                    errors: vec![],
                    warnings: vec!["源无收盘数据，证券状态未知，未输出".into()],
                    errors_omitted: 0,
                    warnings_omitted: 0,
                });
                continue;
            }
            let validation: Result<()> = (|| {
                let canonical = row.to_canonical_with_calendar(calendar)?;
                let date = canonical.candle.bar.datetime.date();
                if date != expected_date {
                    eyre::bail!("{}快照日期{date}不是当前行情日{expected_date}", row.code);
                }
                if let Some(previous) = &previous {
                    let factor = previous
                        .get(&row.code)
                        .ok_or_else(|| eyre::eyre!("{}缺少前因子", row.code))?;
                    let prior = chrono::NaiveDate::parse_from_str(&factor.date, "%Y-%m-%d")?;
                    let expected = calendar.previous_trading_day(date)?;
                    if prior != expected {
                        eyre::bail!(
                            "{}前因子日期{prior}应为{expected}，快照不能跨日补全累计收益",
                            row.code
                        );
                    }
                    row.factor = canonical
                        .advance_return_index(rustdx_complete::data::ReturnIndex::new(
                            factor.factor,
                        )?)?
                        .value();
                }
                Ok(())
            })();
            let issue = crate::quality::FileReport {
                source: "eastmoney".into(),
                code: row.code.clone(),
                rows: usize::from(validation.is_ok()),
                first_date: Some(row.date.clone()),
                last_date: Some(row.date.clone()),
                errors: validation
                    .as_ref()
                    .err()
                    .map(|e| vec![e.to_string()])
                    .unwrap_or_default(),
                warnings: vec![],
                errors_omitted: 0,
                warnings_omitted: 0,
            };
            report.files.push(issue);
            validation?;
            report.record_rows(&row.code, 1, &row.date, &row.date)?;
            rows.push(row);
        }
        if rows.is_empty() {
            eyre::bail!("没有可发布的收盘日线");
        }
        let (atomic, file) = AtomicFile::create(self.output_path())?;
        if self.json {
            serde_json::to_writer_pretty(file, &rows)?;
        } else {
            let mut writer = csv::Writer::from_writer(file);
            for row in &rows {
                rustdx_complete::operation::check()?;
                writer.serialize(row)?;
            }
            writer.flush()?;
        }
        if !self.json {
            report.verify_csv(atomic.temporary_path())?;
        }
        report.stage_artifact(atomic.temporary_path())?;
        rustdx_complete::operation::check()?;
        atomic.commit()?;
        report.published()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_export_and_failed_validation_preserve_success() {
        let dir = std::env::temp_dir().join(format!("rustdx-east-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("east.json");
        let cmd = EastCmd::from_args(&["rustdx", "east"], &["-j", "-o", output.to_str().unwrap()])
            .unwrap();
        let date = rustdx_complete::calendar::TradingCalendar::market_trading_day();
        let timestamp = date
            .and_hms_opt(15, 1, 0)
            .unwrap()
            .and_local_timezone(rustdx_complete::data::exchange_timezone())
            .unwrap()
            .timestamp();
        let row = Day {
            date: date.to_string(),
            code: "sh600000".into(),
            market: Some(1),
            timestamp: Some(timestamp),
            open: Some(10.0),
            high: Some(11.0),
            low: Some(9.0),
            close: Some(10.0),
            amount: Some(1000.0),
            vol: Some(10.0),
            preclose: Some(9.0),
            factor: 0.0,
        };
        let data = EastMarket {
            data: EastData {
                total: 1,
                diff: vec![row],
            },
        };
        let mut report = crate::quality::CollectionReport::new(output.to_str().unwrap());
        cmd.process(data.clone(), &mut report).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result[0]["code"], "sh600000");
        let mut invalid = data;
        invalid.data.diff[0].open = Some(f64::NAN);
        assert!(cmd.process(invalid, &mut report).is_err());
        assert_eq!(std::fs::read(output).unwrap(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
