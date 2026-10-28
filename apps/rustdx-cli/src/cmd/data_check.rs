//! 数据更新先交叉核对，再原子发布；不在线猜测休市或证券历史。
use std::path::PathBuf;

use argh::FromArgs;
use eyre::Result;
use rustdx_complete::{
    atomic_file::AtomicFile,
    calendar::{CalendarDataset, CalendarProvider, HolidayCalendar},
    limit::{SecurityHistory, SecurityHistoryDataset},
};

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "data-check")]
/// 校验日历/证券历史更新及保存的来源证据。
pub struct DataCheckCmd {
    /// calendar或security。
    #[argh(positional)]
    kind: String,
    /// 待校验JSON。
    #[argh(positional)]
    input: PathBuf,
    /// 日历必需：独立整理、同覆盖范围的参考日历JSON。
    #[argh(option)]
    reference: Option<PathBuf>,
    /// 证券历史必需：公告原始文件目录，以SHA256值为文件名。
    #[argh(option)]
    evidence_dir: Option<PathBuf>,
    /// 可选：校验通过后原子发布到此路径。
    #[argh(option)]
    output: Option<PathBuf>,
    /// 仅校验证券状态；不扩大已核实的涨跌幅规则范围。
    #[argh(switch)]
    state_only: bool,
}

fn compare_calendars(a: &dyn CalendarProvider, b: &dyn CalendarProvider) -> Result<usize> {
    let left = a.metadata();
    let right = b.metadata();
    if left.start != right.start || left.end != right.end {
        eyre::bail!("参考日历覆盖范围须完全一致");
    }
    let mut date = left.start;
    let mut sessions = 0;
    loop {
        let trading = a.is_trading_day(date)?;
        if trading != b.is_trading_day(date)? {
            eyre::bail!("日历交叉核对不一致: {date}");
        }
        sessions += usize::from(trading);
        if date == left.end {
            break;
        }
        date = date.succ_opt().ok_or_else(|| eyre::eyre!("日期溢出"))?;
    }
    if sessions == 0 {
        eyre::bail!("日历覆盖范围没有交易日");
    }
    Ok(sessions)
}

impl DataCheckCmd {
    pub fn run(&self) -> Result<()> {
        let (_, bytes) = crate::quality::source_snapshot(&self.input, 16 * 1024 * 1024)?;
        let mut inputs = vec![self.input.clone()];
        inputs.extend(self.reference.iter().cloned());
        match self.kind.as_str() {
            "calendar" => {
                if self.state_only {
                    eyre::bail!("state-only仅适用于security");
                }
                let reference = self
                    .reference
                    .as_ref()
                    .ok_or_else(|| eyre::eyre!("日历更新需要独立reference"))?;
                let (_, ref_bytes) = crate::quality::source_snapshot(reference, 1024 * 1024)?;
                if bytes == ref_bytes {
                    eyre::bail!("参考数据不能与待更新文件完全相同");
                }
                let calendar =
                    HolidayCalendar::try_from(serde_json::from_slice::<CalendarDataset>(&bytes)?)?;
                let other = HolidayCalendar::try_from(serde_json::from_slice::<CalendarDataset>(
                    &ref_bytes,
                )?)?;
                if calendar.metadata().source == other.metadata().source {
                    eyre::bail!("交叉核对须声明独立来源");
                }
                let sessions = compare_calendars(&calendar, &other)?;
                println!("逐日交叉核对通过：{sessions}个交易日；来源完整性仍由维护者核实");
            }
            "security" => {
                let dataset = serde_json::from_slice::<SecurityHistoryDataset>(&bytes)?;
                let history = if self.state_only {
                    SecurityHistory::from_state_dataset(dataset)?
                } else {
                    SecurityHistory::try_from(dataset)?
                };
                let directory = self
                    .evidence_dir
                    .as_ref()
                    .ok_or_else(|| eyre::eyre!("证券状态需要evidence-dir公告原始字节"))?;
                for interval in &history.dataset().intervals {
                    let digest = interval.announcement.sha256.to_ascii_lowercase();
                    inputs.push(directory.join(&digest));
                    let (source, _) = crate::quality::source_snapshot(
                        &directory.join(&digest),
                        16 * 1024 * 1024,
                    )?;
                    if source.sha256 != digest {
                        eyre::bail!("公告摘要不一致: {}", interval.code);
                    }
                }
                println!(
                    "状态区间及公告摘要核验通过：{}条；公告含义由维护者复核",
                    history.dataset().intervals.len()
                );
            }
            _ => eyre::bail!("kind必须为calendar或security"),
        }
        if let Some(output) = &self.output {
            crate::paths::validate(std::slice::from_ref(output), &inputs)?;
            let _lock = crate::paths::OutputLocks::acquire(std::slice::from_ref(output))?;
            let (atomic, mut file) = AtomicFile::create(output)?;
            std::io::Write::write_all(&mut file, &bytes)?;
            drop(file);
            atomic.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_disagreement_and_coverage_are_rejected() {
        let dataset = CalendarDataset {
            version: "test".into(),
            source: "fixture".into(),
            start: "2026-01-01".parse().unwrap(),
            end: "2026-01-05".parse().unwrap(),
            holidays: vec!["2026-01-01".parse().unwrap(), "2026-01-02".parse().unwrap()],
        };
        let a = HolidayCalendar::try_from(dataset.clone()).unwrap();
        let mut other = dataset;
        other.holidays.pop();
        let b = HolidayCalendar::try_from(other.clone()).unwrap();
        assert!(compare_calendars(&a, &b).is_err());
        other.end = "2026-01-06".parse().unwrap();
        assert!(compare_calendars(&a, &HolidayCalendar::try_from(other).unwrap()).is_err());
        assert_eq!(compare_calendars(&a, &a).unwrap(), 1);
    }
}
