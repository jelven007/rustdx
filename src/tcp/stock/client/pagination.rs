//! Shared stock/index history pagination with bounded protocol offsets.
use super::{DateTime, validate_bar_dates};
use std::io;

const PAGE: u16 = 800;
const MAX_PAGES: usize = 82;

pub(super) fn collect<T>(
    begin: Option<u32>,
    end: Option<u32>,
    mut fetch: impl FnMut(u16, u16) -> io::Result<Vec<T>>,
    datetime: impl Fn(&T) -> DateTime,
) -> io::Result<Vec<T>> {
    let mut all = Vec::new();
    let mut start = 0u16;
    let mut previous_oldest = None;
    for page in 1..=MAX_PAGES {
        let bars = fetch(start, PAGE)?;
        validate_bar_dates(bars.iter().map(&datetime))?;
        let count = bars.len();
        if count > PAGE as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "K线页超过请求数量",
            ));
        }
        let oldest = bars.iter().map(&datetime).min();
        if count > 0 && previous_oldest.is_some_and(|prev| oldest >= Some(prev)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "K线分页未向历史方向推进",
            ));
        }
        previous_oldest = oldest;
        all.extend(bars);
        if count < PAGE as usize
            || oldest.is_some_and(|dt| begin.is_some_and(|b| DateTime::to_u32(dt) < b))
        {
            break;
        }
        if page == MAX_PAGES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "K线分页超过协议偏移上限",
            ));
        }
        start = start
            .checked_add(PAGE)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "K线分页偏移溢出"))?;
    }
    all.sort_by_key(&datetime);
    all.dedup_by_key(|bar| datetime(bar));
    all.retain(|bar| {
        let date = DateTime::to_u32(datetime(bar));
        begin.is_none_or(|b| date >= b) && end.is_none_or(|e| date <= e)
    });
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dt(day: u16) -> DateTime {
        DateTime {
            year: 2025,
            month: 1,
            day,
            hour: 0,
            minute: 0,
        }
    }

    #[test]
    fn pages_deduplicate_filter_and_require_history_progress() {
        let mut calls = 0;
        let values = collect(
            Some(20250106),
            Some(20250108),
            |start, count| {
                calls += 1;
                assert_eq!(count, 800);
                Ok(if start == 0 {
                    vec![dt(8); 800]
                } else {
                    vec![dt(8), dt(7), dt(6), dt(3)]
                })
            },
            |dt| *dt,
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(values, vec![dt(6), dt(7), dt(8)]);
        assert!(collect(None, None, |_, _| Ok(vec![dt(8); 800]), |dt| *dt).is_err());
        assert!(collect(None, None, |_, _| Ok(vec![dt(8); 801]), |dt| *dt).is_err());
        assert!(collect(None, None, |_, _| Ok(vec![dt(32)]), |dt| *dt).is_err());
    }
}
