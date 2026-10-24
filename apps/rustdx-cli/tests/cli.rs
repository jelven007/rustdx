use std::{fs, process::Command};

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rustdx"))
        .arg("--no-tcp-startup")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_invalid_output_finish_without_network() {
    let help = cli(&["day", "-h", "output"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("csv_path.csv"));
    let invalid = cli(&["day", "-o", "unsupported"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("不支持的输出"));
    assert!(cli(&["show", "-v"]).status.success());
}

#[test]
fn invalid_readback_batch_preserves_output_before_database_access() {
    let dir = std::env::temp_dir().join(format!("rustdx-batch-config-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for backend in ["clickhouse", "mongodb"] {
        fs::write(dir.join(backend), "previous success").unwrap();
        for batch in ["0", "16385", "invalid"] {
            let result = Command::new(env!("CARGO_BIN_EXE_rustdx"))
                .arg("--no-tcp-startup")
                .current_dir(&dir)
                .env("RUSTDX_VERIFY_BATCH_ROWS", batch)
                .env("CLICKHOUSE_URL", "http://127.0.0.1:1")
                .env("MONGODB_URI", "mongodb://127.0.0.1:1")
                .args(["day", ".", "-o", backend, "--report", "run.json"])
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert_eq!(
                fs::read_to_string(dir.join(backend)).unwrap(),
                "previous success"
            );
            let report: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
            assert_eq!(report["database_status"], "not_started");
            assert!(report["artifact"].is_null());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn conflicting_outputs_fail_before_network_or_input_changes() {
    let dir = std::env::temp_dir().join(format!("rustdx-cli-paths-{}", std::process::id()));
    fs::create_dir_all(dir.join("sub")).unwrap();
    let input = dir.join("sh600000.day");
    fs::write(&input, b"original source").unwrap();
    fs::write(dir.join("gbbq"), b"original events").unwrap();
    fs::write(dir.join("previous.csv"), b"original factor").unwrap();
    let absolute = dir.join("bars.csv");
    let cases = [
        vec![
            "day",
            ".",
            "-o",
            "bars.csv",
            "--report",
            absolute.to_str().unwrap(),
        ],
        vec!["day", ".", "--report", "sub/../sh600000.day"],
        vec!["day", ".", "-g", "gbbq", "--report", "sub/../gbbq"],
        vec![
            "day",
            ".",
            "-g",
            "gbbq",
            "-p",
            "previous.csv",
            "-o",
            "previous.csv",
        ],
        vec![
            "day",
            ".",
            "-o",
            "clickhouse",
            "--keep-csv",
            "--report",
            "./clickhouse.csv",
        ],
        vec![
            "day",
            ".",
            "-g",
            "gbbq",
            "-p",
            "previous.csv",
            "--report",
            "./previous.csv",
        ],
        vec![
            "day",
            ".",
            "-g",
            "gbbq",
            "-p",
            "clickhouse",
            "--keep-factor",
            "-o",
            "factor.csv",
        ],
    ];
    for args in cases {
        let result = Command::new(env!("CARGO_BIN_EXE_rustdx"))
            .arg("--no-tcp-startup")
            .current_dir(&dir)
            .args(&args)
            .output()
            .unwrap();
        assert!(!result.status.success(), "{args:?}");
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains("输出路径冲突") || error.contains("输出不能覆盖输入"),
            "{args:?}: {error}"
        );
    }
    assert_eq!(fs::read(input).unwrap(), b"original source");
    assert_eq!(fs::read(dir.join("gbbq")).unwrap(), b"original events");
    assert_eq!(
        fs::read(dir.join("previous.csv")).unwrap(),
        b"original factor"
    );
    assert!(!absolute.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn csv_collection_and_strict_gap_protect_previous_output() {
    let dir = std::env::temp_dir().join(format!("rustdx-cli-smoke-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("sz000001.day");
    let output = dir.join("bars.csv");
    let report = dir.join("run.json");
    let record = |date: u32| {
        let mut bytes = Vec::new();
        for n in [date, 1000, 1100, 900, 1000] {
            bytes.extend(n.to_le_bytes());
        }
        bytes.extend(1000f32.to_le_bytes());
        bytes.extend(1000u32.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes
    };
    let mut bytes = record(20250106);
    bytes.extend(record(20250108)); // 1月7日缺口
    fs::write(source, bytes).unwrap();
    let args = [
        dir.to_str().unwrap(),
        "-o",
        output.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
        "--workers",
        "2",
    ];
    let mut command = vec!["day"];
    command.extend(args);
    assert!(cli(&command).status.success());
    let success = fs::read(&output).unwrap();
    assert!(String::from_utf8_lossy(&success).contains("sz000001"));
    command.push("--strict");
    assert!(!cli(&command).status.success());
    assert_eq!(fs::read(output).unwrap(), success);
    let status: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(status["status"], "failed");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_database_import_keeps_csv_and_marks_report_failed() {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("rustdx-db-report-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut bytes = Vec::new();
    for n in [20250106u32, 1000, 1100, 900, 1000] {
        bytes.extend(n.to_le_bytes());
    }
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend(1000u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    fs::write(dir.join("sz000001.day"), bytes).unwrap();
    let columns = [
        ("date", "Date"),
        ("code", "String"),
        ("open", "Float64"),
        ("high", "Float64"),
        ("low", "Float64"),
        ("close", "Float64"),
        ("amount", "Float64"),
        ("vol", "Float64"),
        ("preclose", "Float64"),
        ("factor", "Float64"),
        ("version", "UInt64"),
    ]
    .into_iter()
    .map(|(name, kind)| {
        let default = match name {
            "version" => "\tDEFAULT\ttoUnixTimestamp64Nano(now64(9))",
            "factor" | "preclose" => "\tDEFAULT\t0",
            _ => "\t\t",
        };
        format!("{name}\t{kind}{default}\n")
    })
    .collect::<String>();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for (status, body) in [
            ("200 OK", String::new()),
            ("200 OK", String::new()),
            (
                "200 OK",
                "ReplacingMergeTree(version) ORDER BY (date, code)\tdate, code\t\n".into(),
            ),
            ("200 OK", columns),
            (
                "500 Internal Server Error",
                "DB::Exception: injected insert failure".into(),
            ),
        ] {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "CLI没有发出预期的HTTP请求");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            socket.write_all(format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            ).as_bytes()).unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_rustdx"))
        .arg("--no-tcp-startup")
        .current_dir(&dir)
        .args(["day", ".", "-o", "clickhouse", "--report", "run.json"])
        .env("CLICKHOUSE_URL", url)
        .env("CLICKHOUSE_USER", "default")
        .env("CLICKHOUSE_PASSWORD", "")
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("injected insert failure"));
    assert!(
        fs::read_to_string(dir.join("clickhouse"))
            .unwrap()
            .contains("sz000001")
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "failed");
    assert!(!report["errors"].as_array().unwrap().is_empty());
    assert_eq!(report["files"][0]["rows"], 1);
    fs::remove_dir_all(dir).unwrap();
}
