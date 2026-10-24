//! Frozen nonempty live 0feb frames. Requested dates are not echoed server dates.
use rustdx_complete::tcp::{PACK1, Tcp, TcpConfig, stock::*};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
#[path = "common/ip_pool.rs"]
mod ip_pool;
#[path = "common/extended_protocol.rs"]
mod protocol;

fn fixtures() -> (PathBuf, Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recent_minutes_nonempty");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["synthetic"], false);
    assert_eq!(manifest["server_trading_date_verified"], false);
    (root, manifest)
}

fn precision(fixture: &Value) -> PrecisionSnapshot {
    PrecisionSnapshot::from_decimals(
        &[(
            fixture["market"].as_u64().unwrap() as u16,
            fixture["code"].as_str().unwrap(),
            fixture["decimal"].as_u64().unwrap() as u8,
        )],
        chrono::Utc::now(),
        None,
    )
    .unwrap()
}

fn legacy_wire_date(annotation: u32) -> u32 {
    // Preserve frozen annotations and bytes: the old constructor sent these
    // June selectors when the caller requested the annotated August dates.
    match annotation {
        20260817 => 20260609,
        20260818 => 20260610,
        _ => panic!("unexpected legacy fixture date"),
    }
}

fn query(fixture: &Value) -> RecentMinutesRequest {
    RecentMinutesRequest::new(
        fixture["market"].as_u64().unwrap() as u16,
        fixture["code"].as_str().unwrap(),
        legacy_wire_date(fixture["requested_date"].as_u64().unwrap() as u32),
    )
    .unwrap()
}

fn assert_series(series: &MinuteSeries, expected: &Value, legacy_annotation: bool) {
    let annotation = expected["requested_date"].as_u64().unwrap() as u32;
    assert_eq!(
        series.requested_date,
        if legacy_annotation {
            legacy_wire_date(annotation)
        } else {
            annotation
        }
    );
    if legacy_annotation {
        assert_ne!(series.requested_date, annotation);
    }
    assert_eq!(series.server_trading_date, None);
    assert_eq!(
        series.date_selector_raw,
        expected["date_selector_raw"].as_u64().unwrap() as u32
    );
    assert_eq!(
        series.previous_close_bits,
        expected["previous_close_bits"].as_u64().unwrap() as u32
    );
    assert_eq!(
        series.open_bits,
        expected["open_bits"].as_u64().unwrap() as u32
    );
    let records = expected["records"].as_array().unwrap();
    assert_eq!(series.page.records.len(), records.len());
    for (point, row) in series.page.records.iter().zip(records) {
        assert_eq!(point.index, row["index"].as_u64().unwrap() as u16);
        assert_eq!(point.time_label, row["time_label"].as_str().unwrap());
        assert_eq!(
            point.price_field_raw,
            row["price_field_raw"].as_i64().unwrap()
        );
        assert_eq!(
            point.average_field_raw,
            row["average_field_raw"].as_i64().unwrap()
        );
        assert_eq!(point.volume_raw, row["volume_raw"].as_i64().unwrap());
        assert_eq!(point.price_raw, row["price_raw"].as_i64().unwrap());
        assert_eq!(point.average_raw, row["average_raw"].as_i64().unwrap());
        // Raw integers are exact; tolerate only JSON's floating-point roundtrip.
        assert!((point.price - row["price"].as_f64().unwrap()).abs() < 1e-11);
        assert!((point.average_price - row["average_price"].as_f64().unwrap()).abs() < 1e-11);
    }
}

#[test]
fn twenty_four_nonempty_live_frames_match_5760_independent_expected_records() {
    let (root, manifest) = fixtures();
    let fixtures = manifest["fixtures"].as_object().unwrap();
    assert_eq!(fixtures.len(), 24);
    let mut count = 0;
    let mut signed_deltas = false;
    for (name, fixture) in fixtures {
        let read = |suffix: &str| std::fs::read(root.join(format!("{name}.{suffix}"))).unwrap();
        let header: [u8; 16] = read("header.bin").try_into().unwrap();
        let payload = read("payload.bin");
        assert_eq!(
            rustdx_complete::tcp::decode_frame(&header, &read("wire.bin"), true).unwrap(),
            payload
        );
        let saved_request = read("request.bin");
        let id = u32::from_le_bytes(saved_request[1..5].try_into().unwrap());
        let request = query(fixture);
        assert_eq!(request.encode(id).unwrap(), saved_request);
        assert_eq!(&header[5..9], &saved_request[1..5]);
        assert_eq!(&header[10..12], &0x0febu16.to_le_bytes());
        let expected: Value = serde_json::from_slice(&read("expected.json")).unwrap();
        let series = decode_recent_minutes(&request, &payload, &precision(fixture)).unwrap();
        assert_series(&series, &expected, true);
        assert_eq!(series.page.records.len(), 240);
        assert_eq!(&*series.page.raw_payload, &payload);
        assert_eq!(series.page.records[119].time_label, "11:30");
        assert_eq!(series.page.records[120].time_label, "13:01");
        assert!(series.page.source.endpoint.is_none());
        signed_deltas |= series
            .page
            .records
            .iter()
            .any(|p| p.price_field_raw < 0 || p.average_field_raw < 0);
        count += series.page.records.len();
    }
    assert_eq!(count, 5760);
    assert!(signed_deltas, "live fixture must exercise signed deltas");
}

#[test]
fn corrected_selectors_replay_72_live_series_with_17280_independent_records() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recent_minutes_date_selector");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["synthetic"], false);
    assert_eq!(manifest["server_trading_date_verified"], false);
    let entries = manifest["fixtures"].as_object().unwrap();
    assert_eq!(entries.len(), 72);
    let mut count = 0;
    for (name, fixture) in entries {
        assert_eq!(fixture["recent_minutes_date_schema_version"], 2);
        let read = |suffix: &str| std::fs::read(root.join(format!("{name}.{suffix}"))).unwrap();
        let saved = read("request.bin");
        let requested = fixture["requested_date"].as_u64().unwrap() as u32;
        let request = RecentMinutesRequest::new(
            fixture["market"].as_u64().unwrap() as u16,
            fixture["code"].as_str().unwrap(),
            requested,
        )
        .unwrap();
        let id = u32::from_le_bytes(saved[1..5].try_into().unwrap());
        assert_eq!(request.encode(id).unwrap(), saved);
        let header: [u8; 16] = read("header.bin").try_into().unwrap();
        assert_eq!(&header[5..9], &saved[1..5]);
        let bytes = read("payload.bin");
        assert_eq!(
            rustdx_complete::tcp::decode_frame(&header, &read("wire.bin"), true).unwrap(),
            bytes
        );
        let expected: Value = serde_json::from_slice(&read("expected.json")).unwrap();
        let series = decode_recent_minutes(&request, &bytes, &precision(fixture)).unwrap();
        assert_series(&series, &expected, false);
        assert_eq!(&*series.page.raw_payload, &bytes);
        assert_eq!(series.page.records.len(), 240);
        assert!(series.require_trading_date().is_err());
        count += series.page.records.len();
    }
    assert_eq!(count, 17280);
}

#[test]
fn nonempty_live_payload_truncations_never_return_a_partial_series() {
    let (root, manifest) = fixtures();
    for code in ["000001", "159915", "600519"] {
        let (name, fixture) = manifest["fixtures"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, fixture)| fixture["code"] == code)
            .unwrap();
        let payload = std::fs::read(root.join(format!("{name}.payload.bin"))).unwrap();
        let request = query(fixture);
        let precision = precision(fixture);
        for end in 0..payload.len() {
            assert!(
                decode_recent_minutes(&request, &payload[..end], &precision).is_err(),
                "{name} at {end}"
            );
        }
        let mut extra = payload.clone();
        extra.push(0);
        assert!(decode_recent_minutes(&request, &extra, &precision).is_err());
        extra[..2].copy_from_slice(&241u16.to_le_bytes());
        assert!(decode_recent_minutes(&request, &extra, &precision).is_err());
    }
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
fn nonempty_live_stock_and_etf_bodies_replay_through_checked_network_entry() {
    let (root, manifest) = fixtures();
    for code in ["000001", "159915", "510300"] {
        let (name, fixture) = manifest["fixtures"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, fixture)| fixture["code"] == code)
            .unwrap();
        let mut header = std::fs::read(root.join(format!("{name}.header.bin"))).unwrap();
        let wire = std::fs::read(root.join(format!("{name}.wire.bin"))).unwrap();
        let expected: Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("{name}.expected.json"))).unwrap(),
        )
        .unwrap();
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
            // Only the local replay's routing ID changes; captured wire is untouched.
            header[5..9].copy_from_slice(&req[1..5]);
            stream.write_all(&header).unwrap();
            stream.write_all(&wire).unwrap();
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let mut tcp = Tcp::with_config(&config).unwrap();
        tcp.set_heartbeat_interval(None).unwrap();
        let series = query(fixture).recv(&mut tcp, &precision(fixture)).unwrap();
        assert_series(&series, &expected, true);
        assert_eq!(series.page.source.endpoint, config.ip);
        assert_eq!(
            series.page.source.connection_id,
            Some(tcp.connection_id().get())
        );
        drop(tcp);
        server.join().unwrap();
    }
}
