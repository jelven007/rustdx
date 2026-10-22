use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn full_command_deadline_and_cancellation_preserve_previous_output() {
    for cancel in [false, true] {
        let directory =
            std::env::temp_dir().join(format!("rustdx-cli-budget-{}-{cancel}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("bars.csv"), "previous success").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/day.zip", listener.local_addr().unwrap());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
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
                    socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                } else {
                    ready_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(1200));
                }
            }
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_rustdx"));
        command
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
        let start = Instant::now();
        let child = command.spawn().unwrap();
        ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        if cancel {
            fs::write(directory.join("cancel"), []).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(start.elapsed() < Duration::from_millis(if cancel { 800 } else { 1600 }));
        assert_eq!(
            fs::read_to_string(directory.join("bars.csv")).unwrap(),
            "previous success"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("run.json")).unwrap()).unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(report["stage"], "source");
        assert_eq!(report["resources"]["network_active"], 0);
        server.join().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
}
