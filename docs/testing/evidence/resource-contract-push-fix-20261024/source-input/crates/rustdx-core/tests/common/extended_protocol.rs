#![allow(dead_code)]
use rustdx_complete::tcp::stock::PrecisionSnapshot;

pub fn metadata() -> PrecisionSnapshot {
    PrecisionSnapshot::from_decimals(
        &[(0, "000001", 2), (0, "159915", 3), (1, "510300", 3)],
        chrono::Utc::now(),
        None,
    )
    .unwrap()
}
pub fn varint(value: i64) -> Vec<u8> {
    let mut magnitude = value.unsigned_abs();
    let mut bytes = vec![(magnitude & 63) as u8 | if value < 0 { 64 } else { 0 }];
    magnitude >>= 6;
    while magnitude != 0 {
        *bytes.last_mut().unwrap() |= 128;
        bytes.push((magnitude & 127) as u8);
        magnitude >>= 7;
    }
    bytes
}
/// Synthetic fault-testing quote, not a captured market fixture.
pub fn quote(market: u8, code: &str, refresh: bool, cursor: u32) -> Vec<u8> {
    let mut bytes = vec![market];
    bytes.extend_from_slice(code.as_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    for value in [1234, -34, -10, 40, -50] {
        bytes.extend(varint(value));
    }
    if refresh {
        bytes.extend_from_slice(&cursor.to_le_bytes());
    } else {
        bytes.extend(varint(930));
    }
    for value in [0, 100, 2] {
        bytes.extend(varint(value));
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for value in [40, 60, 0, 1] {
        bytes.extend(varint(value));
    }
    for _ in 0..if refresh { 5 } else { 1 } {
        for value in [-1, 1, 100, 200] {
            bytes.extend(varint(value));
        }
    }
    bytes
}
pub fn refresh_payload(cursor: u32) -> Vec<u8> {
    let mut bytes = 1u16.to_le_bytes().to_vec();
    bytes.extend(quote(0, "000001", true, cursor));
    bytes.into_iter().map(|b| b ^ 0x93).collect()
}
pub fn response(request: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 16];
    bytes[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
    bytes[5..9].copy_from_slice(&request[1..5]);
    bytes[10..12].copy_from_slice(&request[10..12]);
    bytes[12..14].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes[14..16].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
