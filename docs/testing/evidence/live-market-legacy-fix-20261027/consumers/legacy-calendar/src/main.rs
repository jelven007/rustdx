use rustdx_complete::calendar::TradingCalendar;
fn main() {
    let date = "2025-01-06".parse().unwrap();
    let _: bool = TradingCalendar::is_trading_day(&date);
}
