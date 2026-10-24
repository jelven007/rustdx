//! 使用本地协议服务验证断线恢复与安全解码。
use rustdx_complete::tcp::stock::{Client, Kline, SecurityQuotes};
use rustdx_complete::tcp::{PACK1, Tcp, TcpConfig, Tdx};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

fn frame(stream: &mut std::net::TcpStream, request: &[u8], body: &[u8]) {
    let mut header = [0; 16];
    header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
    header[5..9].copy_from_slice(&request[1..5]);
    header[10..12].copy_from_slice(&request[10..12]);
    header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
    header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
    stream.write_all(&header).unwrap();
    stream.write_all(body).unwrap();
}

#[test]
fn reconnects_once_after_truncated_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        timeout: Duration::from_secs(2),
        auto_reconnect: 1,
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = vec![0; PACK1.len()];
            stream.read_exact(&mut bytes).unwrap();
            assert_eq!(bytes, PACK1);
            frame(&mut stream, &bytes, &[0, 0]);
            let mut request = [0; 18];
            stream.read_exact(&mut request).unwrap();
            if attempt == 0 {
                let mut header = [0; 16];
                header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
                header[5..9].copy_from_slice(&request[1..5]);
                header[10..12].copy_from_slice(&request[10..12]);
                header[12..14].copy_from_slice(&2u16.to_le_bytes());
                header[14..16].copy_from_slice(&2u16.to_le_bytes());
                stream.write_all(&header).unwrap();
                stream.write_all(&[1]).unwrap();
            } else {
                frame(&mut stream, &request, &123u16.to_le_bytes());
            }
        }
    });
    let mut tcp = Tcp::with_config(&config).unwrap();
    assert_eq!(tcp.heartbeat_legacy().unwrap(), 123);
    assert!(tcp.is_reusable());
    server.join().unwrap();
}

#[test]
fn reconnect_handshake_failure_consumes_one_attempt_and_can_recover() {
    for explicit_retry in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = TcpConfig {
            ip: Some(listener.local_addr().unwrap()),
            ip_pool: Some(ip_pool_test::prepare(&listener)),
            timeout: Duration::from_secs(2),
            auto_reconnect: if explicit_retry { 0 } else { 2 },
            retry_delay_ms: 0,
            ..Default::default()
        };
        let server = std::thread::spawn(move || {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = vec![0; PACK1.len()];
                stream.read_exact(&mut bytes).unwrap();
                if attempt == 1 {
                    continue; // 第一次重连的握手中断。
                }
                frame(&mut stream, &bytes, &[0, 0]);
                let mut request = [0; 18];
                stream.read_exact(&mut request).unwrap();
                if attempt == 2 {
                    frame(&mut stream, &request, &123u16.to_le_bytes());
                } // 首次请求中断，第二次重连成功。
            }
        });
        let mut tcp = Tcp::with_config(&config).unwrap();
        let result = if explicit_retry {
            tcp.retry(Tcp::heartbeat_legacy, 3)
        } else {
            tcp.heartbeat_legacy()
        };
        assert_eq!(result.unwrap(), 123);
        assert!(tcp.is_reusable());
        server.join().unwrap();
    }
}

#[test]
fn rejects_every_truncated_quote_prefix_without_panic() {
    let hex = "0114010000303030303031fe08a61242440a45bb9fe30ae612a9853d0d31160e4e91981e98ed1e00bfaf0700019f12074102a724ab3b4203911295164304a8288a234405a33296457501000000000000fe08";
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let precision = rustdx_complete::tcp::stock::PrecisionSnapshot::from_decimals(
        &[(0, "000001", 2)],
        chrono::Utc::now(),
        None,
    )
    .unwrap();
    let mut request = SecurityQuotes::default().with_precision(std::sync::Arc::new(precision));
    for n in 0..bytes.len() {
        assert!(
            request.try_parse(bytes[..n].to_vec()).is_err(),
            "prefix {n}"
        );
        assert!(request.result().is_empty());
    }
    request.try_parse(bytes).unwrap();
    assert_eq!(request.result()[0].price, 11.9);
}

#[test]
fn kline_recovers_after_empty_and_malformed_responses() {
    let mut request = Kline::new(0, "000001", 9, 0, 1);
    request.try_parse(vec![0, 0]).unwrap();
    assert!(request.try_parse(vec![1, 0, 0]).is_err());
    let mut response = vec![1, 0];
    response.extend_from_slice(&20260901u32.to_le_bytes());
    response.extend_from_slice(&[1, 0, 0, 0]);
    response.extend_from_slice(&[0; 8]);
    request.try_parse(response).unwrap();
    assert_eq!(request.result().len(), 1);
}

#[test]
fn invalid_client_inputs_return_errors() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = vec![0; PACK1.len()];
        stream.read_exact(&mut bytes).unwrap();
        frame(&mut stream, &bytes, &[0, 0]);
    });
    let mut client = Client::with_config(&config).unwrap();
    assert!(client.quotes(&[]).unwrap().is_empty());
    assert!(client.k(0, "坏代码", None, None).is_err());
    assert!(client.k(0, "000001", Some(20260230), None).is_err());
    assert!(client.bars(0, "000001", 99, 0, 1).is_err());
    server.join().unwrap();
}

#[test]
fn incremental_factor_preserves_previous_state_without_events() {
    use rustdx_complete::file::{day::Day, gbbq::Fq};
    let day = Day {
        date: 20260901,
        code: "sh600000".into(),
        open: 11.0,
        high: 11.0,
        low: 11.0,
        close: 11.0,
        vol: 100,
        amount: 1100.0,
    };
    let factors = Fq::concat(vec![day].into_iter(), &[], 10.0, 10.0).unwrap();
    assert!((factors[0].factor - 11.0).abs() < 1e-10);
}
