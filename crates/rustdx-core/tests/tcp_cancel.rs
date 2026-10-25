use rustdx_complete::{
    operation::OperationContext,
    tcp::{Tcp, TcpConfig},
};
use std::{
    io::{ErrorKind, Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

#[path = "common/ip_pool.rs"]
mod ip_pool_test;

#[test]
fn idle_tcp_response_is_cancelled_before_request_timeout() {
    // Run sequentially: both scenarios verify that the global permit returns
    // to zero, and neither cancellation nor expiry may be reported as peer EOF.
    for cancel in [true, false] {
        idle_response_returns_before_peer_closes(cancel);
    }
}

fn idle_response_returns_before_peer_closes(cancel: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ip_pool: Some(ip_pool_test::prepare(&listener)),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut handshake = [0; 13];
        socket.read_exact(&mut handshake).unwrap();
        let mut frame = [0; 18];
        frame[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
        frame[5..9].copy_from_slice(&handshake[1..5]);
        frame[10..12].copy_from_slice(&handshake[10..12]);
        frame[12..14].copy_from_slice(&2u16.to_le_bytes());
        frame[14..16].copy_from_slice(&2u16.to_le_bytes());
        socket.write_all(&frame).unwrap();
        let mut request = [0; 12];
        socket.read_exact(&mut request).unwrap();
        std::thread::sleep(Duration::from_millis(600));
    });
    let timeout = if cancel {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(50)
    };
    let mut tcp = Tcp::with_config(&config).unwrap();
    let context = OperationContext::new(timeout, None).unwrap();
    let _guard = context.enter();
    let signal = context.clone();
    let cancellation = cancel.then(|| {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            signal.cancel();
        })
    });
    let start = Instant::now();
    let error = tcp.heartbeat().unwrap_err();
    assert_eq!(
        error.kind(),
        if cancel {
            ErrorKind::Interrupted
        } else {
            ErrorKind::TimedOut
        }
    );
    assert!(start.elapsed() < Duration::from_millis(400));
    if let Some(cancellation) = cancellation {
        cancellation.join().unwrap();
    }
    drop(tcp);
    server.join().unwrap();
    assert_eq!(rustdx_complete::tcp::network_stats().active, 0);
}
