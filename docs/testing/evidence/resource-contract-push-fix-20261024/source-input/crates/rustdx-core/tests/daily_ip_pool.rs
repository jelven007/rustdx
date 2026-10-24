//! 真实本地协议服务验证启动补测、三轮筛选、八并发与业务连接统一选址。
use rustdx_complete::{
    pool::{ConnectionPool, PoolConfig},
    tcp::{
        Tcp, TcpConfig,
        ip::{IpPool, ProbeStage},
    },
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Default)]
struct Meter {
    active: AtomicUsize,
    peak: AtomicUsize,
}

struct Server {
    address: SocketAddr,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new(count: u16, delay: Duration, flaky: bool, meter: Arc<Meter>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicUsize::new(0));
        let worker = {
            let stop = stop.clone();
            let connections = connections.clone();
            thread::spawn(move || {
                thread::scope(|scope| {
                    while !stop.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((mut stream, _)) => {
                                stream.set_nonblocking(false).unwrap();
                                stream
                                    .set_read_timeout(Some(Duration::from_millis(200)))
                                    .unwrap();
                                let number = connections.fetch_add(1, Ordering::SeqCst) + 1;
                                let meter = meter.clone();
                                let stop = stop.clone();
                                scope.spawn(move || {
                                    while !stop.load(Ordering::Relaxed) {
                                        let mut header = [0; 10];
                                        if stream.read_exact(&mut header).is_err() {
                                            break;
                                        }
                                        let length =
                                            u16::from_le_bytes([header[6], header[7]]) as usize;
                                        let mut request = vec![0; length];
                                        if stream.read_exact(&mut request).is_err() {
                                            break;
                                        }
                                        let active =
                                            meter.active.fetch_add(1, Ordering::SeqCst) + 1;
                                        meter.peak.fetch_max(active, Ordering::SeqCst);
                                        thread::sleep(delay);
                                        let body = if flaky && number == 2 && request[0] == 0x4e {
                                            vec![1] // 心跳内容不够两字节，TCP和握手仍然成功。
                                        } else if request[0] == 4 {
                                            vec![0; 10]
                                        } else {
                                            count.to_le_bytes().to_vec()
                                        };
                                        let mut full_request = header.to_vec();
                                        full_request.extend_from_slice(&request);
                                        let result = frame(&mut stream, &full_request, &body);
                                        meter.active.fetch_sub(1, Ordering::SeqCst);
                                        if result.is_err() {
                                            break;
                                        }
                                    }
                                });
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(1));
                            }
                            Err(error) => panic!("{error}"),
                        }
                    }
                });
            })
        };
        Self {
            address,
            connections,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn frame(stream: &mut TcpStream, request: &[u8], body: &[u8]) -> std::io::Result<()> {
    let mut header = [0; 16];
    header[..4].copy_from_slice(&[0xb1, 0xcb, 0x74, 0]);
    header[5..9].copy_from_slice(&request[1..5]);
    header[10..12].copy_from_slice(&request[10..12]);
    header[12..14].copy_from_slice(&(body.len() as u16).to_le_bytes());
    header[14..16].copy_from_slice(&(body.len() as u16).to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(body)
}

fn config(pool: &IpPool) -> TcpConfig {
    TcpConfig {
        ip_pool: Some(pool.clone()),
        timeout: Duration::from_secs(1),
        ..Default::default()
    }
}

#[test]
fn filters_a_flaky_and_refused_node_and_ranks_only_three_round_successes() {
    let meter = Arc::new(Meter::default());
    let fast = Server::new(11, Duration::ZERO, false, meter.clone());
    let slow = Server::new(22, Duration::from_millis(40), false, meter.clone());
    let flaky = Server::new(33, Duration::ZERO, true, meter);
    let refused = TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_address = refused.local_addr().unwrap();
    drop(refused);
    let pool = IpPool::new(vec![
        flaky.address,
        slow.address,
        fast.address,
        refused_address,
    ])
    .unwrap();
    let daily = pool.refresh().unwrap();
    assert_eq!(daily.valid_nodes, [fast.address, slow.address]);
    assert!(daily.results.iter().all(|node| node.rounds.len() == 3));
    let flaky = &daily.results[0];
    assert_eq!(
        flaky.rounds.iter().filter(|round| round.passed()).count(),
        2
    );
    assert_eq!(
        flaky.rounds[1].failure.as_ref().unwrap().stage,
        ProbeStage::Heartbeat
    );
    assert!(daily.results[3].rounds.iter().all(|round| {
        round
            .failure
            .as_ref()
            .is_some_and(|error| error.stage == ProbeStage::Tcp)
    }));
    let config = TcpConfig {
        ip: Some(flaky.address),
        ..config(&pool)
    };
    assert_eq!(
        Tcp::with_config(&config).unwrap_err().kind(),
        std::io::ErrorKind::NotConnected
    );
    assert_eq!(fast.connections.load(Ordering::SeqCst), 3);
    assert_eq!(slow.connections.load(Ordering::SeqCst), 3);
}

#[test]
fn bootstrap_is_shared_and_starts_only_one_background_measurement() {
    let server = Server::new(123, Duration::ZERO, false, Arc::new(Meter::default()));
    let pool = IpPool::new(vec![server.address]).unwrap();
    assert!(pool.snapshot().is_none());
    let mut tcp = Tcp::with_config(&config(&pool)).unwrap();
    assert_eq!(tcp.heartbeat_legacy().unwrap(), 123);
    for _ in 0..3 {
        pool.start_daily_refresh().unwrap();
    }
    let daily = pool.snapshot().unwrap();
    assert_eq!(daily.valid_nodes, [server.address]);
    assert_eq!(daily.results[0].rounds.len(), 3);
    assert_eq!(server.connections.load(Ordering::SeqCst), 4);
}

#[test]
fn removal_and_update_revoke_live_and_idle_connections_without_admitting_untested_nodes() {
    let meter = Arc::new(Meter::default());
    let a = Server::new(11, Duration::ZERO, false, meter.clone());
    let b = Server::new(22, Duration::ZERO, false, meter.clone());
    let c = Server::new(33, Duration::ZERO, false, meter);
    let pool = IpPool::new(vec![a.address, b.address]).unwrap();
    pool.refresh().unwrap();
    let cfg = config(&pool);
    let connections = ConnectionPool::with_tcp_config(
        PoolConfig {
            max_size: 2,
            health_check_timeout: 0,
            ..Default::default()
        },
        cfg.clone(),
    )
    .unwrap();
    let idle = connections.get_connection().unwrap();
    drop(idle);
    let mut live = Tcp::with_config(&cfg).unwrap();
    let old = live.get_ref().0.peer_addr().unwrap();
    let other = if old == a.address {
        b.address
    } else {
        a.address
    };
    assert!(pool.remove(old));
    assert!(!live.is_reusable());
    let expected = if other == a.address { 11 } else { 22 };
    assert_eq!(live.heartbeat_legacy().unwrap(), expected);
    assert_eq!(live.get_ref().0.peer_addr().unwrap(), other);
    pool.update(other, c.address).unwrap();
    assert!(pool.snapshot().unwrap().valid_nodes.is_empty());
    assert_eq!(
        live.heartbeat().unwrap_err().kind(),
        std::io::ErrorKind::NotConnected
    );
    assert_eq!(
        connections.get_connection().err().unwrap().kind(),
        std::io::ErrorKind::NotConnected
    );
    assert_eq!(c.connections.load(Ordering::SeqCst), 0);
    pool.refresh().unwrap();
    assert_eq!(live.heartbeat_legacy().unwrap(), 33);
    let mut borrowed = connections.get_connection().unwrap();
    assert_eq!(borrowed.get_mut().heartbeat_legacy().unwrap(), 33);
    assert_eq!(
        borrowed.get_mut().get_ref().0.peer_addr().unwrap(),
        c.address
    );
    drop(borrowed);
    connections.close();
}

#[test]
fn actual_probe_connections_are_bounded_to_eight_and_cover_every_node_three_times() {
    let meter = Arc::new(Meter::default());
    let servers: Vec<_> = (0..12)
        .map(|_| Server::new(123, Duration::from_millis(20), false, meter.clone()))
        .collect();
    let pool = IpPool::new(servers.iter().map(|server| server.address).collect()).unwrap();
    let daily = pool.refresh().unwrap();
    assert_eq!(daily.valid_nodes.len(), 12);
    assert!(
        servers
            .iter()
            .all(|server| server.connections.load(Ordering::SeqCst) == 3)
    );
    let peak = meter.peak.load(Ordering::SeqCst);
    assert!((2..=8).contains(&peak), "实际并发请求峰值: {peak}");
}
