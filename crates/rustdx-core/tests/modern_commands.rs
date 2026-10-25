//! New default command paths: independent fixtures and complete local wire I/O.
use rustdx_complete::tcp::{
    self, HEARTBEAT_REQUEST, PACK1, Tcp, TcpConfig,
    stock::{
        Client, PrecisionSnapshot, SecurityBoard, SecurityCategory, SecurityDirectoryRequest,
        TodayMinuteRequest, TradeProtocol, TradeQuery, decode_security_directory,
        decode_today_minutes, decode_trade_page,
    },
};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

fn varint(value: i32, bytes: &mut Vec<u8>) {
    let mut magnitude = i64::from(value).unsigned_abs();
    let sign = if value < 0 { 0x40 } else { 0 };
    let first = (magnitude & 63) as u8 | sign;
    magnitude >>= 6;
    bytes.push(first | if magnitude > 0 { 128 } else { 0 });
    while magnitude > 0 {
        let next = (magnitude & 127) as u8;
        magnitude >>= 7;
        bytes.push(next | if magnitude > 0 { 128 } else { 0 });
    }
}

fn directory(codes: &[(&str, u8)]) -> Vec<u8> {
    let mut bytes = (codes.len() as u16).to_le_bytes().to_vec();
    for &(code, decimal) in codes {
        let mut row = [0; 37];
        row[..6].copy_from_slice(code.as_bytes());
        row[6..8].copy_from_slice(&100u16.to_le_bytes());
        row[8..24].copy_from_slice(b"FULL-NAME-123456");
        row[24..28].copy_from_slice(&12.5f32.to_le_bytes());
        row[28] = decimal;
        row[29..33].copy_from_slice(&3.625f32.to_le_bytes());
        row[33..37].copy_from_slice(&[1, 2, 3, 4]);
        bytes.extend(row);
    }
    bytes
}

fn minutes(rows: &[[i32; 3]]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    bytes.extend([7, 0]);
    for row in rows {
        for value in row {
            varint(*value, &mut bytes);
        }
    }
    bytes
}

fn ack() -> Vec<u8> {
    let mut bytes = vec![1, 2, 3, 4, 5, 6];
    bytes.extend(20261023u32.to_le_bytes());
    bytes
}

fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut header = [0; 10];
    stream.read_exact(&mut header)?;
    let mut bytes = header.to_vec();
    bytes.resize(
        10 + usize::from(u16::from_le_bytes([header[6], header[7]])),
        0,
    );
    stream.read_exact(&mut bytes[10..])?;
    Ok(bytes)
}

fn frame(stream: &mut TcpStream, request: &[u8], bytes: &[u8]) -> io::Result<()> {
    let body = miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6);
    let mut header = [0; 16];
    header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
    header[5..9].copy_from_slice(&request[1..5]);
    header[10..12].copy_from_slice(&request[10..12]);
    header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
    header[14..16].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
    // Fragment every header and body, including zero-count pages.
    for fragment in header.chunks(3).chain(body.chunks(3)) {
        stream.write_all(fragment)?;
    }
    Ok(())
}

fn server(
    connections: usize,
    mut reply: impl FnMut(&[u8]) -> Vec<u8> + Send + 'static,
) -> (TcpConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let handle = std::thread::spawn(move || {
        for _ in 0..connections {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            assert_eq!(read_request(&mut socket).unwrap(), PACK1);
            frame(&mut socket, PACK1, &[0, 0]).unwrap();
            while let Ok(request) = read_request(&mut socket) {
                if frame(&mut socket, &request, &reply(&request)).is_err() {
                    break;
                }
            }
        }
    });
    (config, handle)
}

#[test]
fn directory_has_u32_offsets_full_name_ieee_prices_and_checked_pages() {
    let request = SecurityDirectoryRequest::new(1, 70000, 1600).unwrap();
    let encoded = request.encode();
    assert_eq!(&encoded[10..12], &[0x4d, 4]);
    assert_eq!(&encoded[14..18], &70000u32.to_le_bytes());
    assert_eq!(&encoded[18..22], &1600u32.to_le_bytes());
    assert!(SecurityDirectoryRequest::new(0, 0, 1601).is_err());
    assert!(SecurityDirectoryRequest::new(0, 0, 0).is_err());
    assert!(SecurityDirectoryRequest::new(2, 0, 1).is_err());
    let bytes = directory(&[("688001", 3)]);
    let page = decode_security_directory(&request, &bytes).unwrap();
    let row = &page.records[0];
    assert_eq!(row.name, "FULL-NAME-123456");
    assert_eq!(row.previous_close, 3.625);
    assert_eq!(row.volume_ratio_base, 12.5);
    assert_eq!(row.unknown3_raw, [1, 2, 3, 4]);
    assert_eq!(
        (row.category, row.board),
        (SecurityCategory::AShare, SecurityBoard::Star)
    );
    // Eight GBK characters occupy all sixteen name bytes, with no NUL.
    // Subsequent numeric slots must keep their original offsets.
    let mut gbk = bytes.clone();
    gbk[10..26].copy_from_slice(&[
        0xc6, 0xbd, 0xb0, 0xb2, 0xd2, 0xf8, 0xd0, 0xd0, 0xb2, 0xe2, 0xca, 0xd4, 0xc8, 0xab, 0xc3,
        0xfb,
    ]);
    let page = decode_security_directory(&request, &gbk).unwrap();
    assert_eq!(page.records[0].name, "平安银行测试全名");
    assert_eq!(page.records[0].previous_close, 3.625);
    assert_eq!(page.records[0].volume_ratio_base, 12.5);
    assert_eq!(page.raw_payload, gbk);
    for end in 0..bytes.len() {
        assert!(decode_security_directory(&request, &bytes[..end]).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode_security_directory(&request, &extra).is_err());
}

#[test]
fn today_minute_uses_first_point_bases_and_distinct_average_scale() {
    let request = TodayMinuteRequest::new(0, "159915").unwrap();
    assert_eq!(&request.encode()[10..12], &[0x37, 5]);
    assert_eq!(&request.encode()[20..24], &[0, 0, 0, 0x93]);
    let metadata =
        PrecisionSnapshot::from_decimals(&[(0, "159915", 3)], chrono::Utc::now(), None).unwrap();
    let bytes = minutes(&[[3000, 300000, 10], [-100, 500, 20], [50, -500, 30]]);
    let result = decode_today_minutes(&request, &bytes, &metadata).unwrap();
    assert_eq!(
        result.points.iter().map(|p| p.price).collect::<Vec<_>>(),
        [3.0, 2.9, 3.05]
    );
    assert_eq!(
        result
            .points
            .iter()
            .map(|p| p.average_price)
            .collect::<Vec<_>>(),
        [3.0, 3.005, 2.995]
    );
    assert_eq!(result.points[0].time_label, "09:31");
    assert_eq!(result.trading_date, None);
    for end in 0..bytes.len() {
        assert!(decode_today_minutes(&request, &bytes[..end], &metadata).is_err());
    }
    let full =
        decode_today_minutes(&request, &minutes(&vec![[3000, 300000, 0]; 240]), &metadata).unwrap();
    assert_eq!(full.points[119].time_label, "11:30");
    assert_eq!(full.points[120].time_label, "13:01");
    assert_eq!(full.points[239].time_label, "15:00");
    assert!(decode_today_minutes(&request, &minutes(&vec![[1, 1, 1]; 241]), &metadata).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(decode_today_minutes(&request, &extra, &metadata).is_err());
    assert!(decode_today_minutes(&request, &minutes(&[[1, 1, -1]]), &metadata).is_err());
}

#[test]
fn minute_precision_uses_metadata_for_two_three_and_four_decimals() {
    let request = TodayMinuteRequest::new(0, "000001").unwrap();
    for decimal in [2, 3, 4] {
        let metadata =
            PrecisionSnapshot::from_decimals(&[(0, "000001", decimal)], chrono::Utc::now(), None)
                .unwrap();
        let series =
            decode_today_minutes(&request, &minutes(&[[12345, 1234500, 1]]), &metadata).unwrap();
        let divisor = 10f64.powi(i32::from(decimal));
        assert_eq!(series.points[0].price, 12345.0 / divisor);
        assert_eq!(series.points[0].average_price, 12345.0 / divisor);
    }
}

#[test]
fn client_defaults_load_new_directory_cache_and_only_use_0537_minutes() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let (config, handle) = server(1, move |request| {
        let command = u16::from_le_bytes([request[10], request[11]]);
        observed.lock().unwrap().push(command);
        match command {
            0x044e => vec![1, 0],
            0x044d => directory(&[("159915", 3)]),
            0x0537 => minutes(&[[3000, 300000, 10], [50, 500, 20]]),
            _ => panic!("unexpected {command:x}"),
        }
    });
    let mut client = Client::with_config(&config).unwrap();
    let rows = client.stocks(0).unwrap();
    assert_eq!(rows[0].pre_close, 3.625);
    let series = client.minute_series(0, "159915").unwrap();
    assert_eq!(series.metadata_generation, 1);
    assert_eq!(series.source_endpoint, config.ip);
    assert_eq!(client.minute(0, "159915").unwrap()[1].price, 3.05);
    assert_eq!(*calls.lock().unwrap(), [0x044e, 0x044d, 0x0537, 0x0537]);
    drop(client);
    handle.join().unwrap();
}

#[test]
fn new_directory_short_pages_continue_but_early_empty_and_duplicate_fail() {
    for mode in 0..3 {
        let (config, handle) = server(1, move |request| {
            if request[10] == 0x4e {
                return vec![2, 0];
            }
            assert_eq!(request[10], 0x4d);
            let start = u32::from_le_bytes(request[14..18].try_into().unwrap());
            directory(match (mode, start) {
                (_, 0) => &[("000001", 2)],
                (0, 1) => &[("000002", 2)],
                (1, 1) => &[],
                (2, 1) => &[("000001", 2)],
                _ => panic!("unexpected offset"),
            })
        });
        let mut client = Client::with_config(&config).unwrap();
        let result = client.security_directory(0);
        if mode == 0 {
            assert_eq!(result.unwrap().len(), 2);
        } else {
            assert!(result.is_err());
        }
        drop(client);
        handle.join().unwrap();
    }
}

#[test]
fn manual_heartbeat_returns_date_and_idle_worker_serializes_with_business() {
    let (observed, received) = mpsc::channel();
    let (config, handle) = server(1, move |request| {
        if request == HEARTBEAT_REQUEST {
            observed.send(4).unwrap();
            ack()
        } else {
            assert_eq!(request[10], 0x4e);
            std::thread::sleep(Duration::from_millis(40));
            vec![42, 0]
        }
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    let result = tcp.heartbeat().unwrap();
    assert_eq!(result.server_date_raw, 20261023);
    assert_eq!(result.reserved_raw, [1, 2, 3, 4, 5, 6]);
    assert_eq!(result.server_date.unwrap().to_string(), "2026-10-23");
    received.recv_timeout(Duration::from_secs(1)).unwrap();
    tcp.set_heartbeat_interval(Some(Duration::from_millis(10)))
        .unwrap();
    received.recv_timeout(Duration::from_secs(1)).unwrap(); // actual idle heartbeat
    for _ in 0..3 {
        assert_eq!(tcp.heartbeat_legacy().unwrap(), 42);
    }
    tcp.set_heartbeat_interval(None).unwrap();
    assert_eq!(tcp.heartbeat_legacy().unwrap(), 42);
    let start = Instant::now();
    drop(tcp);
    handle.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn idle_failure_invalidates_and_next_request_reconnects_without_mixing_sessions() {
    let (sent, received) = mpsc::channel();
    let (config, handle) = server(2, move |request| {
        if request == HEARTBEAT_REQUEST {
            sent.send(()).unwrap();
            vec![0; 2] // malformed ack
        } else {
            vec![42, 0]
        }
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    let connection = tcp.connection_id();
    tcp.set_heartbeat_interval(Some(Duration::from_millis(10)))
        .unwrap();
    received.recv_timeout(Duration::from_secs(1)).unwrap();
    let until = Instant::now() + Duration::from_secs(1);
    while tcp.is_reusable() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!tcp.is_reusable());
    // Join the failed worker so its shutdown has finished, rather than racing
    // the interval between setting `failed` and actually closing the socket.
    tcp.set_heartbeat_interval(None).unwrap();
    #[cfg(target_os = "macos")]
    assert!(tcp.get_ref().0.peer_addr().is_err());
    assert_eq!(tcp.heartbeat_legacy().unwrap(), 42);
    assert_ne!(connection, tcp.connection_id());
    tcp.set_heartbeat_interval(None).unwrap();
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn raw_extraction_stops_idle_worker_and_preserves_the_socket() {
    let (sent, received) = mpsc::channel();
    let (config, handle) = server(1, move |request| {
        assert_eq!(request, HEARTBEAT_REQUEST);
        sent.send(()).unwrap();
        // Exceed several I/O polls so extraction must finish the pending frame,
        // rather than aborting it when the idle loop is stopped.
        std::thread::sleep(Duration::from_millis(80));
        ack()
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(Some(Duration::from_millis(10)))
        .unwrap();
    received.recv_timeout(Duration::from_secs(1)).unwrap();
    let (mut stream, mut buffer, _) = tcp.into_inner();
    assert_eq!(stream.read_timeout().unwrap(), Some(config.timeout));
    assert_eq!(stream.write_timeout().unwrap(), Some(config.timeout));
    stream.write_all(HEARTBEAT_REQUEST).unwrap();
    let mut header = [0; 16];
    buffer.read_exact(&mut header).unwrap();
    let size = u16::from_le_bytes([header[12], header[13]]) as usize;
    let mut body = vec![0; size];
    buffer.read_exact(&mut body).unwrap();
    let decoded = tcp::decode_frame(&header, &body, true).unwrap();
    assert_eq!(
        tcp::decode_heartbeat(&decoded).unwrap().server_date_raw,
        20261023
    );
    drop(stream);
    drop(buffer);
    handle.join().unwrap();
}

#[test]
fn waiting_for_heartbeat_honors_business_deadline_and_drop_interrupts_worker() {
    let (sent, received) = mpsc::channel();
    let (config, handle) = server(1, move |request| {
        assert_eq!(request, HEARTBEAT_REQUEST);
        sent.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        ack()
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(Some(Duration::from_millis(10)))
        .unwrap();
    received.recv_timeout(Duration::from_secs(1)).unwrap();
    let start = Instant::now();
    let error = tcp
        .with_timeout(Duration::from_millis(30), Tcp::heartbeat_legacy)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_millis(150));
    let start = Instant::now();
    drop(tcp);
    assert!(start.elapsed() < Duration::from_millis(150));
    handle.join().unwrap();
}

#[test]
fn enhanced_history_event_model_replays_real_wire_without_losing_new_slots() {
    let query = TradeQuery::history(0, "300308", 20261023).unwrap();
    let request = rustdx_complete::tcp::stock::EnhancedHistoryTransactionRequest::new(
        0, "300308", 0, 1800, 20261023,
    )
    .unwrap();
    let payload = include_bytes!("fixtures/protocol/enhanced-history-0fc6.payload.bin");
    let rich = decode_trade_page(&query, 0, 1800, payload).unwrap();
    let enhanced =
        rustdx_complete::tcp::stock::decode_enhanced_history_transactions(&request, payload)
            .unwrap();
    assert_eq!(rich.query().protocol(), TradeProtocol::EnhancedHistory0fc6);
    assert_eq!(rich.raw_count(), 1800);
    assert_eq!(rich.raw_count(), enhanced.raw_count());
    for (new, old) in rich.records().iter().zip(&enhanced.records) {
        assert_eq!(new.price_raw(), old.price_acc_raw);
        assert_eq!(new.trade_count_raw(), Some(old.order_count));
        assert_eq!(new.status_raw(), old.status_raw);
        assert_eq!(new.tail_raw(), old.reserved_raw);
    }
}
