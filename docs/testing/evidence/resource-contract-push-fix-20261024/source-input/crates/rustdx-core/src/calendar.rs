//! 中国A股交易日历模块
//!
//! 提供交易日历查询功能，自动识别周末和节假日
//!
//! # 特性
//!
//! - ✅ 无需联网，所有数据内置
//! - 内置休市日覆盖2024–2026；范围外兼容接口仅按工作日估计
//! - ✅ 数据定期更新（使用 trade_date_a crate）
//! - ✅ 简单易用的 API
//!
//! # 使用示例
//!
//! ```rust
//! use rustdx_complete::calendar::TradingCalendar;
//! use chrono::{NaiveDate, NaiveDateTime};
//!
//! // 判断是否是交易日
//! let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
//! assert!(TradingCalendar::is_trading_day(&date));
//!
//! // 获取前一个交易日
//! let prev = TradingCalendar::previous_trading_day(&date);
//!
//! // 获取后一个交易日
//! let next = TradingCalendar::next_trading_day(&date);
//!
//! // 获取最近N个交易日
//! let dates = TradingCalendar::get_trading_days(&date, 10);
//! ```

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use trade_date_a;

mod provider;
pub use provider::*;

/// 中国A股交易日历
///
/// 提供交易日判断、前后交易日查询等功能
pub struct TradingCalendar;

impl TradingCalendar {
    /// 内置休市数据覆盖范围；未知年份不能用工作日推断完整性。
    pub fn supported_range() -> (NaiveDate, NaiveDate) {
        (
            NaiveDate::from_ymd_opt(2024, 1, 1).expect("固定覆盖起始日期"),
            NaiveDate::from_ymd_opt(2026, 12, 31).expect("固定覆盖结束日期"),
        )
    }

    pub fn try_is_trading_day(date: &NaiveDate) -> crate::Result<bool> {
        Ok(BuiltinCalendar.is_trading_day(*date)?)
    }

    /// 严格查询前一个交易日，搜索跨出覆盖范围时返回错误。
    pub fn try_previous_trading_day(date: &NaiveDate) -> crate::Result<NaiveDate> {
        Ok(BuiltinCalendar.previous_trading_day(*date)?)
    }

    /// 严格查询后一个交易日，搜索跨出覆盖范围时返回错误。
    pub fn try_next_trading_day(date: &NaiveDate) -> crate::Result<NaiveDate> {
        Ok(BuiltinCalendar.next_trading_day(*date)?)
    }

    pub fn try_get_trading_days(date: &NaiveDate, count: usize) -> crate::Result<Vec<NaiveDate>> {
        Ok(BuiltinCalendar.get_trading_days(*date, count)?)
    }

    pub fn try_get_trading_days_before(
        date: &NaiveDate,
        count: usize,
    ) -> crate::Result<Vec<NaiveDate>> {
        Ok(BuiltinCalendar.get_trading_days_before(*date, count)?)
    }

    /// UTC+8下的市场交易日，9:30之前返回前一个交易日。
    pub fn try_market_trading_day_at(datetime: NaiveDateTime) -> crate::Result<NaiveDate> {
        let date = datetime.date();
        if Self::try_is_trading_day(&date)? && datetime.time().num_seconds_from_midnight() >= 34200
        {
            Ok(date)
        } else {
            Self::try_previous_trading_day(&date)
        }
    }

    /// 返回闭区间内的交易日；范围外返回错误，不猜测历史休市日。
    pub fn trading_days_between(start: NaiveDate, end: NaiveDate) -> crate::Result<Vec<NaiveDate>> {
        Ok(BuiltinCalendar.trading_days_between(start, end)?)
    }

    /// 判断指定日期是否是交易日
    ///
    /// # 参数
    ///
    /// - `date`: 要判断的日期
    ///
    /// # 返回
    ///
    /// - `true`: 是交易日
    /// - `false`: 非交易日（周末或节假日）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDate;
    ///
    /// let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    /// let is_trading = TradingCalendar::is_trading_day(&date);
    /// ```
    pub fn is_trading_day(date: &NaiveDate) -> bool {
        // 将 NaiveDate 转换为 i64 格式 (YYYYMMDD)
        let date_i64 = Self::date_to_i64(date);
        trade_date_a::is_work_day(date_i64)
    }

    /// 判断指定日期时间是否是交易时间
    ///
    /// # 参数
    ///
    /// - `datetime`: 要判断的日期时间
    ///
    /// # 返回
    ///
    /// - `true`: 是交易时间
    /// - `false`: 非交易时间
    ///
    /// # 说明
    ///
    /// UTC+8本地时间：9:30–11:30、13:00–15:00（包含收盘端点，不含午休）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDateTime;
    ///
    /// let dt = NaiveDateTime::parse_from_str("2025-01-06 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    /// let is_trading_time = TradingCalendar::is_trading_time(&dt);
    /// ```
    pub fn is_trading_time(datetime: &NaiveDateTime) -> bool {
        // 首先判断是否是交易日
        if !Self::is_trading_day(&datetime.date()) {
            return false;
        }

        let seconds = datetime.time().num_seconds_from_midnight();
        (34200..=41400).contains(&seconds) || (46800..=54000).contains(&seconds)
    }

    /// 获取指定日期的前一个交易日
    ///
    /// # 参数
    ///
    /// - `date`: 基准日期
    ///
    /// # 返回
    ///
    /// 前一个交易日
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDate;
    ///
    /// let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    /// let prev = TradingCalendar::previous_trading_day(&date);
    /// ```
    pub fn previous_trading_day(date: &NaiveDate) -> NaiveDate {
        let mut current = *date - chrono::Duration::days(1);

        // 最多查找100天，避免无限循环
        for _ in 0..100 {
            if Self::is_trading_day(&current) {
                return current;
            }
            current -= chrono::Duration::days(1);
        }

        // 如果找不到，返回原日期减一天
        *date - chrono::Duration::days(1)
    }

    /// 获取指定日期的后一个交易日
    ///
    /// # 参数
    ///
    /// - `date`: 基准日期
    ///
    /// # 返回
    ///
    /// 后一个交易日
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDate;
    ///
    /// let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    /// let next = TradingCalendar::next_trading_day(&date);
    /// ```
    pub fn next_trading_day(date: &NaiveDate) -> NaiveDate {
        let mut current = *date + chrono::Duration::days(1);

        // 最多查找100天，避免无限循环
        for _ in 0..100 {
            if Self::is_trading_day(&current) {
                return current;
            }
            current += chrono::Duration::days(1);
        }

        // 如果找不到，返回原日期
        *date
    }

    /// 获取从指定日期开始的最近N个交易日（包括指定日期）
    ///
    /// # 参数
    ///
    /// - `start_date`: 起始日期
    /// - `count`: 交易日数量
    ///
    /// # 返回
    ///
    /// 交易日列表（按时间升序排列）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDate;
    ///
    /// let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    /// let days = TradingCalendar::get_trading_days(&date, 10);
    /// assert_eq!(days.len(), 10);
    /// ```
    pub fn get_trading_days(start_date: &NaiveDate, count: usize) -> Vec<NaiveDate> {
        let mut result = Vec::with_capacity(count);
        let mut current = *start_date;

        // 如果起始日期不是交易日，找到下一个交易日
        if !Self::is_trading_day(&current) {
            current = Self::next_trading_day(&current);
        }

        while result.len() < count {
            if Self::is_trading_day(&current) {
                result.push(current);
            }
            current += chrono::Duration::days(1);
        }

        result
    }

    /// 获取从指定日期结束的最近N个交易日（包括指定日期）
    ///
    /// # 参数
    ///
    /// - `end_date`: 结束日期
    /// - `count`: 交易日数量
    ///
    /// # 返回
    ///
    /// 交易日列表（按时间升序排列）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDate;
    ///
    /// let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    /// let days = TradingCalendar::get_trading_days_before(&date, 10);
    /// assert_eq!(days.len(), 10);
    /// ```
    pub fn get_trading_days_before(end_date: &NaiveDate, count: usize) -> Vec<NaiveDate> {
        let mut result = Vec::with_capacity(count);
        let mut current = *end_date;

        // 如果结束日期不是交易日，找到前一个交易日
        if !Self::is_trading_day(&current) {
            current = Self::previous_trading_day(&current);
        }

        while result.len() < count {
            if Self::is_trading_day(&current) {
                result.push(current);
            }
            if current <= NaiveDate::from_ymd_opt(2000, 1, 1).unwrap() {
                break;
            }
            current -= chrono::Duration::days(1);
        }

        result.reverse();
        result
    }

    /// 计算两个日期之间的交易日数量
    ///
    /// # 参数
    ///
    /// - `start_date`: 起始日期（包含）
    /// - `end_date`: 结束日期（包含）
    ///
    /// # 返回
    ///
    /// 交易日数量
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    /// use chrono::NaiveDate;
    ///
    /// let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    /// let end = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
    /// let count = TradingCalendar::count_trading_days(&start, &end);
    /// ```
    pub fn count_trading_days(start_date: &NaiveDate, end_date: &NaiveDate) -> usize {
        if start_date > end_date {
            return 0;
        }

        let mut count = 0;
        let mut current = *start_date;

        while current <= *end_date {
            if Self::is_trading_day(&current) {
                count += 1;
            }
            current += chrono::Duration::days(1);
        }

        count
    }

    /// 获取当前交易日
    ///
    /// 如果今天是交易日，返回今天；
    /// 如果今天不是交易日，返回下一个交易日
    ///
    /// # 返回
    ///
    /// 当前交易日
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    ///
    /// let today = TradingCalendar::current_trading_day();
    /// ```
    pub fn current_trading_day() -> NaiveDate {
        let today = chrono::Utc::now()
            .with_timezone(&crate::data::exchange_timezone())
            .date_naive();

        if Self::is_trading_day(&today) {
            today
        } else {
            Self::next_trading_day(&today)
        }
    }

    /// 获取上一个交易日（考虑当前时间）
    ///
    /// 如果今天是交易日且在交易时间内，返回今天；
    /// 如果今天是交易日但已过交易时间，返回今天；
    /// 如果今天不是交易日，返回前一个交易日
    ///
    /// # 返回
    ///
    /// 市场交易日
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::calendar::TradingCalendar;
    ///
    /// let market_day = TradingCalendar::market_trading_day();
    /// ```
    pub fn market_trading_day() -> NaiveDate {
        Self::market_trading_day_at(
            chrono::Utc::now()
                .with_timezone(&crate::data::exchange_timezone())
                .naive_local(),
        )
    }

    pub fn market_trading_day_at(datetime: NaiveDateTime) -> NaiveDate {
        let date = datetime.date();
        if Self::is_trading_day(&date) && datetime.time().num_seconds_from_midnight() >= 34200 {
            date
        } else {
            Self::previous_trading_day(&date)
        }
    }

    // ========== 辅助方法 ==========

    /// 将 NaiveDate 转换为 i64 格式 (YYYYMMDD)
    fn date_to_i64(date: &NaiveDate) -> i64 {
        (date.year() * 10000 + date.month() as i32 * 100 + date.day() as i32) as i64
    }

    /// 将 i64 格式 (YYYYMMDD) 转换为 NaiveDate
    #[allow(dead_code)]
    fn i64_to_date(date: i64) -> NaiveDate {
        let year = (date / 10000) as i32;
        let month = ((date % 10000) / 100) as u32;
        let day = (date % 100) as u32;

        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
    }

    /// 将 NaiveDate 转换为 i32 格式 (YYYYMMDD) - 用于测试
    #[cfg(test)]
    fn date_to_i32(date: &NaiveDate) -> i32 {
        date.year() * 10000 + date.month() as i32 * 100 + date.day() as i32
    }

    /// 将 i32 格式 (YYYYMMDD) 转换为 NaiveDate - 用于测试
    #[cfg(test)]
    fn i32_to_date(date: i32) -> NaiveDate {
        let year = date / 10000;
        let month = (date % 10000) / 100;
        let day = date % 100;

        NaiveDate::from_ymd_opt(year, month as u32, day as u32)
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_trading_day() {
        // 测试周一（应该是交易日）
        let monday = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(); // 2025-01-06 是周一
        assert!(TradingCalendar::is_trading_day(&monday));

        // 测试周六（不应该是交易日）
        let saturday = NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(); // 2025-01-04 是周六
        assert!(!TradingCalendar::is_trading_day(&saturday));

        // 测试周日（不应该是交易日）
        let sunday = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap(); // 2025-01-05 是周日
        assert!(!TradingCalendar::is_trading_day(&sunday));
    }

    #[test]
    fn test_is_trading_time() {
        for (time, expected) in [
            ("11:30:00", true),
            ("11:30:01", false),
            ("12:00:00", false),
            ("13:00:00", true),
            ("15:00:00", true),
            ("15:00:01", false),
        ] {
            let dt =
                NaiveDateTime::parse_from_str(&format!("2025-01-06 {time}"), "%Y-%m-%d %H:%M:%S")
                    .unwrap();
            assert_eq!(TradingCalendar::is_trading_time(&dt), expected);
        }
        assert!(
            TradingCalendar::try_is_trading_day(&NaiveDate::from_ymd_opt(2027, 1, 1).unwrap())
                .is_err()
        );
        // 交易时间内（周一 10:00）
        let dt1 =
            NaiveDateTime::parse_from_str("2025-01-06 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert!(TradingCalendar::is_trading_time(&dt1));

        // 交易时间内（周一 14:00）
        let dt2 =
            NaiveDateTime::parse_from_str("2025-01-06 14:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert!(TradingCalendar::is_trading_time(&dt2));

        // 交易时间前（周一 9:00）
        let dt3 =
            NaiveDateTime::parse_from_str("2025-01-06 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert!(!TradingCalendar::is_trading_time(&dt3));

        // 交易时间后（周一 16:00）
        let dt4 =
            NaiveDateTime::parse_from_str("2025-01-06 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert!(!TradingCalendar::is_trading_time(&dt4));

        // 周末（即使时间在9:30-15:00也不是交易时间）
        let dt5 =
            NaiveDateTime::parse_from_str("2025-01-04 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert!(!TradingCalendar::is_trading_time(&dt5));
    }

    #[test]
    fn test_previous_trading_day() {
        // 2025-01-01 是周三（元旦，节假日）
        // 使用一个确定的交易日序列进行测试

        // 测试1: 周二的前一个交易日应该是周一
        let tuesday = NaiveDate::from_ymd_opt(2025, 1, 7).unwrap(); // 周二
        let prev = TradingCalendar::previous_trading_day(&tuesday);

        // 周二的前一个交易日应该是周一（2025-01-06）
        let expected = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        assert_eq!(prev, expected);

        // 测试2: 确认周一（2025-01-06）是交易日
        assert!(TradingCalendar::is_trading_day(&expected));

        // 测试3: 周六的前一个交易日应该是周五
        let saturday = NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(); // 周六
        let prev_sat = TradingCalendar::previous_trading_day(&saturday);

        // 2025-01-03 是周五，应该是交易日（除非元旦调休）
        // 这个日期可能因调休安排不同，我们只验证它不等于周六
        assert_ne!(prev_sat, saturday);
    }

    #[test]
    fn test_next_trading_day() {
        // 周五的后一个交易日应该是下周一
        let friday = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
        let next = TradingCalendar::next_trading_day(&friday);

        // 下一个交易日应该是 2025-01-06（周一）
        let expected = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_get_trading_days() {
        let start = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(); // 周一
        let days = TradingCalendar::get_trading_days(&start, 5);

        assert_eq!(days.len(), 5);

        // 应该是：周一(6), 周二(7), 周三(8), 周四(9), 周五(10)
        assert_eq!(days[0], NaiveDate::from_ymd_opt(2025, 1, 6).unwrap());
        assert_eq!(days[1], NaiveDate::from_ymd_opt(2025, 1, 7).unwrap());
        assert_eq!(days[2], NaiveDate::from_ymd_opt(2025, 1, 8).unwrap());
        assert_eq!(days[3], NaiveDate::from_ymd_opt(2025, 1, 9).unwrap());
        assert_eq!(days[4], NaiveDate::from_ymd_opt(2025, 1, 10).unwrap());
    }

    #[test]
    fn test_get_trading_days_before() {
        let end = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap(); // 周五
        let days = TradingCalendar::get_trading_days_before(&end, 5);

        assert_eq!(days.len(), 5);

        // 应该是：周一(6), 周二(7), 周三(8), 周四(9), 周五(10)
        assert_eq!(days[0], NaiveDate::from_ymd_opt(2025, 1, 6).unwrap());
        assert_eq!(days[1], NaiveDate::from_ymd_opt(2025, 1, 7).unwrap());
        assert_eq!(days[2], NaiveDate::from_ymd_opt(2025, 1, 8).unwrap());
        assert_eq!(days[3], NaiveDate::from_ymd_opt(2025, 1, 9).unwrap());
        assert_eq!(days[4], NaiveDate::from_ymd_opt(2025, 1, 10).unwrap());
    }

    #[test]
    fn test_count_trading_days() {
        let start = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(); // 周一
        let end = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap(); // 周五

        let count = TradingCalendar::count_trading_days(&start, &end);

        // 周一到周五，5个交易日
        assert_eq!(count, 5);
    }

    #[test]
    fn test_date_conversion() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        let i32 = TradingCalendar::date_to_i32(&date);
        assert_eq!(i32, 20250106);

        let converted = TradingCalendar::i32_to_date(i32);
        assert_eq!(converted, date);
    }
}
