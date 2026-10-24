//! Frozen post-fix wire bytes against byte-identical official .day records.
use rustdx_complete::tcp::{
    decode_response,
    stock::{DailyBarsRequest, decode_daily_bars},
};
use std::{fs, path::Path};

#[test]
fn listed_daily_wire_matches_all_official_fields_on_two_endpoints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/daily-shares");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut matched = 0;
    for entry in manifest["entries"].as_array().unwrap() {
        let name = entry["name"].as_str().unwrap();
        let read = |suffix| fs::read(root.join(format!("{name}.{suffix}"))).unwrap();
        let req = read("request.bin");
        let header: [u8; 16] = read("header.bin").try_into().unwrap();
        let body = decode_response(&req, &header, &read("wire.bin"), true).unwrap();
        let market = u16::from_le_bytes(req[12..14].try_into().unwrap());
        let code = std::str::from_utf8(&req[14..20]).unwrap();
        let start = u16::from_le_bytes(req[20..22].try_into().unwrap());
        let count = u16::from_le_bytes(req[22..24].try_into().unwrap());
        let page = decode_daily_bars(
            &DailyBarsRequest::new(market, code, start, count).unwrap(),
            &body,
        )
        .unwrap();
        let expected = read("official.day");
        assert_eq!(expected.len(), 4 * 32);
        for row in expected.chunks_exact(32) {
            let integer = |offset| u32::from_le_bytes(row[offset..offset + 4].try_into().unwrap());
            let bar = page
                .records
                .iter()
                .find(|b| b.date_raw == integer(0))
                .unwrap();
            assert_eq!(bar.open_price_milli, i64::from(integer(4)) * 10);
            assert_eq!(bar.high_price_milli, i64::from(integer(8)) * 10);
            assert_eq!(bar.low_price_milli, i64::from(integer(12)) * 10);
            assert_eq!(bar.close_price_milli, i64::from(integer(16)) * 10);
            assert_eq!(bar.amount_raw, integer(20));
            assert_eq!(bar.amount, f64::from(f32::from_bits(integer(20))));
            assert_eq!(bar.volume_shares, integer(24));
            assert_ne!(bar.volume_shares % 100, 0);
            assert_eq!(bar.volume_lots, f64::from(integer(24)) / 100.0);
            matched += 1;
        }
    }
    assert_eq!(matched, 24);
}
