use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// A failed readiness assertion must not leave a CLI process running.
struct ChildGuard(Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn full_command_deadline_and_cancellation_preserve_previous_output() {
    for cancel in [false, true] {
        let directory =
            std::env::temp_dir().join(format!("rustdx-cli-budget-{}-{cancel}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("bars.csv"), "previous success").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/day.zip", listener.local_addr().unwrap());
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            for request in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                if request == 0 {
                    assert!(header.starts_with(b"HEAD "));
                    started_tx.send(Instant::now()).unwrap();
                    socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                } else {
                    assert!(header.starts_with(b"GET "));
                    ready_tx.send(()).unwrap();
                    // Only the CLI deadline/cancellation may end the pending
                    // request. Do not manufacture an EOF after a fixed sleep.
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            }
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_rustdx"));
        command
            .arg("--no-tcp-startup")
            .current_dir(&directory)
            .args([
                "--timeout",
                if cancel { "10" } else { "1" },
                "--cancel-file",
                "cancel",
                "--network-dir",
                "permits",
                "day",
                &url,
                "-o",
                "bars.csv",
                "--report",
                "run.json",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let spawned_at = Instant::now();
        let mut child = ChildGuard(Some(command.spawn().unwrap()));
        let request_started = started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        // Cold process startup precedes the CLI's OperationContext. Measure the
        // running source operation from HEAD, or cancellation from its trigger.
        let measured_from = if cancel {
            Instant::now()
        } else {
            request_started
        };
        if cancel {
            fs::write(directory.join("cancel"), []).unwrap();
        }
        let limit = Duration::from_millis(if cancel { 800 } else { 1600 });
        // Bound failure too: a broken cancellation path must not hang the test.
        let exceeded = loop {
            let process = child.0.as_mut().unwrap();
            if process.try_wait().unwrap().is_some() {
                break false;
            }
            if measured_from.elapsed() >= limit {
                let _ = process.kill();
                break true;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let output = child.0.take().unwrap().wait_with_output().unwrap();
        let elapsed = measured_from.elapsed();
        release_tx.send(()).unwrap();
        server.join().unwrap();
        assert!(!output.status.success());
        assert!(
            !exceeded && elapsed < limit,
            "cancel={cancel}, startup={:?}, shutdown={elapsed:?}, limit={limit:?}, exceeded={exceeded}, stderr={}",
            request_started.duration_since(spawned_at),
            String::from_utf8_lossy(&output.stderr),
        );
        assert_eq!(
            fs::read_to_string(directory.join("bars.csv")).unwrap(),
            "previous success"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("run.json")).unwrap()).unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(report["stage"], "source");
        assert_eq!(report["resources"]["network_active"], 0);
        fs::remove_dir_all(directory).unwrap();
    }
}
