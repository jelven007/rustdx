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
        if self
            .report
            .as_ref()
            .is_some_and(|p| p == std::path::Path::new(self.output_path()))
        {
            eyre::bail!("report不能与行情输出相同");
        }
        let mut report = crate::quality::CollectionReport::new(self.output_path());
        let result = fetch(self.max)
            .and_then(|data| self.process(data, &mut report))
            .and_then(|_| {
                if self.output == "clickhouse" {
                    crate::io::setup_clickhouse(true, &self.table)?;
                    crate::io::insert_clickhouse(&self.output, &self.table, false)?;
                }
                Ok(())
            });
        if let Err(e) = &result {
            report.errors.push(e.to_string());
        }
        if let Some(path) = &self.report {
            report.write(path, result.is_ok())?;
        }
        result
    }

    fn process(
        &self,
        mut data: EastMarket,
        report: &mut crate::quality::CollectionReport,
    ) -> Result<()> {
        let previous = if self.previous.is_some() {
            Some(crate::io::previous_csv_table(
                &self.previous,
                &self.table,
                self.keep_factor,
            )?)
        } else {
            None
        };
        let expected_date = rustdx_complete::calendar::TradingCalendar::market_trading_day();
        let mut rows = Vec::new();
        for mut row in data.data.diff.drain(..) {
            if row.close.is_none() {
                report.files.push(crate::quality::FileReport {
                    source: "eastmoney".into(),
                    code: row.code,
                    rows: 0,
                    first_date: Some(row.date.clone()),
                    last_date: Some(row.date),
                    errors: vec![],
                    warnings: vec!["源无收盘数据，证券状态未知，未输出".into()],
                });
                continue;
            }
            let validation: Result<()> = (|| {
                row.ensure_closed()?;
                let date = chrono::NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")?;
                if date != expected_date {
                    eyre::bail!("{}快照日期{date}不是当前行情日{expected_date}", row.code);
                }
                let field = |value: Option<f64>| {
                    value.ok_or_else(|| eyre::eyre!("{}行情字段缺失", row.code))
                };
                let (open, high, low, close, volume, amount, preclose) = (
                    field(row.open)?,
                    field(row.high)?,
                    field(row.low)?,
                    field(row.close)?,
                    field(row.vol)?,
                    field(row.amount)?,
                    field(row.preclose)?,
                );
                rustdx_complete::data::validate_ohlcv(open, high, low, close, volume, amount)?;
                if !preclose.is_finite() || preclose <= 0.0 {
                    eyre::bail!("前收非法");
                }
                if let Some(previous) = &previous {
                    let factor = previous
                        .get(&row.code)
                        .ok_or_else(|| eyre::eyre!("{}缺少前因子", row.code))?;
                    let prior = chrono::NaiveDate::parse_from_str(&factor.date, "%Y-%m-%d")?;
                    let expected =
                        rustdx_complete::calendar::TradingCalendar::previous_trading_day(&date);
                    if prior != expected {
                        eyre::bail!(
                            "{}前因子日期{prior}应为{expected}，快照不能跨日补全累计收益",
                            row.code
                        );
                    }
                    row.factor = close / preclose * factor.factor;
                    if !row.factor.is_finite() || row.factor <= 0.0 {
                        eyre::bail!("累计收益因子非法");
                    }
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
            };
            report.files.push(issue);
            validation?;
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
                writer.serialize(row)?;
            }
            writer.flush()?;
        }
        atomic.commit()?;
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
