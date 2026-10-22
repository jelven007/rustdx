use rustdx_complete::calendar::{
    BuiltinCalendar, CalendarDataset, CalendarProvider, HolidayCalendar,
};

#[test]
fn exchange_2026_datasets_match_each_other_and_builtin_every_day() {
    let read = |text| {
        HolidayCalendar::try_from(serde_json::from_str::<CalendarDataset>(text).unwrap()).unwrap()
    };
    let a = read(include_str!("../../data/calendars/sse-2026.json"));
    let b = read(include_str!("../../data/calendars/szse-2026.json"));
    let mut date = a.metadata().start;
    let mut sessions = 0;
    loop {
        let trading = a.is_trading_day(date).unwrap();
        assert_eq!(trading, b.is_trading_day(date).unwrap(), "{date}");
        assert_eq!(
            trading,
            BuiltinCalendar.is_trading_day(date).unwrap(),
            "{date}"
        );
        sessions += usize::from(trading);
        if date == a.metadata().end {
            break;
        }
        date = date.succ_opt().unwrap();
    }
    assert_eq!(sessions, 242);
}
