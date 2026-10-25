//! Validate frozen calendar/history/universe and enumerate exact expected keys.
use std::path::Path;

use chrono::NaiveDate;
use rustdx_complete::{
    calendar::{CalendarDataset, CalendarProvider, HolidayCalendar},
    limit::{SecurityHistory, SecurityHistoryDataset, TradingState},
};

use crate::{
    Result,
    config::{Config, MAX_JSON, Universe},
    invalid, storage,
};

pub(super) struct Facts {
    pub(super) calendar: HolidayCalendar,
    pub(super) history: SecurityHistory,
    pub(super) universe: Universe,
}

pub(super) fn load(config: &Config, directory: &Path) -> Result<Facts> {
    load_paths(
        config,
        &directory.join("calendar.json"),
        &directory.join("history.json"),
        &directory.join("universe.json"),
    )
}

pub(super) fn load_config(config: &Config) -> Result<Facts> {
    load_paths(
        config,
        &config.calendar,
        &config.security_history,
        &config.universe,
    )
}

fn load_paths(
    config: &Config,
    calendar_path: &Path,
    history_path: &Path,
    universe_path: &Path,
) -> Result<Facts> {
    let calendar: CalendarDataset =
        serde_json::from_slice(&storage::read_limited(calendar_path, MAX_JSON)?)?;
    let calendar = HolidayCalendar::try_from(calendar).map_err(|e| invalid(e.to_string()))?;
    calendar
        .check_range(config.from)
        .map_err(|e| invalid(e.to_string()))?;
    calendar
        .check_range(config.through)
        .map_err(|e| invalid(e.to_string()))?;
    let history: SecurityHistoryDataset =
        serde_json::from_slice(&storage::read_limited(history_path, MAX_JSON)?)?;
    if history.checked_through < config.through || history.checked_through > config.planned_date {
        return Err(invalid(
            "security history verification cutoff does not cover planned range",
        ));
    }
    let history = SecurityHistory::try_from(history).map_err(|e| invalid(e.to_string()))?;
    let universe: Universe =
        serde_json::from_slice(&storage::read_limited(universe_path, MAX_JSON)?)?;
    universe.validate(config)?;
    Ok(Facts {
        calendar,
        history,
        universe,
    })
}

pub(super) fn visit_expected(
    config: &Config,
    facts: &Facts,
    mut visitor: impl FnMut(NaiveDate, &str, TradingState) -> Result<()>,
) -> Result<()> {
    let mut count = 0_u64;
    for date in facts
        .calendar
        .trading_days_between(config.from, config.through)
        .map_err(|e| invalid(e.to_string()))?
    {
        for member in &facts.universe.members {
            super::check()?;
            if date < member.from || date > member.through {
                continue;
            }
            count += 1;
            if count > config.max_rows {
                return Err(invalid("expected coverage exceeds row budget"));
            }
            let state = facts.history.state_at(&member.code, date).ok_or_else(|| {
                invalid(format!("Unknown security state: {},{date}", member.code))
            })?;
            visitor(date, &member.code, state.state)?;
        }
    }
    if count == 0 {
        return Err(invalid("empty expected scope is not a complete dataset"));
    }
    Ok(())
}
