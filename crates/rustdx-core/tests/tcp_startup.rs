use rustdx_complete::{
    operation::OperationContext,
    pool::{ConnectionPool, PoolConfig},
    tcp::{
        FrameEvent, FrameObserver, NetworkPolicy, Tcp, TcpConfig, acquire_network,
        configure_network, ip::IpPool, network_stats, startup, stock::Client,
    },
};
use std::{
    io,
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[path = "common/startup_server.rs"]
mod local;
use local::{Mode, Server};

#[derive(Debug)]
struct Observer(AtomicUsize);
impl FrameObserver for Observer {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()> {
        if matches!(event, FrameEvent::Request { .. }) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[test]
fn process_startup_is_idempotent_hands_off_the_client_and_yields_a_single_permit() {
    configure_network(NetworkPolicy {
        max_connections: 1,
        acquire_timeout: Duration::from_millis(100),
        ..Default::default()
    })
    .unwrap();
    let good = Server::new(Mode::Good);
    let flaky = Server::new(Mode::Flaky);
    let refused = TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_address = refused.local_addr().unwrap();
    drop(refused);
    let pool = IpPool::new(vec![good.address, flaky.address, refused_address]).unwrap();
    let config = TcpConfig {
        timeout: Duration::from_millis(500),
        ip_pool: Some(pool.clone()),
        ..Default::default()
    };
    let report = startup::initialize_with_config(&config).unwrap();
    assert_eq!(report.daily.valid_nodes, [good.address]);
    assert_eq!(report.heartbeat.server_date_raw, 20261024);
    assert_eq!(good.counts.connections.load(Ordering::SeqCst), 4);
    assert_eq!(flaky.counts.connections.load(Ordering::SeqCst), 3);
    assert_eq!(network_stats().active, 1);
    let again = startup::initialize_with_config(&config).unwrap();
    assert_eq!(report.connection_id, again.connection_id);
    assert_eq!(good.counts.connections.load(Ordering::SeqCst), 4);
    let client = Client::with_config(&config).unwrap();
    assert_eq!(client.tcp.connection_id(), report.connection_id);
    assert_eq!(client.tcp.config(), &config);
    assert_eq!(good.counts.connections.load(Ordering::SeqCst), 4);
    drop(client);
    assert_eq!(network_stats().active, 0);

    let second_config = TcpConfig {
        auto_reconnect: 1,
        ..config.clone()
    };
    startup::initialize_with_config(&second_config).unwrap();
    let permit = acquire_network(Duration::from_secs(1)).unwrap();
    assert_eq!(network_stats().active, 1, "parked TCP yields to HTTP");
    drop(permit);
    assert_eq!(network_stats().active, 0);

    let report = startup::initialize_with_config(&config).unwrap();
    let observer = Arc::new(Observer(AtomicUsize::new(0)));
    let observed = Tcp::with_config_and_observer(&config, observer.clone()).unwrap();
    assert_ne!(observed.connection_id(), report.connection_id);
    assert_eq!(
        observer.0.load(Ordering::SeqCst),
        1,
        "observer receives its own handshake"
    );
    drop(observed);

    let report = startup::initialize_with_config(&second_config).unwrap();
    let connections = ConnectionPool::with_tcp_config(
        PoolConfig {
            health_check_timeout: 0,
            ..Default::default()
        },
        second_config.clone(),
    )
    .unwrap();
    let mut connection = connections.get_connection().unwrap();
    assert_eq!(connection.get_mut().connection_id(), report.connection_id);
    drop(connection);
    connections.close();

    // A changed seed list revokes the old report and must be fully remeasured.
    pool.replace(vec![refused_address]).unwrap();
    assert!(startup::snapshot().is_none());
    assert_eq!(
        startup::initialize_with_config(&config).unwrap_err().kind(),
        io::ErrorKind::NotConnected
    );
    assert!(pool.snapshot().unwrap().valid_nodes.is_empty());
    assert_eq!(network_stats().active, 0);
    pool.replace(vec![good.address]).unwrap();
    assert_eq!(
        startup::initialize_with_config(&config)
            .unwrap()
            .daily
            .valid_nodes,
        [good.address]
    );
    startup::release_client();
    assert_eq!((network_stats().active, network_stats().peak), (0, 1));

    let silent = Server::new(Mode::Silent);
    let silent_config = TcpConfig {
        ip_pool: Some(IpPool::new(vec![silent.address]).unwrap()),
        ..config.clone()
    };
    let operation = OperationContext::new(Duration::from_secs(10), None).unwrap();
    let worker_operation = operation.clone();
    let worker = thread::spawn(move || {
        let _scope = worker_operation.enter();
        startup::initialize_with_config(&silent_config)
            .unwrap_err()
            .kind()
    });
    silent.wait_for_handshake();
    let now = Instant::now();
    assert_eq!(
        startup::initialize_until(&config, now + Duration::from_millis(30))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    operation.cancel();
    assert_eq!(worker.join().unwrap(), io::ErrorKind::Interrupted);
    assert!(now.elapsed() < Duration::from_millis(500));
    assert_eq!(network_stats().active, 0);

    let bad_heartbeat = Server::new(Mode::BadHeartbeat);
    let bad_pool = IpPool::new(vec![bad_heartbeat.address]).unwrap();
    let bad_config = TcpConfig {
        ip_pool: Some(bad_pool.clone()),
        ..config
    };
    assert!(startup::initialize_with_config(&bad_config).is_err());
    assert_eq!(
        bad_pool.snapshot().unwrap().valid_nodes,
        [bad_heartbeat.address]
    );
    assert!(
        startup::snapshot().is_none(),
        "failed warmup cannot publish startup success"
    );
    assert_eq!(
        network_stats().active,
        0,
        "failed warm client releases its permit"
    );
}

#[test]
fn full_measurement_and_refresh_lock_obey_deadline_and_worker_cancellation() {
    let silent = Server::new(Mode::Silent);
    let pool = IpPool::new(vec![silent.address]).unwrap();
    let started = Instant::now();
    assert_eq!(
        pool.refresh_until(started + Duration::from_millis(80))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(
        pool.snapshot().is_none(),
        "partial timeout is never published"
    );

    let operation = OperationContext::new(Duration::from_secs(10), None).unwrap();
    let worker_operation = operation.clone();
    let worker_pool = pool.clone();
    let worker = thread::spawn(move || {
        let _scope = worker_operation.enter();
        worker_pool
            .refresh_until(Instant::now() + Duration::from_secs(10))
            .unwrap_err()
            .kind()
    });
    silent.wait_for_handshake();
    // The first timed-out attempt already sent a handshake; wait for the new batch.
    let until = Instant::now() + Duration::from_secs(2);
    while silent.counts.connections.load(Ordering::SeqCst) < 2 {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(2));
    }
    let start = Instant::now();
    assert_eq!(
        pool.refresh_until(start + Duration::from_millis(30))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut,
        "refresh lock also has a deadline"
    );
    operation.cancel();
    assert_eq!(worker.join().unwrap(), io::ErrorKind::Interrupted);
    assert!(start.elapsed() < Duration::from_millis(500));
    assert!(pool.snapshot().is_none());
}
