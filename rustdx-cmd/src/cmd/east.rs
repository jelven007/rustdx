use argh::FromArgs;
use eyre::Result;
use rustdx_cli::eastmoney::*;

/// 东方财富当日 A 股数据。多数情况下使用 `rustdx east -p factor.csv` 即可。
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "east")]
pub struct EastCmd {
    /// 保存数据的 CSV 路径文件名。默认为当前路径下 eastmoney.csv 文件。
    #[argh(option, short = 'o', default = r#""eastmoney.csv".into()"#)]
    pub output: String,

    /// 指定前一日复权 csv 文件。
    #[argh(option, short = 'p')]
    pub previous: Option<std::path::PathBuf>,

    /// debug 版本时，为了通过断言测试而手动忽略掉前收价格或复权因子计算不一致的股票。
    /// 这种不一致的原因：浮点误差、或者当天为除权日。当第一个元素为 `all` 时，直接通过测试。
    /// release 版本无需此选项。
    #[argh(option, short = 'i')]
    pub ignore: Vec<String>,

    /// 可选。指定时，表示保存 factor.csv 文件。
    #[argh(switch)]
    pub keep_factor: bool,

    /// 股票总个数，默认全部。
    #[argh(option)]
    pub max: Option<u16>,

    /// output 以 json 格式显示
    #[argh(switch, short = 'j')]
    pub json: bool,

    /// 可选。指定表名称，默认为 `rustdx.tmp`。
    #[argh(option, short = 't', default = "String::from(\"rustdx.tmp\")")]
    pub table: String,
}

impl EastCmd {
    /// 注意：即使没有提供前一天的 factor 数据，
    /// 产生的 csv 文件依然会有 factor 一列，但数据是 0.
    pub fn run_no_previous(&self) -> Result<()> {
        let data = fetch(self.max)?;

        {
            let file = std::fs::File::create(&self.output)?;
            let mut wrt = csv::Writer::from_writer(file);
            for row in &data.data.diff {
                if row.close.is_some() {
                    row.ensure_closed()?;
                    wrt.serialize(row)?;
                }
            }
            wrt.flush()?;
        }
        self.insert_clickhouse()
    }

    pub fn run_previous(&self) -> Result<()> {
        let data = fetch(self.max)?;
        self._run_previous(data)?;
        self.insert_clickhouse()
    }

    fn _run_previous(&self, mut data: EastMarket) -> Result<()> {
        let previous =
            crate::io::previous_csv_table(&self.previous, &self.table, self.keep_factor)?;
        let file = std::fs::File::create(&self.output)?;
        let mut wrt = csv::Writer::from_writer(file);
        for row in &mut data.data.diff {
            // 排除掉无数据的股票：停牌、未上市之类
            if let (&Some(c), &Some(p)) = (&row.close, &row.preclose) {
                // 东方财富返回的是 6 位裸代码，需拼上市场前缀才能匹配 factor（含市场前缀）
                row.ensure_closed()?;
                if p <= 0.0 || c <= 0.0 {
                    eyre::bail!("{}价格非法", row.code);
                }
                if let Some(f) = previous.get(&row.code) {
                    if f.date >= row.date {
                        eyre::bail!("{}前因子日期必须早于增量日期", row.code);
                    }
                    row.factor = c / p * f.factor;
                } else {
                    eyre::bail!("{}缺少前因子，无法保证增量连续性", row.code);
                }
                wrt.serialize(row)?;
            }
        }
        wrt.flush()?;
        Ok(())
    }

    pub fn run(&self) -> Result<()> {
        if self.previous.is_some() {
            self.run_previous()
        } else {
            self.run_no_previous()
        }
    }

    fn insert_clickhouse(&self) -> Result<()> {
        if self.output.eq("clickhouse") {
            crate::io::setup_clickhouse(true, &self.table)?;
            // 插入 clickhouse 时，不保存解析结果。
            // 如果需要保存结果，则使用 `rustdx east -o output.csv` 指定 csv 文件。
            crate::io::insert_clickhouse(&self.output, &self.table, false)
        } else {
            Ok(())
        }
    }
}
