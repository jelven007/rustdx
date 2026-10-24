//! Synthetic boundary cases, separate from live protocol/business validation.
use rustdx_complete::tcp::stock::*;
use std::io::ErrorKind;
#[path = "common/extended_protocol.rs"]
mod common;
use common::*;

fn minute_payload(rows: &[[i64; 3]], previous_close: f32, open: f32) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    bytes.extend(previous_close.to_le_bytes());
    bytes.extend(open.to_le_bytes());
    for row in rows {
        for value in row {
            bytes.extend(varint(*value));
        }
    }
    bytes
}

#[test]
fn recent_minutes_rejects_negative_header_prices_even_for_empty_series() {
    let request = RecentMinutesRequest::new(0, "000001", 20261023).unwrap();
    for rows in [vec![], vec![[100, 10_000, 1]]] {
        for (previous_close, open) in [(-1.0, 1.0), (1.0, -1.0)] {
            let bytes = minute_payload(&rows, previous_close, open);
            assert_eq!(
                decode_recent_minutes(&request, &bytes, &metadata())
                    .unwrap_err()
                    .kind(),
                ErrorKind::InvalidData
            );
        }
    }
}

#[test]
fn recent_minutes_checks_counts_session_labels_and_zero_values() {
    let request = RecentMinutesRequest::new(0, "000001", 20261023).unwrap();
    let precision = metadata();
    for n in [0, 1, 119, 120, 121, 239, 240] {
        let bytes = minute_payload(&vec![[0, 0, 0]; n], 0.0, 0.0);
        let result = decode_recent_minutes(&request, &bytes, &precision).unwrap();
        assert_eq!(result.page.records.len(), n);
        if n == 240 {
            let points = &result.page.records;
            for (index, time) in [(0, "09:31"), (119, "11:30"), (120, "13:01"), (239, "15:00")] {
                assert_eq!(points[index].time_label, time);
                assert_eq!(points[index].price, 0.0);
                assert_eq!(points[index].average_price, 0.0);
                assert_eq!(points[index].volume_raw, 0);
            }
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(decode_recent_minutes(&request, &extra, &precision).is_err());
    }
    let mut bytes = minute_payload(&vec![[0, 0, 0]; 241], 0.0, 0.0);
    assert!(decode_recent_minutes(&request, &bytes, &precision).is_err());
    bytes[..2].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(decode_recent_minutes(&request, &bytes, &precision).is_err());
}

#[test]
fn recent_minutes_rejects_nonfinite_overflow_negative_and_truncated_fields() {
    let request = RecentMinutesRequest::new(0, "000001", 20261023).unwrap();
    let precision = metadata();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for (previous_close, open) in [(value, 1.0), (1.0, value)] {
            assert!(
                decode_recent_minutes(
                    &request,
                    &minute_payload(&[], previous_close, open),
                    &precision
                )
                .is_err()
            );
        }
    }
    for rows in [
        vec![[-1, 0, 0]],
        vec![[0, -1, 0]],
        vec![[0, 0, -1]],
        vec![[i64::MAX, 0, 0], [1, 0, 0]],
        vec![[0, i64::MAX, 0], [0, 1, 0]],
        vec![[1, 1, 0], [-2, 0, 0]],
        vec![[1, 1, 0], [0, -2, 0]],
    ] {
        assert!(
            decode_recent_minutes(&request, &minute_payload(&rows, 0.0, 0.0), &precision).is_err()
        );
    }
    let bytes = minute_payload(&[[i64::MAX, i64::MAX, i64::MAX]], 0.0, 0.0);
    assert!(decode_recent_minutes(&request, &bytes, &precision).is_ok());
    for end in 0..bytes.len() {
        assert!(
            decode_recent_minutes(&request, &bytes[..end], &precision).is_err(),
            "{end}"
        );
    }
    assert!(decode_recent_minutes(&request, &vec![0; 65_536], &precision).is_err());
}

#[test]
fn recent_minutes_request_date_and_symbol_bounds() {
    for date in [10000101, 20000229, 20240229, 99991231] {
        let request = RecentMinutesRequest::new(0, "000001", date).unwrap();
        let encoded = request.encode(1).unwrap();
        let selector = u32::from_le_bytes(encoded[12..16].try_into().unwrap());
        assert_eq!(u64::from(selector) + u64::from(date), 1u64 << 32);
    }
    for date in [
        0,
        9991231,
        19000229,
        20260230,
        20261301,
        20261000,
        100000101,
        u32::MAX,
    ] {
        assert_eq!(
            RecentMinutesRequest::new(0, "000001", date)
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput
        );
    }
    assert!(RecentMinutesRequest::new(0, "00000a", 20261023).is_ok());
    for (market, code) in [
        (2, "000001"),
        (0, "00001"),
        (0, "00000!"),
        (0, "０００００１"),
    ] {
        assert!(RecentMinutesRequest::new(market, code, 20261023).is_err());
    }
}

#[test]
fn refresh_accepts_all_one_hundred_records_with_and_without_opaque_tails() {
    let codes: Vec<_> = (0..100).map(|n| format!("{n:06}")).collect();
    let stocks: Vec<_> = codes.iter().map(|c| (0, c.as_str())).collect();
    let precision_entries: Vec<_> = stocks.iter().map(|&(m, c)| (m, c, 2)).collect();
    let precision =
        PrecisionSnapshot::from_decimals(&precision_entries, chrono::Utc::now(), None).unwrap();
    for n in [1, 12, 32, 100] {
        let request = RefreshRequest::from_stocks(&stocks[..n]).unwrap();
        for tail in [vec![], vec![0xa5; 46]] {
            let mut decoded = (n as u16).to_le_bytes().to_vec();
            // Replies may be out of request order.
            for code in codes[..n].iter().rev() {
                decoded.extend(quote(0, code, true, u32::MAX));
                decoded.extend(&tail);
            }
            let bytes: Vec<_> = decoded.into_iter().map(|b| b ^ 0x93).collect();
            let result = decode_quote_refresh(&request, &bytes, &precision).unwrap();
            assert_eq!(result.page.records.len(), n);
            for (q, code) in result.page.records.iter().zip(codes[..n].iter().rev()) {
                assert_eq!(&q.code, code);
                assert_eq!(q.tail_raw, tail);
                assert_eq!(q.update_cursor, Some(u32::MAX));
            }
        }
    }
}

#[test]
fn refresh_rejects_cursor_below_request_before_initial_baseline() {
    let request = RefreshRequest::new(vec![RefreshCursor {
        symbol: CommandSymbol::new(0, "000001").unwrap(),
        cursor: 10,
    }])
    .unwrap();
    for cursor in [10, 11, u32::MAX] {
        assert!(decode_quote_refresh(&request, &refresh_payload(cursor), &metadata()).is_ok());
    }
    assert_eq!(
        decode_quote_refresh(&request, &refresh_payload(9), &metadata())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn refresh_checks_count_identity_duplicates_and_ambiguous_boundaries() {
    let request =
        RefreshRequest::from_stocks(&[(0, "000001"), (0, "159915"), (1, "510300")]).unwrap();
    let precision = metadata();
    let decode = |decoded: Vec<u8>| {
        let bytes: Vec<_> = decoded.into_iter().map(|b| b ^ 0x93).collect();
        decode_quote_refresh(&request, &bytes, &precision)
    };
    assert!(decode(vec![0, 0]).is_ok());
    for decoded in [
        vec![],
        vec![0],
        vec![0, 0, 0],
        vec![1, 0],
        vec![4, 0],
        vec![255, 255],
    ] {
        assert!(decode(decoded).is_err());
    }
    for records in [
        vec![quote(0, "000001", true, 0), quote(0, "000001", true, 0)],
        vec![quote(0, "000001", true, 0), quote(0, "999999", true, 0)],
        // Two valid choices for the second record are ambiguous; neither is published.
        vec![
            quote(0, "000001", true, 0),
            quote(0, "159915", true, 0),
            quote(1, "510300", true, 0),
        ],
    ] {
        let mut decoded = vec![2, 0];
        for record in records {
            decoded.extend(record);
        }
        assert!(decode(decoded).is_err());
    }
    // A marker alone in an opaque tail must not create another record.
    let mut decoded = vec![2, 0];
    decoded.extend(quote(0, "000001", true, 0));
    decoded.extend([0]);
    decoded.extend(b"159915");
    decoded.extend([0xff; 20]);
    decoded.extend(quote(1, "510300", true, 0));
    let result = decode(decoded).unwrap();
    assert_eq!(result.page.records[1].code, "510300");
}

#[test]
fn refresh_preserves_marker_candidate_limit() {
    let request = RefreshRequest::from_stocks(&[(0, "000001")]).unwrap();
    for candidates in [512, 513] {
        let mut decoded = vec![1, 0];
        decoded.extend(quote(0, "000001", true, 0));
        for _ in 1..candidates {
            decoded.push(0);
            decoded.extend(b"000001");
        }
        let bytes: Vec<_> = decoded.into_iter().map(|b| b ^ 0x93).collect();
        let result = decode_quote_refresh(&request, &bytes, &metadata());
        if candidates == 512 {
            assert!(result.is_ok());
        } else {
            assert!(result.unwrap_err().to_string().contains("候选超过预算"));
        }
    }
}

#[test]
fn refresh_accepts_zero_prices_and_depth_but_rejects_negative_values() {
    let request = RefreshRequest::from_stocks(&[(0, "000001")]).unwrap();
    let mut record = vec![0];
    record.extend(b"000001");
    // Identity is 9 bytes; the remaining minimum scalar/cursor/five-depth
    // layout is 40 zero bytes. Every signed varint occupies one byte here.
    record.resize(49, 0);
    let decode = |record: &[u8]| {
        let mut decoded = vec![1, 0];
        decoded.extend(record);
        let bytes: Vec<_> = decoded.into_iter().map(|b| b ^ 0x93).collect();
        decode_quote_refresh(&request, &bytes, &metadata())
    };
    let result = decode(&record).unwrap();
    let q = &result.page.records[0];
    assert_eq!(q.price, Some(0.0));
    assert_eq!(q.update_cursor, Some(0));
    assert!(
        q.bids
            .iter()
            .chain(&q.asks)
            .all(|l| l.price == Some(0.0) && l.volume_raw == 0)
    );
    for offset in [9, 10, 11, 12, 13, 19, 20, 25, 26, 29, 30, 31, 32, 47, 48] {
        let mut negative = record.clone();
        negative[offset] = 0x41; // sign/magnitude -1
        assert!(decode(&negative).is_err(), "accepted negative at {offset}");
    }
    for end in 0..record.len() {
        assert!(
            decode(&record[..end]).is_err(),
            "accepted truncation at {end}"
        );
    }
}

#[test]
fn shared_quote_partition_accepts_eighty_snapshot_records() {
    let codes: Vec<_> = (0..80).map(|n| format!("{n:06}")).collect();
    let stocks: Vec<_> = codes.iter().map(|c| (0, c.as_str())).collect();
    let precision_entries: Vec<_> = stocks.iter().map(|&(m, c)| (m, c, 2)).collect();
    let precision =
        PrecisionSnapshot::from_decimals(&precision_entries, chrono::Utc::now(), None).unwrap();
    let request = SnapshotsRequest::new(&stocks).unwrap();
    let mut bytes = vec![0, 0, 80, 0];
    for code in codes.iter().rev() {
        bytes.extend(quote(0, code, false, 0));
        bytes.extend([0xa5; 56]);
    }
    let result = decode_snapshots(&request, &bytes, &precision).unwrap();
    assert_eq!(result.page.records.len(), 80);
    assert!(result.missing_codes.is_empty());
}
