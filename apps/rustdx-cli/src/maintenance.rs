//! Evidence coverage, not an inference that an exchange rule has expired.
use chrono::NaiveDate;
use rustdx_complete::{
    calendar::CalendarProvider,
    limit::{RULE_CHECKED_THROUGH, RULE_VERSION, SecurityHistory},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Coverage {
    pub dataset: &'static str,
    pub version: Option<String>,
    pub through: Option<NaiveDate>,
    pub remaining_days: Option<i64>,
    pub status: &'static str,
    pub scope: &'static str,
    pub symbols: Option<usize>,
    pub symbols_with_state: Option<usize>,
}

fn horizon(
    dataset: &'static str,
    version: &str,
    end: NaiveDate,
    as_of: NaiveDate,
    warn_days: u16,
    scope: &'static str,
) -> Coverage {
    let days = (end - as_of).num_days();
    Coverage {
        dataset,
        version: Some(version.into()),
        through: Some(end),
        remaining_days: Some(days),
        status: if days < 0 {
            "unverified"
        } else if days <= i64::from(warn_days) {
            "due"
        } else {
            "covered"
        },
        scope,
        symbols: None,
        symbols_with_state: None,
    }
}

pub fn coverage(
    calendar: &dyn CalendarProvider,
    security: Option<&SecurityHistory>,
    as_of: NaiveDate,
    warn_days: u16,
) -> Vec<Coverage> {
    let meta = calendar.metadata();
    let mut cal = horizon(
        "calendar",
        meta.version,
        meta.end,
        as_of,
        warn_days,
        "declared_range",
    );
    if as_of < meta.start {
        cal.status = "unverified";
    }
    let rules = horizon(
        "rules",
        RULE_VERSION,
        RULE_CHECKED_THROUGH
            .parse()
            .expect("checked-in ISO rule review date"),
        as_of,
        warn_days,
        "review_horizon_not_rule_expiration",
    );
    let states = match security {
        None => Coverage {
            dataset: "security",
            version: None,
            through: None,
            remaining_days: None,
            status: "missing",
            scope: "no_market_coverage_claim",
            symbols: None,
            symbols_with_state: None,
        },
        Some(history) => {
            let data = history.dataset();
            let codes = data
                .intervals
                .iter()
                .map(|i| i.code.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let known = codes
                .iter()
                .filter(|code| history.state_at(code, as_of).is_some())
                .count();
            let mut item = horizon(
                "security",
                &data.version,
                data.checked_through,
                as_of,
                warn_days,
                "only_symbols_in_dataset_not_full_market",
            );
            item.symbols = Some(codes.len());
            item.symbols_with_state = Some(known);
            if known != codes.len() || known == 0 {
                item.status = "incomplete";
            }
            item
        }
    };
    vec![cal, rules, states]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustdx_complete::calendar::BuiltinCalendar;

    #[test]
    fn inclusive_warning_and_coverage_boundaries_do_not_extend_evidence() {
        for (date, status) in [
            ("2026-09-30", "covered"),
            ("2026-10-02", "due"),
            ("2026-12-31", "due"),
            ("2027-01-01", "unverified"),
            ("2023-12-31", "unverified"),
        ] {
            let report = coverage(&BuiltinCalendar, None, date.parse().unwrap(), 90);
            assert_eq!(report[0].status, status, "{date}");
            assert_eq!(report[2].status, "missing");
        }
        let report = coverage(&BuiltinCalendar, None, "2026-10-23".parse().unwrap(), 0);
        assert_eq!(report[1].status, "unverified");
        assert_eq!(report[1].remaining_days, Some(-1));
        let empty = SecurityHistory::try_from(rustdx_complete::limit::SecurityHistoryDataset {
            version: "empty".into(),
            checked_through: "2026-10-22".parse().unwrap(),
            intervals: vec![],
        })
        .unwrap();
        assert_eq!(
            coverage(
                &BuiltinCalendar,
                Some(&empty),
                "2026-10-22".parse().unwrap(),
                0
            )[2]
            .status,
            "incomplete"
        );
    }
}
