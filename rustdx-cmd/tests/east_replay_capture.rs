use rustdx_cli::eastmoney::{RawPage, replay};
use rustdx_complete::calendar::BuiltinCalendar;

#[test]
fn captured_http_body_replays_to_independently_retained_csv() {
    let page: RawPage =
        serde_json::from_str(include_str!("fixtures/east-raw-20261023.json")).unwrap();
    assert!(page.body.contains("\"f124\":"));
    assert!(page.body.contains("\"f13\":"));
    let data = replay(page.max, [Ok(page)], &BuiltinCalendar).unwrap();
    assert_eq!(data.data.total, 5563);
    let mut writer = csv::Writer::from_writer(Vec::new());
    for row in data.data.diff {
        row.to_canonical().unwrap();
        writer.serialize(row).unwrap();
    }
    assert_eq!(
        writer.into_inner().unwrap(),
        include_bytes!("fixtures/east-20261022.csv")
    );
}
