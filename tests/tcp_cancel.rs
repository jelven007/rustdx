use rustdx_complete::{
    operation::OperationContext,
    tcp::{Tcp, TcpConfig},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

#[test]
fn idle_tcp_response_is_cancelled_before_request_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
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
        frame[12..14].copy_from_slice(&2u16.to_le_bytes());
        frame[14..16].copy_from_slice(&2u16.to_le_bytes());
        socket.write_all(&frame).unwrap();
        let mut request = [0; 18];
        socket.read_exact(&mut request).unwrap();
        std::thread::sleep(Duration::from_millis(600));
    });
    let context = OperationContext::new(Duration::from_secs(5), None).unwrap();
    let _guard = context.enter();
    let mut tcp = Tcp::with_config(&config).unwrap();
    let signal = context.clone();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        signal.cancel();
    });
    let start = Instant::now();
    assert!(tcp.heartbeat().is_err());
    assert!(start.elapsed() < Duration::from_millis(400));
    cancel.join().unwrap();
    drop(tcp);
    server.join().unwrap();
    assert_eq!(rustdx_complete::tcp::network_stats().active, 0);
}
