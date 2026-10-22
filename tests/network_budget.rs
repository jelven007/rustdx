//! 独立测试进程，验证主连接、池与批次共同遵守全局预算。
use rustdx_complete::{
    pool::{ConnectionPool, PoolConfig},
    tcp::{NetworkPolicy, TcpConfig, configure_network, network_stats, stock::Client},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[test]
fn managed_connections_share_capacity_and_absolute_deadlines() {
    configure_network(NetworkPolicy {
        max_connections: 2,
        acquire_timeout: Duration::from_millis(25),
        request_timeout: Duration::from_millis(80),
        ..Default::default()
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = TcpConfig {
        ip: Some(listener.local_addr().unwrap()),
        ..Default::default()
    };
    let done = Arc::new(AtomicBool::new(false));
    let slow = Arc::new(AtomicBool::new(false));
    let server = {
        let done = done.clone();
        let slow = slow.clone();
        std::thread::spawn(move || {
            std::thread::scope(|scope| {
                while !done.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let slow = slow.clone();
                            scope.spawn(move || {
                                // macOS 会继承 listener 的非阻塞标记。
                                stream.set_nonblocking(false).unwrap();
                                stream
                                    .set_read_timeout(Some(Duration::from_secs(1)))
                                    .unwrap();
                                loop {
                                    let mut header = [0u8; 10];
                                    if stream.read_exact(&mut header).is_err() {
                                        break;
                                    }
                                    let size = u16::from_le_bytes([header[6], header[7]]) as usize;
                                    let mut request = vec![0; size];
                                    if stream.read_exact(&mut request).is_err() {
                                        break;
                                    }
                                    let trickle = slow.load(Ordering::Relaxed)
                                        && request.first() == Some(&0x4e);
                                    let mut reply = [0u8; 16];
                                    let body = if trickle { vec![0; 16] } else { vec![0; 2] };
                                    reply[12..14]
                                        .copy_from_slice(&(body.len() as u16).to_le_bytes());
                                    reply[14..16]
                                        .copy_from_slice(&(body.len() as u16).to_le_bytes());
                                    if stream.write_all(&reply).is_err() {
                                        break;
                                    }
                                    if trickle {
                                        for byte in body {
                                            std::thread::sleep(Duration::from_millis(20));
                                            if stream.write_all(&[byte]).is_err() {
                                                break;
                                            }
                                        }
                                    } else if stream.write_all(&body).is_err() {
                                        break;
                                    }
                                }
                            });
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("{error}"),
                    }
                }
            })
        })
    };
    let mut client = Client::with_config(&config).unwrap();
    let pool = ConnectionPool::with_tcp_config(
        PoolConfig {
            max_size: 2,
            max_idle: 0,
            health_check_timeout: 0,
            ..Default::default()
        },
        config,
    )
    .unwrap();
    let mut connection = pool.get_connection().unwrap();
    assert_eq!(network_stats().active, 2);
    let start = Instant::now();
    assert_eq!(
        pool.get_connection_timeout(Duration::from_millis(10))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(start.elapsed() < Duration::from_millis(250));
    connection.get_mut().reconnect().unwrap();
    assert_eq!((network_stats().active, network_stats().peak), (2, 2));
    drop(connection);
    let stalled = TcpListener::bind("127.0.0.1:0").unwrap();
    let stalled_config = TcpConfig {
        ip: Some(stalled.local_addr().unwrap()),
        ..Default::default()
    };
    let stalled_server = std::thread::spawn(move || {
        let (stream, _) = stalled.accept().unwrap();
        std::thread::sleep(Duration::from_millis(60));
        drop(stream);
    });
    let stalled_pool = ConnectionPool::with_tcp_config(
        PoolConfig {
            health_check_timeout: 0,
            ..Default::default()
        },
        stalled_config,
    )
    .unwrap();
    let start = Instant::now();
    assert!(
        stalled_pool
            .get_connection_timeout(Duration::from_millis(20))
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_millis(200));
    assert_eq!(
        (stalled_pool.stats().active, network_stats().active),
        (0, 1)
    );
    stalled_server.join().unwrap();
    for _ in 0..2 {
        let rows = client
            .k_batch(&[(0, "000001"), (1, "600000")], None, None, 8)
            .unwrap();
        assert!(
            rows.iter()
                .all(|row| row.result.as_ref().unwrap().is_empty())
        );
        assert_eq!(network_stats().active, 1);
    }
    slow.store(true, Ordering::Relaxed);
    let start = Instant::now();
    assert!(client.heartbeat().is_err());
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "逐字节正文必须受总体时限约束"
    );
    assert!(!client.tcp.is_reusable());
    assert!(network_stats().request_failures > 0);
    drop(client);
    pool.close();
    assert_eq!(network_stats().active, 0);
    done.store(true, Ordering::Relaxed);
    server.join().unwrap();
}
