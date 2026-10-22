use argh::FromArgs;
use eyre::Result;
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::{BuiltinCalendar, CalendarDataset, CalendarProvider, HolidayCalendar},
    limit::{SecurityHistory, SecurityHistoryDataset},
};
use std::path::PathBuf;

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "data-status")]
/// 输出日历/规则核实范围与证券历史覆盖告警JSON。
pub struct DataStatusCmd {
    /// 核对日期YYYY-MM-DD，默认UTC+8今日。
    #[argh(option)]
    as_of: Option<chrono::NaiveDate>,
    /// 提前告警自然日数，默认90。
    #[argh(option, default = "90")]
    warn_days: u16,
    /// 外部日历JSON，未指定时检查内置日历。
    #[argh(option)]
    calendar: Option<PathBuf>,
    /// 证券历史JSON；缺失时明确标记missing。
    #[argh(option)]
    security: Option<PathBuf>,
    /// 可选JSON报告文件，原子发布。
    #[argh(option)]
    report: Option<PathBuf>,
    /// 任一due/unverified/missing/incomplete时返回非零，报告仍输出。
    #[argh(switch)]
    strict: bool,
}

impl DataStatusCmd {
    pub fn run(&self) -> Result<()> {
        let inputs = self
            .calendar
            .iter()
            .chain(self.security.iter())
            .cloned()
            .collect::<Vec<_>>();
        let outputs = self.report.iter().cloned().collect::<Vec<_>>();
        crate::paths::validate(&outputs, &inputs)?;
        let _locks = crate::paths::OutputLocks::acquire(&outputs)?;
        let mut sources = Vec::new();
        let mut read = |path: &std::path::Path| -> Result<Vec<u8>> {
            let (evidence, bytes) = crate::quality::source_snapshot(path, 16 * 1024 * 1024)?;
            sources.push(evidence);
            Ok(bytes)
        };
        let calendar: Box<dyn CalendarProvider> = match &self.calendar {
            Some(path) => Box::new(HolidayCalendar::try_from(serde_json::from_slice::<
                CalendarDataset,
            >(&read(path)?)?)?),
            None => Box::new(BuiltinCalendar),
        };
        let security = self
            .security
            .as_ref()
            .map(|path| -> Result<_> {
                Ok(SecurityHistory::try_from(serde_json::from_slice::<
                    SecurityHistoryDataset,
                >(&read(path)?)?)?)
            })
            .transpose()?;
        let as_of = self.as_of.unwrap_or_else(|| {
            chrono::Utc::now()
                .with_timezone(&rustdx_complete::data::exchange_timezone())
                .date_naive()
        });
        let coverage = rustdx_cli::maintenance::coverage(
            calendar.as_ref(),
            security.as_ref(),
            as_of,
            self.warn_days,
        );
        let attention = coverage.iter().any(|item| item.status != "covered");
        let value = serde_json::json!({
            "schema_version": 1, "as_of": as_of, "warn_days": self.warn_days,
            "attention_required": attention, "coverage": coverage, "sources": sources,
            "update_command": "rustdx data-check calendar|security <candidate> ..."
        });
        let bytes = serde_json::to_vec_pretty(&value)?;
        if let Some(path) = &self.report {
            let (atomic, mut file) = AtomicFile::create(path)?;
            std::io::Write::write_all(&mut file, &bytes)?;
            drop(file);
            atomic.commit()?;
        }
        println!("{}", String::from_utf8(bytes)?);
        if attention && self.strict {
            eyre::bail!("数据覆盖需要维护；请按data-check流程核实更新，不能仅延长截止日期");
        }
        Ok(())
    }
}
