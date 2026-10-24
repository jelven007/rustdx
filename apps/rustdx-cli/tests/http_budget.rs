use rustdx_cli::http::{self, HttpOperation};
use rustdx_complete::operation::{self, OperationContext};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

fn slow_response(delays: &[u64]) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/test", listener.local_addr().unwrap());
    let delays = delays.to_vec();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
        }
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            delays.len()
        )
        .unwrap();
        for delay in delays {
            std::thread::sleep(Duration::from_millis(delay));
            if socket.write_all(b"x").is_err() {
                break;
            }
        }
    });
    (url, server)
}

fn read(url: &str) -> eyre::Result<String> {
    let mut operation = HttpOperation::new(Duration::from_secs(5))?;
    let mut response = http::get(url)
        .config()
        .timeout_global(Some(operation.timeout()))
        .build()
        .call()?;
    let text = http::text(response.body_mut(), 100)?;
    operation.complete(text.is_empty())?;
    Ok(text)
}

#[test]
fn slow_progress_does_not_refresh_deadline_and_idle_body_can_be_cancelled() {
    let context = OperationContext::new(Duration::from_millis(100), None).unwrap();
    let guard = context.enter();
    let (url, server) = slow_response(&[40; 20]);
    let start = Instant::now();
    assert!(read(&url).is_err());
    assert!(start.elapsed() < Duration::from_millis(400));
    drop(guard);
    server.join().unwrap();
    let context = OperationContext::new(Duration::from_secs(5), None).unwrap();
    let guard = context.enter();
    let signal = context.clone();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        signal.cancel();
    });
    let (url, server) = slow_response(&[600]);
    let start = Instant::now();
    assert!(read(&url).is_err());
    assert!(start.elapsed() < Duration::from_millis(400));
    cancel.join().unwrap();
    drop(guard);
    server.join().unwrap();
    assert_eq!(rustdx_complete::tcp::network_stats().active, 0);
    operation::check().unwrap();
    // The same process-wide counters must not be asserted while another test
    // owns a healthy HTTP connection.
    let (url, server) = slow_response(&[240; 5]);
    assert_eq!(read(&url).unwrap(), "xxxxx");
    server.join().unwrap();
    assert_eq!(rustdx_complete::tcp::network_stats().active, 0);
}
