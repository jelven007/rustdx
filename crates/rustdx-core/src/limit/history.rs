//! 公告支持的证券状态区间。缺口返回未知，不用当前名称推断历史。
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Board, ListingStage, RuleContext};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Announcement {
    pub url: String,
    /// 保存的公告原始字节摘要，便于维护者复核。
    pub sha256: String,
    pub published: NaiveDate,
    pub checked: NaiveDate,
}

impl Announcement {
    pub fn validate(&self) -> Result<(), HistoryError> {
        if !self.url.starts_with("https://")
            || self.url.len() <= 8
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || self.checked < self.published
        {
            return Err(HistoryError::Invalid("公告来源、摘要或核实日期非法"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingState {
    Trading,
    Suspended,
    Delisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityInterval {
    pub code: String,
    /// 闭区间；只声明实际核实的区间，不自动继承到未来。
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub board: Board,
    pub st: bool,
    pub stage: ListingStage,
    pub state: TradingState,
    pub announcement: Announcement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityHistoryDataset {
    pub version: String,
    pub checked_through: NaiveDate,
    pub intervals: Vec<SecurityInterval>,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("证券历史数据非法: {0}")]
    Invalid(&'static str),
    #[error("{0}状态区间重叠")]
    Overlap(String),
}

#[derive(Debug, Clone)]
pub struct SecurityHistory {
    dataset: SecurityHistoryDataset,
}

impl TryFrom<SecurityHistoryDataset> for SecurityHistory {
    type Error = HistoryError;

    fn try_from(mut dataset: SecurityHistoryDataset) -> Result<Self, Self::Error> {
        if dataset.version.trim().is_empty() {
            return Err(HistoryError::Invalid("版本不能为空"));
        }
        dataset
            .intervals
            .sort_by(|a, b| (&a.code, a.start).cmp(&(&b.code, b.start)));
        for interval in &dataset.intervals {
            let symbol = interval
                .code
                .parse::<crate::data::Symbol>()
                .map_err(|_| HistoryError::Invalid("证券代码非法"))?;
            if !symbol.is_a_share()
                || super::board_of(symbol.code()) != Some(interval.board)
                || interval.start > interval.end
                || interval.end > dataset.checked_through
                || interval.announcement.checked > dataset.checked_through
            {
                return Err(HistoryError::Invalid("证券、板块或覆盖区间非法"));
            }
            interval.announcement.validate()?;
            // IPO 序号是单日事实，不能作为一个多日区间的相同状态。
            if matches!(
                interval.stage,
                ListingStage::Ipo { .. } | ListingStage::AnnouncedUnrestricted
            ) && interval.start != interval.end
            {
                return Err(HistoryError::Invalid("上市阶段/无幅度限制公告须逐日声明"));
            }
            super::rule_at(RuleContext {
                date: interval.start,
                board: interval.board,
                st: interval.st,
                stage: interval.stage,
            })
            .map_err(|_| HistoryError::Invalid("不支持的规则或上市阶段"))?;
            super::rule_at(RuleContext {
                date: interval.end,
                board: interval.board,
                st: interval.st,
                stage: interval.stage,
            })
            .map_err(|_| HistoryError::Invalid("区间末日超出支持规则"))?;
        }
        for pair in dataset.intervals.windows(2) {
            if pair[0].code == pair[1].code && pair[1].start <= pair[0].end {
                return Err(HistoryError::Overlap(pair[0].code.clone()));
            }
        }
        Ok(Self { dataset })
    }
}

impl SecurityHistory {
    pub fn dataset(&self) -> &SecurityHistoryDataset {
        &self.dataset
    }

    /// 未覆盖、停牌或摘牌都不产生可计算的交易日规则。
    pub fn context_at(&self, code: &str, date: NaiveDate) -> Option<RuleContext> {
        let interval = self.state_at(code, date)?;
        (interval.state == TradingState::Trading).then_some(RuleContext {
            date,
            board: interval.board,
            st: interval.st,
            stage: interval.stage,
        })
    }

    pub fn state_at(&self, code: &str, date: NaiveDate) -> Option<&SecurityInterval> {
        let index = self
            .dataset
            .intervals
            .partition_point(|i| i.code.as_str() < code || (i.code == code && i.start <= date));
        let interval = self.dataset.intervals.get(index.checked_sub(1)?)?;
        (interval.code == code && date <= interval.end).then_some(interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_overlaps_and_single_day_stages_are_explicit() {
        let interval = SecurityInterval {
            code: "sh600000".into(),
            start: "2026-07-06".parse().unwrap(),
            end: "2026-07-07".parse().unwrap(),
            board: Board::Main,
            st: true,
            stage: ListingStage::Seasoned,
            state: TradingState::Trading,
            announcement: Announcement {
                url: "https://example.org/test-fixture".into(),
                sha256: "a".repeat(64),
                published: "2026-07-03".parse().unwrap(),
                checked: "2026-07-08".parse().unwrap(),
            },
        };
        let mut dataset = SecurityHistoryDataset {
            version: "test-fixture".into(),
            checked_through: "2026-07-08".parse().unwrap(),
            intervals: vec![interval.clone()],
        };
        let history = SecurityHistory::try_from(dataset.clone()).unwrap();
        assert!(
            history
                .state_at("sh600000", "2026-07-05".parse().unwrap())
                .is_none()
        );
        assert!(history.context_at("sh600000", interval.end).unwrap().st);
        assert!(
            history
                .state_at("sh600000", "2026-07-08".parse().unwrap())
                .is_none()
        );
        dataset.intervals.push(interval);
        assert!(matches!(
            SecurityHistory::try_from(dataset.clone()),
            Err(HistoryError::Overlap(_))
        ));
        dataset.intervals.pop();
        dataset.intervals[0].stage = ListingStage::Ipo {
            session: 1,
            registration: true,
        };
        assert!(SecurityHistory::try_from(dataset).is_err());
    }
}
