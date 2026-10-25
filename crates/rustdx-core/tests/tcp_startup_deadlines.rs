//! Keep probe deadline tests in a separate process from startup policy configuration.
//! The process-wide network governor must be configured before its first use.
use rustdx_complete::{operation::OperationContext, tcp::ip::IpPool};
use std::{
    io,
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

#[allow(dead_code)]
#[path = "common/startup_server.rs"]
mod local;
use local::{Mode, Server};

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
