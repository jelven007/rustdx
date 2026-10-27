use rustdx_complete::{
    cache::{Cache, CacheFetchError}, calendar::TradingCalendar,
    file::{day::Day, gbbq::Fq}, operation::OperationContext,
    tcp::{stock::{Client, FinanceSnapshot}, HeartbeatAck},
};
use std::{io, time::Duration};
fn finance(client: &mut Client) -> io::Result<FinanceSnapshot> { client.finance(0, "000001") }
fn heartbeat(client: &mut Client) -> io::Result<HeartbeatAck> { client.heartbeat() }
fn main() {
    let date = "2025-01-06".parse().unwrap();
    assert!(TradingCalendar::is_trading_day(&date).unwrap());
    assert!(TradingCalendar::is_trading_day(&"2027-01-01".parse().unwrap()).is_err());
    let cache = Cache::memory(Duration::from_secs(1));
    let task = OperationContext::new(Duration::from_secs(1), None).unwrap();
    let value: Result<_, CacheFetchError<io::Error>> =
        cache.get_or_fetch_checked("key", &task, || Ok(vec![1]));
    assert_eq!(value.unwrap(), vec![1]);
    let day = Day { code: "sz000001".into(), date: 20250106,
        open: 10., high: 10., low: 10., close: 10., amount: 100., vol: 100 };
    assert_eq!(Fq::new_checked(vec![day].into_iter(), &[]).unwrap()[0].factor, 1.);
    let _ = (finance, heartbeat);
}
