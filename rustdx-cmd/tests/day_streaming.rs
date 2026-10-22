use chrono::{Datelike, NaiveDate};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const CHUNK: usize = 4096;

fn raw(date: NaiveDate, close: u32) -> Vec<u8> {
    [
        date.year() as u32 * 10000 + date.month() * 100 + date.day(),
        close,
        close,
        close,
        close,
        1000.0f32.to_bits(),
        100,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}

fn next_weekday(mut date: NaiveDate) -> NaiveDate {
    loop {
        date = date.succ_opt().unwrap();
        if date.weekday().number_from_monday() <= 5 {
            return date;
        }
    }
}

fn command(directory: &Path, workers: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustdx"));
    command.current_dir(directory).args([
        "--timeout",
        "20",
        "day",
        "z-first",
        "a-second",
        "-o",
        "bars.csv",
        "--report",
        "run.json",
        "--calendar",
        "calendar.json",
        "--workers",
        workers,
    ]);
    command
}

#[test]
fn worker_counts_are_byte_identical_and_late_errors_preserve_output() {
    let dir = std::env::temp_dir().join(format!("rustdx-stream-order-{}", std::process::id()));
    fs::create_dir_all(dir.join("z-first")).unwrap();
    fs::create_dir_all(dir.join("a-second")).unwrap();
    fs::write(
        dir.join("calendar.json"),
        r#"{
        "version":"synthetic-weekdays","source":"test-only",
        "start":"2000-01-01","end":"2030-12-31","holidays":[]}"#,
    )
    .unwrap();
    let mut dates = Vec::new();
    let mut date: NaiveDate = "2000-01-03".parse().unwrap();
    for _ in 0..CHUNK + 7 {
        dates.push(date);
        date = next_weekday(date);
    }
    let bytes: Vec<_> = dates.iter().flat_map(|d| raw(*d, 1000)).collect();
    for directory in ["z-first", "a-second"] {
        // Create in reverse filename order to check directory listing normalization.
        for stock in (0..4).rev() {
            let code = if directory == "z-first" {
                format!("sz00000{}", stock + 1)
            } else {
                format!("sh60000{stock}")
            };
            fs::write(dir.join(directory).join(format!("{code}.day")), &bytes).unwrap();
        }
    }
    assert!(
        command(&dir, "1")
            .arg("--strict")
            .output()
            .unwrap()
            .status
            .success()
    );
    let baseline = fs::read(dir.join("bars.csv")).unwrap();
    for workers in ["4", "8"] {
        let result = command(&dir, workers).arg("--strict").output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(dir.join("bars.csv")).unwrap(), baseline);
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
        assert_eq!(report["schema_version"], 6);
        assert_eq!(report["observations"]["temporary_current_bytes"], 0);
        assert!(
            report["observations"]["temporary_peak_bytes"]
                .as_u64()
                .unwrap()
                >= baseline.len() as u64
        );
        assert_eq!(report["observations"]["stages"]["snapshot"]["calls"], 8);
        assert_eq!(report["observations"]["stages"]["parse"]["failures"], 0);
        assert!(
            report["observations"]["stages"]["readback"]["calls"]
                .as_u64()
                .unwrap()
                >= 2
        );
        assert_eq!(report["freshness"]["quality_errors"], 0);
    }
    let mut reader = csv::Reader::from_reader(baseline.as_slice());
    let codes: Vec<_> = reader
        .records()
        .step_by(dates.len())
        .map(|record| record.unwrap()[1].to_owned())
        .collect();
    assert_eq!(
        codes,
        [
            "sz000001", "sz000002", "sz000003", "sz000004", "sh600000", "sh600001", "sh600002",
            "sh600003"
        ]
    );

    let path = dir.join("z-first/sz000001.day");
    let mut gap = bytes.clone();
    // A gap exactly at the chunk boundary must not escape continuity checks.
    gap[CHUNK * 32..(CHUNK + 1) * 32].copy_from_slice(&raw(dates[CHUNK + 1], 1000));
    fs::write(&path, &gap).unwrap();
    assert!(
        !command(&dir, "4")
            .arg("--strict")
            .output()
            .unwrap()
            .status
            .success()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
    assert!(report["files"].as_array().unwrap().iter().any(|file| {
        file["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("缺少"))
    }));
    assert_eq!(report["observations"]["temporary_current_bytes"], 0);
    assert!(
        report["observations"]["stages"]["parse"]["failures"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(fs::read(dir.join("bars.csv")).unwrap(), baseline);

    let mut malformed = bytes;
    malformed[CHUNK * 32..CHUNK * 32 + 4].copy_from_slice(&20250230u32.to_le_bytes());
    fs::write(&path, malformed).unwrap();
    assert!(!command(&dir, "4").output().unwrap().status.success());
    assert_eq!(fs::read(dir.join("bars.csv")).unwrap(), baseline);
    assert!(!fs::read_dir(&dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")
    }));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancellation_unblocks_ordered_workers_and_cleans_spools() {
    let dir = std::env::temp_dir().join(format!("rustdx-stream-cancel-{}", std::process::id()));
    fs::create_dir_all(dir.join("source")).unwrap();
    fs::write(dir.join("bars.csv"), b"previous success").unwrap();
    let bytes = raw("2025-01-06".parse().unwrap(), 1000).repeat(500_000);
    for code in 1..=12 {
        fs::write(
            dir.join("source").join(format!("sh000{code:03}.day")),
            &bytes,
        )
        .unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustdx"))
        .current_dir(&dir)
        .args([
            "--timeout",
            "20",
            "--cancel-file",
            "cancel",
            "day",
            "source",
            "-o",
            "bars.csv",
            "--report",
            "run.json",
            "--workers",
            "4",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read_dir(&dir).unwrap().any(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            name.starts_with("bars.csv.") && name.ends_with(".tmp")
        }) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(dir.join("cancel"), []).unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        if start.elapsed() > Duration::from_secs(3) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("取消后解析线程未退出");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read(dir.join("bars.csv")).unwrap(), b"previous success");
    assert!(!fs::read_dir(&dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")
    }));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["observations"]["temporary_current_bytes"], 0);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_zip_source_observations_survive_report_creation_after_extraction() {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("rustdx-observe-zip-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("input.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    zip.start_file(
        "sh/lday/sh600000.day",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(&raw("2026-10-22".parse().unwrap(), 1000))
        .unwrap();
    zip.finish().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rustdx"))
        .current_dir(&dir)
        .args(["day", "input.zip", "-o", "bars.csv", "--report", "run.json"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
    assert_eq!(report["observations"]["stages"]["source"]["calls"], 1);
    assert_eq!(report["observations"]["stages"]["extract"]["calls"], 1);
    assert_eq!(report["observations"]["stages"]["download"]["calls"], 0);
    assert_eq!(report["observations"]["temporary_current_bytes"], 0);
    assert_eq!(report["freshness"]["newest_source_last_date"], "2026-10-22");
    fs::remove_dir_all(dir).unwrap();
}
