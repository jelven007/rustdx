use std::{fs, process::Command};

#[test]
fn status_strict_failure_still_publishes_reviewable_report() {
    let dir = std::env::temp_dir().join(format!("rustdx-maintenance-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("status.json");
    let run = |strict: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rustdx"));
        command
            .arg("--no-tcp-startup")
            .args(["data-status", "--as-of", "2026-10-23", "--report"])
            .arg(&path);
        if strict {
            command.arg("--strict");
        }
        command.output().unwrap()
    };
    assert!(run(false).status.success());
    let result = run(true);
    assert!(!result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        report
    );
    assert_eq!(report["coverage"][0]["status"], "due");
    assert_eq!(report["coverage"][0]["remaining_days"], 69);
    assert_eq!(report["coverage"][1]["status"], "unverified");
    assert_eq!(report["coverage"][2]["status"], "missing");
    assert_eq!(report["attention_required"], true);
    fs::remove_dir_all(dir).unwrap();
}
