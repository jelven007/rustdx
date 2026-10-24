use rustdx_complete::tcp::{
    NetworkPolicy, SharedNetworkPolicy, acquire_network, configure_network,
};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn shared_child() {
    let Some(directory) = std::env::var_os("RUSTDX_TEST_PERMITS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    configure_network(NetworkPolicy {
        shared: Some(SharedNetworkPolicy {
            directory: directory.clone(),
            max_connections: 1,
        }),
        ..Default::default()
    })
    .unwrap();
    let _permit = acquire_network(Duration::from_secs(1)).unwrap();
    fs::write(directory.join("ready"), []).unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn independent_processes_share_a_slot_and_crash_releases_it() {
    let directory = std::env::temp_dir().join(format!("rustdx-shared-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "shared_child", "--nocapture"])
        .current_dir(std::env::temp_dir())
        .env("RUSTDX_TEST_PERMITS", &directory)
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !directory.join("ready").exists() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    configure_network(NetworkPolicy {
        shared: Some(SharedNetworkPolicy {
            directory: directory.clone(),
            max_connections: 1,
        }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        acquire_network(Duration::from_millis(25))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    child.kill().unwrap();
    child.wait().unwrap();
    drop(acquire_network(Duration::from_secs(1)).unwrap());
    assert!(directory.join("permit-0.lock").exists());
    assert_eq!(rustdx_complete::tcp::network_stats().active, 0);
    fs::remove_dir_all(directory).unwrap();
}
