//! Synthetic nonempty 0feb cases and local wire replay, not live market evidence.
use rustdx_complete::tcp::{PACK1, Tcp, TcpConfig, stock::*};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};
#[path = "common/ip_pool.rs"]
mod ip_pool;
#[path = "common/extended_protocol.rs"]
mod protocol;

fn payload(rows: &[[i64; 3]]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    bytes.extend(2.69f32.to_le_bytes());
    bytes.extend(2.7f32.to_le_bytes());
    for row in rows {
        for &value in row {
            bytes.extend(protocol::varint(value));
        }
    }
    bytes
}

fn request(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 10];
    stream.read_exact(&mut header).unwrap();
    let mut bytes = header.to_vec();
    bytes.resize(
        10 + usize::from(u16::from_le_bytes([header[6], header[7]])),
        0,
    );
    stream.read_exact(&mut bytes[10..]).unwrap();
    bytes
}

#[test]
fn fixed_nonempty_bytes_preserve_signed_deltas_zero_volume_and_unknown_date() {
    // Independently hand-encoded: base (2702, 270000), then (+10,+1000),
    // (-5,-2000), and (0,0). Values are relative to the FIRST point.
    let bytes = [
        4, 0, 246, 40, 44, 64, 205, 204, 44, 64, 142, 42, 176, 250, 32, 0, 10, 168, 15, 20, 69,
        208, 31, 30, 0, 0, 0,
    ];
    let query = RecentMinutesRequest::new(0, "159915", 20261023).unwrap();
    let series = decode_recent_minutes(&query, &bytes, &protocol::metadata()).unwrap();
    let expected = [
        (2702, 270000, 2.702, 2.7, 0),
        (2712, 271000, 2.712, 2.71, 20),
        (2697, 268000, 2.697, 2.68, 30),
        (2702, 270000, 2.702, 2.7, 0),
    ];
    assert_eq!(series.previous_close_bits, 0x402c28f6);
    assert_eq!(series.open_bits, 0x402ccccd);
    assert_eq!(series.requested_date, 20261023);
    assert_eq!(series.server_trading_date, None);
    assert_eq!(&*series.page.raw_payload, &bytes);
    assert!(series.page.source.endpoint.is_none());
    for (index, (point, expected)) in series.page.records.iter().zip(expected).enumerate() {
        assert_eq!(
            (
                point.price_raw,
                point.average_raw,
                point.price,
                point.average_price,
                point.volume_raw
            ),
            expected
        );
        assert_eq!(point.index, index as u16);
    }
    // Every proper prefix is incomplete, including prefixes inside a varint.
    for end in 0..bytes.len() {
        assert!(decode_recent_minutes(&query, &bytes[..end], &protocol::metadata()).is_err());
    }
}

#[test]
fn full_session_respects_lunch_boundary_point_limit_and_all_supported_scales() {
    let rows: Vec<_> = (0..240)
        .map(|index| match index {
            0 => [27020, 2700000, 0],
            _ if index % 2 == 0 => [-20, -2000, 30],
            _ => [10, 1000, 20],
        })
        .collect();
    let bytes = payload(&rows);
    let query = RecentMinutesRequest::new(0, "159915", 20261023).unwrap();
    for (decimal, divisor) in [(2, 100.0), (3, 1000.0), (4, 10000.0)] {
        let metadata =
            PrecisionSnapshot::from_decimals(&[(0, "159915", decimal)], chrono::Utc::now(), None)
                .unwrap();
        for n in [1, 120, 121, 240] {
            let series = decode_recent_minutes(&query, &payload(&rows[..n]), &metadata).unwrap();
            assert_eq!(series.page.records.len(), n);
            assert_eq!(series.scale.decimal(), decimal);
            for (index, point) in series.page.records.iter().enumerate() {
                let (price, average, volume) = match index {
                    0 => (27020, 2700000, 0),
                    _ if index % 2 == 0 => (27000, 2698000, 30),
                    _ => (27030, 2701000, 20),
                };
                assert_eq!(
                    (point.price_raw, point.average_raw, point.volume_raw),
                    (price, average, volume)
                );
                assert_eq!(point.price, price as f64 / divisor);
                assert_eq!(point.average_price, average as f64 / (divisor * 100.0));
            }
        }
        let series = decode_recent_minutes(&query, &bytes, &metadata).unwrap();
        for (index, label) in [(0, "09:31"), (119, "11:30"), (120, "13:01"), (239, "15:00")] {
            assert_eq!(series.page.records[index].time_label, label);
        }
    }
    let mut excessive = rows;
    excessive.push([0, 0, 0]);
    assert!(decode_recent_minutes(&query, &payload(&excessive), &protocol::metadata()).is_err());
}

#[test]
fn nonempty_rejects_overflow_negative_values_bad_floats_and_extra_bytes() {
    let query = RecentMinutesRequest::new(0, "159915", 20261023).unwrap();
    let metadata = protocol::metadata();
    for rows in [
        vec![[-1, 100, 0]],
        vec![[100, -1, 0]],
        vec![[100, 100, -1]],
        vec![[100, 100, 0], [-101, 0, 0]],
        vec![[100, 100, 0], [0, -101, 0]],
        vec![[i64::MAX, 100, 0], [1, 0, 0]],
        vec![[100, i64::MAX, 0], [0, 1, 0]],
        vec![[100, 100, 0], [i64::MIN, 0, 0]],
    ] {
        assert_eq!(
            decode_recent_minutes(&query, &payload(&rows), &metadata)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
    // The full signed-varint positive range is retained, without i32 narrowing.
    let large = decode_recent_minutes(
        &query,
        &payload(&[[i64::MAX, i64::MAX, i64::MAX]]),
        &metadata,
    )
    .unwrap();
    assert_eq!(large.page.records[0].volume_raw, i64::MAX);
    for offset in [2, 6] {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bytes = payload(&[[100, 100, 0]]);
            bytes[offset..offset + 4].copy_from_slice(&bad.to_le_bytes());
            assert!(decode_recent_minutes(&query, &bytes, &metadata).is_err());
        }
    }
    let mut trailing = payload(&[[100, 100, 0]]);
    trailing.push(0);
    assert!(decode_recent_minutes(&query, &trailing, &metadata).is_err());
    let mut empty_with_rows = payload(&[[100, 100, 0]]);
    empty_with_rows[..2].copy_from_slice(&0u16.to_le_bytes());
    assert!(decode_recent_minutes(&query, &empty_with_rows, &metadata).is_err());
    let mut overlong = payload(&[]);
    overlong[..2].copy_from_slice(&1u16.to_le_bytes());
    overlong.extend([0xff; 11]);
    overlong.extend([0, 0]);
    assert!(decode_recent_minutes(&query, &overlong, &metadata).is_err());
}

#[test]
fn nonempty_requires_symbol_precision_and_preserves_leap_date_selector() {
    let query = RecentMinutesRequest::new(0, "159915", 20240229).unwrap();
    // Independently computed signed -20240229, little-endian two's complement.
    assert_eq!(
        &query.encode(17).unwrap()[12..16],
        &[0x9b, 0x28, 0xcb, 0xfe]
    );
    assert!(RecentMinutesRequest::new(0, "159915", 20230229).is_err());
    let missing =
        PrecisionSnapshot::from_decimals(&[(0, "000001", 2)], chrono::Utc::now(), None).unwrap();
    assert_eq!(
        decode_recent_minutes(&query, &payload(&[[100, 100, 0]]), &missing)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    let unsupported =
        PrecisionSnapshot::from_decimals(&[(0, "159915", 5)], chrono::Utc::now(), None).unwrap();
    assert_eq!(
        decode_recent_minutes(&query, &payload(&[[100, 100, 0]]), &unsupported)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn date_selector_preserves_calendar_boundaries_and_does_not_repeat_legacy_offset() {
    assert_eq!(RECENT_MINUTES_DATE_SCHEMA_VERSION, 2);
    for requested in [20260609, 20260610, 20260817, 20260818, 20261023, 20261024] {
        let query = RecentMinutesRequest::new(1, "600000", requested).unwrap();
        let encoded = query.encode(17).unwrap();
        let signed = i32::from_le_bytes(encoded[12..16].try_into().unwrap());
        assert_eq!(i64::from(signed), -i64::from(requested));
    }
    let august = RecentMinutesRequest::new(1, "600000", 20260817)
        .unwrap()
        .encode(17)
        .unwrap();
    // Frozen legacy Aug 17 selector actually sent -20260609 to the server.
    assert_ne!(&august[12..16], &0xfecad8ffu32.to_le_bytes());
    let june = RecentMinutesRequest::new(1, "600000", 20260609)
        .unwrap()
        .encode(17)
        .unwrap();
    assert_eq!(&june[12..16], &0xfecad8ffu32.to_le_bytes());
    // Calendar boundaries are decimal YYYYMMDD boundaries, not ordinal steps.
    let dec = RecentMinutesRequest::new(0, "000001", 20261231).unwrap();
    let jan = RecentMinutesRequest::new(0, "000001", 20270101).unwrap();
    let decode = |q: RecentMinutesRequest| {
        i32::from_le_bytes(q.encode(17).unwrap()[12..16].try_into().unwrap())
    };
    assert_eq!(decode(dec) - decode(jan), 8870);
}

#[test]
fn requested_weekend_and_out_of_window_dates_never_qualify_cached_data() {
    for requested in [20261023, 20261024, 20240229, 20270104] {
        let query = RecentMinutesRequest::new(0, "159915", requested).unwrap();
        for bytes in [payload(&[]), payload(&[[2702, 270000, 20]])] {
            let mut series = decode_recent_minutes(&query, &bytes, &protocol::metadata()).unwrap();
            assert_eq!(
                series.date_qualification(),
                MinuteDateQualification::Unverified {
                    reason: MinuteDateUnavailableReason::ServerDoesNotEchoTradingDate,
                }
            );
            assert_eq!(
                series.require_trading_date().unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
            // Mutable legacy metadata cannot promote an unverified series.
            series.server_trading_date = Some(requested);
            series.requested_date = 20261023;
            assert!(series.require_trading_date().is_err());
            let json = serde_json::to_value(&series).unwrap();
            assert_eq!(
                json["date_qualification"]["Unverified"]["reason"],
                "ServerDoesNotEchoTradingDate"
            );
        }
    }
}

#[test]
fn nonempty_compressed_wire_replay_and_mismatched_identity_do_not_hide_errors() {
    for mismatch in [0, 1, 2] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = TcpConfig {
            ip: Some(listener.local_addr().unwrap()),
            ip_pool: Some(ip_pool::prepare(&listener)),
            timeout: Duration::from_secs(1),
            ..Default::default()
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let hello = request(&mut stream);
            assert_eq!(hello, PACK1);
            stream
                .write_all(&protocol::response(&hello, &[0, 0]))
                .unwrap();
            let req = request(&mut stream);
            assert_eq!(&req[10..12], &0x0febu16.to_le_bytes());
            assert_eq!(&req[17..23], b"159915");
            let bytes = payload(&[[2702, 270000, 0], [-5, -2000, 30]]);
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 6);
            assert_ne!(compressed.len(), bytes.len());
            let mut reply = protocol::response(&req, &bytes);
            reply[12..14].copy_from_slice(&(compressed.len() as u16).to_le_bytes());
            reply.truncate(16);
            reply.extend(compressed);
            match mismatch {
                1 => reply[5..9].copy_from_slice(&0xffffu32.to_le_bytes()),
                2 => reply[10..12].copy_from_slice(&0x0537u16.to_le_bytes()),
                _ => {}
            }
            stream.write_all(&reply).unwrap();
        });
        let mut tcp = Tcp::with_config(&config).unwrap();
        tcp.set_heartbeat_interval(None).unwrap();
        let query = RecentMinutesRequest::new(0, "159915", 20261023).unwrap();
        let result = query.recv(&mut tcp, &protocol::metadata());
        if mismatch == 0 {
            let series = result.unwrap();
            assert_eq!(series.page.records[1].price, 2.697);
            assert_eq!(series.page.records[1].average_price, 2.68);
            assert_eq!(series.page.source.endpoint, config.ip);
            assert_eq!(
                series.page.source.connection_id,
                Some(tcp.connection_id().get())
            );
            assert!(series.page.source.fetched_at.is_some());
            assert_eq!(series.server_trading_date, None);
        } else {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
            assert!(!tcp.is_reusable());
        }
        drop(tcp);
        server.join().unwrap();
    }
}
