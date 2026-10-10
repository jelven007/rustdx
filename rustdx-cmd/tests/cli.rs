use std::{fs, process::Command};

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rustdx"))
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
