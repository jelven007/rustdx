use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

#[path = "startup_server.rs"]
mod local;
use local::{Mode, Server};

struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn command(binary: &str, directory: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(directory)
        .env_remove("RUSTDX_TCP_INITIAL_IPS")
        .args(["--network-limit", "1", "--network-dir", "permits"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn finish(mut child: ChildGuard, timeout: Duration) -> Output {
    let started = Instant::now();
    while child.0.as_mut().unwrap().try_wait().unwrap().is_none() {
        assert!(
            started.elapsed() < timeout,
            "binary exceeded startup deadline"
        );
        thread::sleep(Duration::from_millis(5));
    }
    child.0.take().unwrap().wait_with_output().unwrap()
}

pub fn verify(binary: &str, action: &[&str], expected_stdout: &str) {
    let name = Path::new(binary).file_name().unwrap().to_str().unwrap();
    let directory =
        std::env::temp_dir().join(format!("rustdx-startup-{name}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let good = Server::new(Mode::Good);
    let flaky = Server::new(Mode::Flaky);
    let refused = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = refused.local_addr().unwrap();
    drop(refused);
    let output = command(binary, &directory)
        .env(
            "RUSTDX_TCP_INITIAL_IPS",
            format!("{},{},{address}", flaky.address, good.address),
        )
        .args(action)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(expected_stdout));
    assert!(String::from_utf8_lossy(&output.stderr).contains("有效节点1/3"));
    assert_eq!(good.counts.connections.load(Ordering::SeqCst), 4);
    assert_eq!(good.counts.legacy.load(Ordering::SeqCst), 3);
    assert_eq!(good.counts.heartbeat.load(Ordering::SeqCst), 1);
    assert_eq!(flaky.counts.connections.load(Ordering::SeqCst), 3);
    assert_eq!(flaky.counts.legacy.load(Ordering::SeqCst), 3);
    assert_eq!(flaky.counts.heartbeat.load(Ordering::SeqCst), 0);

    // Parsing help terminates before initialization, even with bad environment.
    let help = command(binary, &directory)
        .env("RUSTDX_TCP_INITIAL_IPS", "invalid")
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--no-tcp-startup"));
    let offline = command(binary, &directory)
        .env("RUSTDX_TCP_INITIAL_IPS", "invalid")
        .arg("--no-tcp-startup")
        .args(action)
        .output()
        .unwrap();
    assert!(
        offline.status.success(),
        "{}",
        String::from_utf8_lossy(&offline.stderr)
    );
    assert!(String::from_utf8_lossy(&offline.stdout).contains(expected_stdout));
    assert_eq!(good.counts.connections.load(Ordering::SeqCst), 4);

    let invalid = command(binary, &directory)
        .env("RUSTDX_TCP_INITIAL_IPS", "invalid")
        .args(action)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("RUSTDX_TCP_INITIAL_IPS"));
    assert!(invalid.stdout.is_empty());
    let unavailable = command(binary, &directory)
        .env("RUSTDX_TCP_INITIAL_IPS", address.to_string())
        .args(action)
        .output()
        .unwrap();
    assert!(!unavailable.status.success());
    assert!(String::from_utf8_lossy(&unavailable.stderr).contains("当日有效IP池为空"));
    assert!(
        unavailable.stdout.is_empty(),
        "failed startup must precede business work"
    );

    let bad_heartbeat = Server::new(Mode::BadHeartbeat);
    let invalid_client = command(binary, &directory)
        .env("RUSTDX_TCP_INITIAL_IPS", bad_heartbeat.address.to_string())
        .args(action)
        .output()
        .unwrap();
    assert!(!invalid_client.status.success());
    assert!(invalid_client.stdout.is_empty());
    assert_eq!(bad_heartbeat.counts.legacy.load(Ordering::SeqCst), 3);
    assert_eq!(bad_heartbeat.counts.heartbeat.load(Ordering::SeqCst), 1);

    for cancel in [false, true] {
        let silent = Server::new(Mode::Silent);
        let mut process = command(binary, &directory);
        process
            .env("RUSTDX_TCP_INITIAL_IPS", silent.address.to_string())
            .args([
                "--timeout",
                if cancel { "10" } else { "1" },
                "--cancel-file",
                "cancel",
            ])
            .args(action);
        let spawned = Instant::now();
        let child = ChildGuard(Some(process.spawn().unwrap()));
        silent.wait_for_handshake();
        let signalled = Instant::now();
        if cancel {
            fs::write(directory.join("cancel"), []).unwrap();
        }
        let output = finish(child, Duration::from_secs(3));
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        if cancel {
            assert!(signalled.elapsed() < Duration::from_millis(800));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("取消") || stderr.contains("\"error_category\":\"cancelled\""),
                "{stderr}"
            );
        } else {
            assert!(spawned.elapsed() < Duration::from_secs(3));
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("时限") || stderr.contains("\"error_category\":\"timeout\""),
                "{stderr}"
            );
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
