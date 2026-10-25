#![allow(dead_code)]
use rustdx_complete::tcp::stock::*;
use std::{num::NonZeroU64, path::Path};

pub const DATE: u32 = 20261023;
pub fn evidence_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trade-amount")
}
pub fn source() -> TurnoverSource {
    TurnoverSource::archived(
        "115.238.56.198:7709".parse().unwrap(),
        NonZeroU64::new(1).unwrap(),
        "2026-10-24T05:11:36Z".parse().unwrap(),
        "enhanced-history-amount-20261024",
    )
    .unwrap()
}
pub fn profile() -> TurnoverProfile {
    TurnoverProfile::a_share(
        0,
        "300308",
        DATE,
        source().endpoint(),
        "docs/testing/evidence/enhanced-history-amount-20261024/results.json",
    )
    .unwrap()
}
pub fn context(query: TradeQuery) -> TradeAmountContext {
    TradeAmountContext::a_share(
        query,
        TradeQuantityUnit::LotsOf100Shares,
        "independent frozen volume attribution",
    )
    .unwrap()
}
/// Validate retained raw frame, request command, and decompressed payload.
pub fn frozen(label: &str) -> (Vec<u8>, Vec<u8>) {
    let path = evidence_path().join("wire");
    let request = std::fs::read(path.join(format!("{label}.request.bin"))).unwrap();
    let header = std::fs::read(path.join(format!("{label}.header.bin"))).unwrap();
    let wire = std::fs::read(path.join(format!("{label}.wire.bin"))).unwrap();
    let payload = std::fs::read(path.join(format!("{label}.payload.bin"))).unwrap();
    let frame = std::fs::read(path.join(format!("{label}.frame.bin"))).unwrap();
    assert_eq!(&header[..4], &[0xb1, 0xcb, 0x74, 0]);
    assert_eq!(&header[5..9], &request[1..5]);
    assert_eq!(&header[10..12], &request[10..12]);
    let compressed = u16::from_le_bytes(header[12..14].try_into().unwrap()) as usize;
    let expanded = u16::from_le_bytes(header[14..16].try_into().unwrap()) as usize;
    assert_eq!(wire.len(), compressed);
    assert_eq!(payload.len(), expanded);
    assert_eq!(frame, [header, wire.clone()].concat());
    assert_eq!(
        payload,
        if compressed == expanded {
            wire
        } else {
            miniz_oxide::inflate::decompress_to_vec_zlib(&wire).unwrap()
        }
    );
    (request, payload)
}
pub fn collection(legacy: bool) -> TradeCollection {
    let query = if legacy {
        TradeQuery::history_legacy(0, "300308", DATE)
    } else {
        TradeQuery::history(0, "300308", DATE)
    }
    .unwrap();
    let mut pages = Vec::new();
    let mut start = 0;
    for index in 0..4 {
        let (request, payload) = frozen(&format!(
            "{}-{index:02}",
            if legacy { "legacy" } else { "enhanced" }
        ));
        assert_eq!(request, query.encode(start, 1800).unwrap());
        let page = decode_trade_page(&query, start, 1800, &payload).unwrap();
        start = page.next_offset() as u16;
        pages.push(page);
    }
    TradeCollection::new(query, pages, TradeTermination::EmptyPageObserved).unwrap()
}
pub fn report(category: u16) -> ReportedTurnover {
    let (bytes, payload) = frozen(&format!("kline-{category}"));
    let count = u16::from_le_bytes(bytes[26..28].try_into().unwrap());
    let request = KlineRequest::new(0, "300308", category, 0, count).unwrap();
    assert_eq!(request.encode().as_slice(), bytes);
    let page = decode_reported_turnover(&request, &payload, Some(source())).unwrap();
    ReportedTurnover::from_pages(
        TurnoverQuery::new(
            0,
            "300308",
            DATE,
            match category {
                8 => TurnoverScope::Minute1,
                0 => TurnoverScope::Minute5,
                _ => TurnoverScope::RegularWithAuctions,
            },
            category,
        )
        .unwrap(),
        vec![page],
        Some(&profile()),
    )
    .unwrap()
}
pub fn varint(value: i32, bytes: &mut Vec<u8>) {
    let mut magnitude = i64::from(value).unsigned_abs();
    let mut first = (magnitude & 0x3f) as u8 | if value < 0 { 0x40 } else { 0 };
    magnitude >>= 6;
    if magnitude != 0 {
        first |= 0x80;
    }
    bytes.push(first);
    while magnitude != 0 {
        let mut next = (magnitude & 0x7f) as u8;
        magnitude >>= 7;
        if magnitude != 0 {
            next |= 0x80;
        }
        bytes.push(next);
    }
}
pub type Row = (u16, i32, i32, i32);
pub fn trade_payload(rows: &[Row], protocol: TradeProtocol) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    if protocol != TradeProtocol::Today0fc5 {
        bytes.extend_from_slice(&[0; 4]);
    }
    for &(time, delta, volume, status) in rows {
        bytes.extend_from_slice(&time.to_le_bytes());
        varint(delta, &mut bytes);
        varint(volume, &mut bytes);
        if protocol != TradeProtocol::LegacyHistory0fb5 {
            varint(1, &mut bytes);
        }
        varint(status, &mut bytes);
        varint(0, &mut bytes);
    }
    bytes
}
pub fn page(query: &TradeQuery, start: u16, rows: &[Row]) -> TradePage {
    decode_trade_page(query, start, 1800, &trade_payload(rows, query.protocol())).unwrap()
}
pub fn simple_collection(rows: &[Row]) -> TradeCollection {
    let query = TradeQuery::history(0, "300308", DATE).unwrap();
    let mut pages = vec![page(&query, 0, rows)];
    if !rows.is_empty() {
        pages.push(page(&query, rows.len() as u16, &[]));
    }
    TradeCollection::new(query.clone(), pages, TradeTermination::EmptyPageObserved).unwrap()
}
pub fn kline_payload(category: u16, bars: &[(u32, u16, f32, f32)]) -> Vec<u8> {
    let mut bytes = (bars.len() as u16).to_le_bytes().to_vec();
    for &(date, time, volume, amount) in bars {
        if matches!(category, 0 | 8) {
            let packed = ((date / 10000 - 2004) * 2048 + date % 10000) as u16;
            bytes.extend_from_slice(&packed.to_le_bytes());
            bytes.extend_from_slice(&time.to_le_bytes());
        } else {
            bytes.extend_from_slice(&date.to_le_bytes());
        }
        for delta in [10000, 0, 0, 0] {
            varint(delta, &mut bytes);
        }
        bytes.extend_from_slice(&volume.to_bits().to_le_bytes());
        bytes.extend_from_slice(&amount.to_bits().to_le_bytes());
    }
    bytes
}
pub fn small_report(bars: &[(u32, u16, f32, f32)]) -> ReportedTurnover {
    let request = KlineRequest::new(0, "300308", 8, 0, 800).unwrap();
    let page = decode_reported_turnover(&request, &kline_payload(8, bars), Some(source())).unwrap();
    ReportedTurnover::from_pages(
        TurnoverQuery::new(0, "300308", DATE, TurnoverScope::Minute1, 8).unwrap(),
        vec![page],
        Some(&profile()),
    )
    .unwrap()
}
