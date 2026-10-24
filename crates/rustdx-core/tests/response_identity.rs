//! Synthetic transport faults over unchanged, captured decoder payloads.
use rustdx_complete::{
    pool::{ConnectionPool, PoolConfig},
    tcp::{
        FrameEvent, FrameObserver, HEARTBEAT_REQUEST, PACK1, SecurityCount, Tcp, TcpConfig, Tdx,
        decode_response, network_stats,
        stock::{
            EnhancedHistoryTransactionRequest, FinanceRequest, HistoryTransactionRequest,
            KlineRequest, PrecisionSnapshot, QuotesRequest, TransactionRequest,
        },
    },
};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[path = "common/ip_pool.rs"]
mod ip_pool;
#[path = "common/extended_protocol.rs"]
mod protocol;

static SERIAL: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug)]
enum Query {
    StockBars,
    IndexBars,
    Quotes,
    Finance,
    Transactions,
    EnhancedHistory,
    HistoryTransactions,
}

impl Query {
    fn request(self) -> Vec<u8> {
        match self {
            Self::StockBars => KlineRequest::new(0, "000001", 9, 0, 800)
                .unwrap()
                .encode()
                .to_vec(),
            Self::IndexBars => KlineRequest::new(1, "000001", 9, 0, 800)
                .unwrap()
                .encode()
                .to_vec(),
            Self::Quotes => QuotesRequest::new(vec![(0, "000001"), (1, "600000")])
                .unwrap()
                .encode(),
            Self::Finance => FinanceRequest::new(0, "000001").unwrap().encode().to_vec(),
            Self::Transactions => TransactionRequest::new(0, "000001", 0, 3)
                .unwrap()
                .encode()
                .to_vec(),
            Self::EnhancedHistory => {
                EnhancedHistoryTransactionRequest::new(0, "000001", 0, 3, 20261022)
                    .unwrap()
                    .encode()
                    .to_vec()
            }
            Self::HistoryTransactions => {
                HistoryTransactionRequest::new(0, "000001", 0, 3, 20261022)
                    .unwrap()
                    .encode()
                    .to_vec()
            }
        }
    }

    fn payload(self) -> Vec<u8> {
        let relative = match self {
            Self::StockBars => "kline/stock-day",
            Self::IndexBars => "kline/index-day",
            Self::Quotes => "protocol/quotes",
            Self::Finance => "protocol/finance",
            Self::Transactions => "protocol/transaction",
            Self::EnhancedHistory => {
                return [
                    3u16.to_le_bytes().to_vec(),
                    11.3f32.to_le_bytes().to_vec(),
                    Self::Transactions.payload()[2..].to_vec(),
                ]
                .concat();
            }
            Self::HistoryTransactions => "protocol/history-transaction",
        };
        let hex = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(format!("{relative}.response.hex")),
        )
        .unwrap();
        hex.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn recv(self, tcp: &mut Tcp) -> io::Result<usize> {
        match self {
            Self::StockBars => KlineRequest::new(0, "000001", 9, 0, 800)?
                .recv_stock(tcp)
                .map(|v| v.len()),
            Self::IndexBars => KlineRequest::new(1, "000001", 9, 0, 800)?
                .recv_index(tcp)
                .map(|v| v.len()),
            Self::Quotes => {
                let precision = PrecisionSnapshot::from_decimals(
                    &[(0, "000001", 2), (1, "600000", 2)],
                    chrono::Utc::now(),
                    None,
                )?;
                QuotesRequest::new(vec![(0, "000001"), (1, "600000")])?
                    .recv(tcp, &precision)
                    .map(|v| v.len())
            }
            Self::Finance => FinanceRequest::new(0, "000001")?.recv(tcp).map(|v| v.len()),
            Self::Transactions => TransactionRequest::new(0, "000001", 0, 3)?
                .recv(tcp)
                .map(|v| v.len()),
            Self::EnhancedHistory => {
                EnhancedHistoryTransactionRequest::new(0, "000001", 0, 3, 20261022)?
                    .recv(tcp)
                    .map(|page| page.records.len())
            }
            Self::HistoryTransactions => {
                HistoryTransactionRequest::new(0, "000001", 0, 3, 20261022)?
                    .recv(tcp)
                    .map(|v| v.len())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    None,
    Prefix,
    Opcode,
    Message,
}

fn response(request: &[u8], payload: &[u8], compressed: bool, fault: Fault) -> Vec<u8> {
    let mut reply = protocol::response(request, payload);
    if compressed {
        let wire = miniz_oxide::deflate::compress_to_vec_zlib(payload, 6);
        assert_ne!(wire.len(), payload.len());
        reply[12..14].copy_from_slice(&(wire.len() as u16).to_le_bytes());
        reply.truncate(16);
        reply.extend(wire);
    }
    match fault {
        Fault::None => {}
        Fault::Prefix => reply[0] ^= 1,
        Fault::Opcode => reply[10] ^= 1,
        Fault::Message => reply[5] ^= 1,
    }
    reply
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 10];
    stream.read_exact(&mut header).unwrap();
    let size = usize::from(u16::from_le_bytes([header[6], header[7]]));
    let mut request = header.to_vec();
    request.resize(10 + size, 0);
    stream.read_exact(&mut request[10..]).unwrap();
    request
}

fn assert_closed(stream: &mut TcpStream) {
    match stream.read(&mut [0]) {
        Ok(0) => {}
        // Rejecting a header before its body is consumed may reset the socket.
        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
        result => panic!("client did not close the rejected connection: {result:?}"),
    }
}

fn server(
    expected: Vec<u8>,
    payload: Vec<u8>,
    compressed: bool,
    fault: Fault,
) -> (TcpConfig, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool::prepare(&listener)),
        timeout: Duration::from_millis(500),
        auto_reconnect: 1,
        ..Default::default()
    };
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let hello = read_request(&mut stream);
        assert_eq!(hello, PACK1);
        stream
            .write_all(&response(&hello, &[0, 0], false, Fault::None))
            .unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request, expected);
        stream
            .write_all(&response(&request, &payload, compressed, fault))
            .unwrap();
        // Keep the source alive so an EOF cannot substitute for the identity error.
        assert_closed(&mut stream);
    });
    (config, handle)
}

#[test]
fn legacy_queries_reject_mismatched_identity_with_valid_payloads() {
    let _serial = SERIAL.lock().unwrap();
    for query in [
        Query::StockBars,
        Query::IndexBars,
        Query::Quotes,
        Query::Finance,
        Query::Transactions,
        Query::EnhancedHistory,
        Query::HistoryTransactions,
    ] {
        for compressed in [false, true] {
            for fault in [Fault::None, Fault::Prefix, Fault::Opcode, Fault::Message] {
                let (config, handle) = server(query.request(), query.payload(), compressed, fault);
                let mut tcp = Tcp::with_config(&config).unwrap();
                tcp.set_heartbeat_interval(None).unwrap();
                let connection = tcp.connection_id();
                let result = query.recv(&mut tcp);
                if fault == Fault::None {
                    assert!(result.unwrap() > 0, "{query:?}");
                    assert!(tcp.is_reusable());
                } else {
                    assert_eq!(
                        result.unwrap_err().kind(),
                        io::ErrorKind::InvalidData,
                        "{query:?}, {fault:?}, compressed={compressed}"
                    );
                    assert!(!tcp.is_reusable());
                    assert_eq!(
                        tcp.connection_id(),
                        connection,
                        "identity failure must not retry"
                    );
                }
                drop(tcp);
                handle.join().unwrap();
            }
        }
    }
}

#[test]
fn invalid_responses_are_not_returned_to_the_connection_pool() {
    let _serial = SERIAL.lock().unwrap();
    let query = Query::Transactions;
    let (config, handle) = server(query.request(), query.payload(), false, Fault::Message);
    let pool = ConnectionPool::with_tcp_config(
        PoolConfig {
            max_size: 1,
            max_idle: 1,
            ..Default::default()
        },
        config,
    )
    .unwrap();
    let mut connection = pool.get_connection().unwrap();
    connection.get_mut().set_heartbeat_interval(None).unwrap();
    assert_eq!(
        connection
            .execute(|tcp| query.recv(tcp))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    drop(connection);
    assert_eq!(pool.stats().idle, 0);
    assert_eq!(pool.stats().active, 0);
    handle.join().unwrap();
}

#[test]
fn header_only_and_raw_frame_calls_reject_identity_without_retrying() {
    let _serial = SERIAL.lock().unwrap();
    for header_only in [false, true] {
        for fault in [Fault::Prefix, Fault::Opcode, Fault::Message] {
            let request = TransactionRequest::new(0, "000001", 0, 1).unwrap().encode();
            let (config, handle) = server(request.to_vec(), vec![0, 0], false, fault);
            let mut tcp = Tcp::with_config(&config).unwrap();
            tcp.set_heartbeat_interval(None).unwrap();
            let result = if header_only {
                tcp.send_recv(&request).map(|_| ())
            } else {
                rustdx_complete::tcp::send_recv(&mut tcp, &request, "raw identity").map(|_| ())
            };
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
            assert!(!tcp.is_reusable());
            drop(tcp);
            handle.join().unwrap();
        }
    }
    let query = Query::Transactions;
    let (config, handle) = server(query.request(), query.payload(), false, Fault::Message);
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(None).unwrap();
    assert_eq!(
        tcp.retry(|tcp| query.recv(tcp), 3).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn handshake_and_daily_probes_reject_invalid_identity() {
    let _serial = SERIAL.lock().unwrap();
    for fault in [Fault::Prefix, Fault::Opcode, Fault::Message] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = TcpConfig {
            ip: Some(listener.local_addr().unwrap()),
            ip_pool: Some(ip_pool::prepare(&listener)),
            ..Default::default()
        };
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let hello = read_request(&mut stream);
            stream
                .write_all(&response(&hello, &[0, 0], false, fault))
                .unwrap();
            assert_closed(&mut stream);
        });
        let active = network_stats().active;
        assert_eq!(
            Tcp::with_config(&config).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(network_stats().active, active);
        handle.join().unwrap();
    }
    for handshake in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let pool =
            rustdx_complete::tcp::ip::IpPool::new(vec![listener.local_addr().unwrap()]).unwrap();
        let handle = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let hello = read_request(&mut stream);
                stream
                    .write_all(&response(
                        &hello,
                        &[0, 0],
                        false,
                        if handshake {
                            Fault::Message
                        } else {
                            Fault::None
                        },
                    ))
                    .unwrap();
                if !handshake {
                    let request = read_request(&mut stream);
                    assert_eq!(request, SecurityCount::SEND);
                    stream
                        .write_all(&response(&request, &[123, 0], false, Fault::Opcode))
                        .unwrap();
                }
            }
        });
        let daily = pool.refresh().unwrap();
        assert!(daily.valid_nodes.is_empty());
        assert!(daily.results[0].rounds.iter().all(|round| {
            round
                .failure
                .as_ref()
                .is_some_and(|failure| failure.kind == "InvalidData")
        }));
        handle.join().unwrap();
    }
}

#[derive(Debug, Default)]
struct Events {
    order: Mutex<Vec<String>>,
    response: Mutex<Option<([u8; 16], Vec<u8>)>>,
}

impl FrameObserver for Events {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        let value = match event {
            FrameEvent::Request { .. } => "request".into(),
            FrameEvent::Response { header, body, .. } => {
                *self.response.lock().unwrap() = Some((*header, body.to_vec()));
                "response".into()
            }
            FrameEvent::Failed { kind, .. } => format!("failed:{kind:?}"),
        };
        self.order.lock().unwrap().push(value);
        Ok(())
    }
}

#[test]
fn observer_retains_bad_frame_and_reports_failure_before_success_stats() {
    let _serial = SERIAL.lock().unwrap();
    let query = Query::Transactions;
    let (config, handle) = server(query.request(), query.payload(), true, Fault::Message);
    let observer = Arc::new(Events::default());
    let mut tcp = Tcp::with_config_and_observer(&config, observer.clone()).unwrap();
    tcp.set_heartbeat_interval(None).unwrap();
    let before = network_stats();
    assert!(query.recv(&mut tcp).is_err());
    let after = network_stats();
    assert_eq!(after.requests, before.requests + 1);
    assert_eq!(after.request_failures, before.request_failures + 1);
    assert_eq!(
        *observer.order.lock().unwrap(),
        [
            "request",
            "response",
            "request",
            "response",
            "failed:InvalidData"
        ]
    );
    let captured = observer.response.lock().unwrap();
    let (header, body) = captured.as_ref().unwrap();
    assert_eq!(
        *header,
        response(&query.request(), &query.payload(), true, Fault::Message)[..16]
    );
    assert_eq!(
        decode_response(&query.request(), header, body, true)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn idle_heartbeat_invalidates_on_mismatched_identity() {
    let _serial = SERIAL.lock().unwrap();
    let mut ack = vec![0; 6];
    ack.extend_from_slice(&20261022u32.to_le_bytes());
    let (config, handle) = server(HEARTBEAT_REQUEST.to_vec(), ack, false, Fault::Opcode);
    let mut tcp = Tcp::with_config(&config).unwrap();
    tcp.set_heartbeat_interval(Some(Duration::from_millis(10)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while tcp.is_reusable() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!tcp.is_reusable());
    drop(tcp);
    handle.join().unwrap();
}

#[test]
fn offline_replay_matches_identity_before_decompression() {
    let query = Query::Transactions;
    let request = query.request();
    let payload = query.payload();
    for fault in [Fault::None, Fault::Prefix, Fault::Opcode, Fault::Message] {
        let wire = response(&request, &payload, true, fault);
        let header: [u8; 16] = wire[..16].try_into().unwrap();
        let result = decode_response(&request, &header, &wire[16..], true);
        if fault == Fault::None {
            assert_eq!(result.unwrap(), payload);
        } else {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
    }
    let mut wire = response(&request, &payload, true, Fault::Message);
    wire[16..].fill(0xff);
    let header: [u8; 16] = wire[..16].try_into().unwrap();
    assert!(
        decode_response(&request, &header, &wire[16..], true)
            .unwrap_err()
            .to_string()
            .contains("消息号")
    );
}

#[test]
fn captured_extended_response_headers_match_the_shared_replay_contract() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/protocol_7709");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for name in manifest["fixtures"].as_object().unwrap().keys() {
        let request = std::fs::read(root.join(format!("{name}.request.bin"))).unwrap();
        let header: [u8; 16] = std::fs::read(root.join(format!("{name}.header.bin")))
            .unwrap()
            .try_into()
            .unwrap();
        let wire = std::fs::read(root.join(format!("{name}.wire.bin"))).unwrap();
        let payload = std::fs::read(root.join(format!("{name}.payload.bin"))).unwrap();
        assert_eq!(
            decode_response(&request, &header, &wire, true).unwrap(),
            payload,
            "{name}"
        );
    }
}

#[test]
fn truncated_bad_frame_is_an_identity_failure_not_a_retryable_transport_error() {
    let _serial = SERIAL.lock().unwrap();
    for observed in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = TcpConfig {
            ip: Some(listener.local_addr().unwrap()),
            ip_pool: Some(ip_pool::prepare(&listener)),
            auto_reconnect: 1,
            timeout: Duration::from_millis(200),
            ..Default::default()
        };
        let query = Query::Transactions;
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let hello = read_request(&mut stream);
            stream
                .write_all(&response(&hello, &[0, 0], false, Fault::None))
                .unwrap();
            let request = read_request(&mut stream);
            let wire = response(&request, &query.payload(), false, Fault::Message);
            stream.write_all(&wire[..17]).unwrap();
        });
        let observer = Arc::new(Events::default());
        let mut tcp = if observed {
            Tcp::with_config_and_observer(&config, observer.clone()).unwrap()
        } else {
            Tcp::with_config(&config).unwrap()
        };
        tcp.set_heartbeat_interval(None).unwrap();
        let id = tcp.connection_id();
        let error = query.recv(&mut tcp).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("消息号"));
        assert_eq!(tcp.connection_id(), id);
        assert!(!tcp.is_reusable());
        if observed {
            assert_eq!(
                *observer.order.lock().unwrap(),
                ["request", "response", "request", "failed:InvalidData"]
            );
        }
        drop(tcp);
        handle.join().unwrap();
    }
}
