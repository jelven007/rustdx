use super::Gbbq;
use crate::file::day::Day;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Factor {
    pub date: String,
    pub code: String,
    #[serde(rename(deserialize = "close"))]
    pub preclose: f64,
    /// 从上市日起累计的收益指数，包含现金/送配股事件；不是价格乘数。
    pub factor: f64,
}

impl Factor {
    #[inline]
    pub fn compute_factor(&self, close: f64) -> f64 {
        self.factor * (close / self.preclose)
    }
}

#[derive(Debug, Clone)]
pub struct Fq {
    pub code: String,
    pub date: u32,
    pub factor: f64,
    pub close: f64,
    pub preclose: f64,
    pub trading: bool,
    pub xdxr: bool,
}

impl Fq {
    /// 首个交易日为基期（因子1），不应用基期及其之前的事件。
    pub fn new(
        days: impl ExactSizeIterator<Item = Day> + Clone,
        events: &[Gbbq],
    ) -> Option<Vec<Self>> {
        let first = days.clone().next()?;
        Self::build(days, events, first.close, 1.0, true)
    }

    /// 增量 events 必须限定为上次基准日期之后；停牌期间事件依次作用于理论前收。
    pub fn concat(
        days: impl ExactSizeIterator<Item = Day> + Clone,
        events: &[Gbbq],
        preclose: f64,
        factor: f64,
    ) -> Option<Vec<Self>> {
        Self::build(days, events, preclose, factor, false)
    }

    fn build(
        days: impl ExactSizeIterator<Item = Day>,
        events: &[Gbbq],
        mut preclose: f64,
        mut factor: f64,
        initial: bool,
    ) -> Option<Vec<Self>> {
        if !preclose.is_finite() || preclose <= 0.0 || !factor.is_finite() || factor <= 0.0 {
            return None;
        }
        let mut events: Vec<_> = events.iter().filter(|g| g.category == 1).collect();
        events.sort_by_key(|g| g.date);
        let mut events = events.into_iter().peekable();
        let mut result = Vec::with_capacity(days.len());
        let mut previous_date = None;
        for (i, day) in days.enumerate() {
            if !day.close.is_finite()
                || day.close <= 0.0
                || previous_date.is_some_and(|date| day.date <= date)
            {
                return None;
            }
            let mut applied = false;
            while let Some(event) = events.peek().filter(|g| g.date <= day.date) {
                if !initial || i != 0 {
                    preclose = event.compute_pre_pct(preclose, preclose, true)[0];
                    if !preclose.is_finite() || preclose <= 0.0 {
                        return None;
                    }
                    applied = true;
                }
                events.next();
            }
            factor *= day.close / preclose;
            if !factor.is_finite() {
                return None;
            }
            result.push(Self {
                code: day.code,
                date: day.date,
                factor,
                close: day.close,
                preclose,
                trading: true,
                xdxr: applied,
            });
            preclose = day.close;
            previous_date = Some(day.date);
        }
        Some(result)
    }

    pub fn no_gbbq(days: impl ExactSizeIterator<Item = Day> + Clone) -> Option<Vec<Self>> {
        Self::new(days, &[])
    }

    pub fn no_gbbq_with_previous(
        days: impl ExactSizeIterator<Item = Day> + Clone,
        preclose: f64,
        factor: f64,
    ) -> Option<Vec<Self>> {
        Self::concat(days, &[], preclose, factor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suspended_events_match_full_and_incremental_series() {
        let make_day = |date, close| Day {
            code: "sz000001".into(),
            date,
            open: close,
            high: close,
            low: close,
            close,
            amount: 100.0,
            vol: 100,
        };
        let days = vec![make_day(20250106, 10.0), make_day(20250110, 9.0)];
        let event = Gbbq {
            market: 0,
            code: "000001",
            date: 20250108,
            category: 1,
            fh_qltp: 10.0,
            pgj_qzgb: 0.0,
            sg_hltp: 0.0,
            pg_hzgb: 0.0,
        };
        let full = Fq::new(days.clone().into_iter(), std::slice::from_ref(&event)).unwrap();
        let incremental = Fq::concat(days[1..].iter().cloned(), &[event], 10.0, 1.0).unwrap();
        assert_eq!(full[1].preclose, 9.0);
        assert_eq!(full[1].factor, incremental[0].factor);
        assert_eq!(full[1].factor, 1.0);
    }
}
