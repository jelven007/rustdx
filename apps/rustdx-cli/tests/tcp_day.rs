use rustdx_cli::tcp_day::{Request, SOURCE_CONTRACT, Source, collect, replay_security};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::atomic::Ordering, time::Duration};
use tempfile::TempDir;

#[allow(dead_code)]
#[path = "common/tcp_day_peer.rs"]
mod peer;

fn setup(base: &Path, endpoint: std::net::SocketAddr) -> Request {
    fs::create_dir(base.join("facts")).unwrap();
    fs::create_dir(base.join("raw")).unwrap();
    let evidence = b"SYNTHETIC TEST UNIT BASIS; NOT VERIFIED MARKET EVIDENCE";
    let hash = format!("{:x}", Sha256::digest(evidence));
    fs::write(base.join("facts").join(&hash), evidence).unwrap();
    let request = Request {
        schema_version: rustdx_cli::tcp_day::REQUEST_SCHEMA_VERSION,
        source_contract: SOURCE_CONTRACT.into(),
        source: Source {
            endpoint,
            volume_evidence_sha256: hash,
            page_size: 3,
            max_pages: 10,
            max_raw_rows: 100,
            max_payload_bytes: 65536,
            max_wire_records: 32,
        },
        from: "2026-07-06".parse().unwrap(),
        through: "2026-07-07".parse().unwrap(),
        acquired_on: chrono::Utc::now()
            .with_timezone(&rustdx_complete::data::exchange_timezone())
            .date_naive(),
        codes: vec!["sh600000".into()],
        max_archive_bytes: 1024 * 1024,
        max_csv_bytes: 1024 * 1024,
        max_rows: 100,
        schedule: None,
    };
    fs::write(
        base.join("tcp-request.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    request
}

#[test]
fn compressed_short_pages_replay_exact_fields_and_duplicate_boundaries() {
    let temp = TempDir::new().unwrap();
    let server = peer::Peer::new();
    let request = setup(temp.path(), server.endpoint);
    let context =
        rustdx_complete::operation::OperationContext::new(Duration::from_secs(15), None).unwrap();
    let _scope = context.enter();
    collect(temp.path(), &request).unwrap();
    assert_eq!(*server.offsets.lock().unwrap(), [0, 1, 3, 3]);
    let rows = replay_security(&temp.path().join("raw/sh600000"), &request, "sh600000").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].vol, 137_983_701.0);
    assert_eq!(rows[0].amount, 12345.5);
    let csv: Vec<rustdx_cli::tcp_day::Row> =
        csv::Reader::from_path(temp.path().join("collected.csv"))
            .unwrap()
            .deserialize()
            .collect::<Result<_, _>>()
            .unwrap();
    assert_eq!(csv, rows);
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("raw/sh600000/receipt.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["pages"][1]["duplicate_rows"], 1);
    assert_eq!(receipt["schema_version"], 2);
    assert_eq!(receipt["source_contract"], SOURCE_CONTRACT);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("collection.json")).unwrap()).unwrap();
    assert_eq!(report["business_coverage"], "unverified");
    assert_eq!(report["schema_version"], 8);
    assert_eq!(report["volume_unit"], "shares");
    assert!(collect(temp.path(), &request).is_err());
}

#[test]
fn wire_receipt_and_orphan_file_changes_are_rejected_independently() {
    let temp = TempDir::new().unwrap();
    let server = peer::Peer::new();
    let request = setup(temp.path(), server.endpoint);
    let context =
        rustdx_complete::operation::OperationContext::new(Duration::from_secs(15), None).unwrap();
    let _scope = context.enter();
    collect(temp.path(), &request).unwrap();
    let raw = temp.path().join("raw/sh600000");
    let receipt_file = raw.join("receipt.json");
    let original = fs::read(&receipt_file).unwrap();
    for field in ["requested_count", "raw_count", "duplicate_rows"] {
        let mut receipt: serde_json::Value = serde_json::from_slice(&original).unwrap();
        receipt["pages"][0][field] = serde_json::json!(99);
        fs::write(&receipt_file, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(
            replay_security(&raw, &request, "sh600000").is_err(),
            "{field}"
        );
    }
    let mut receipt: serde_json::Value = serde_json::from_slice(&original).unwrap();
    receipt["rows"][0]["close"] = serde_json::json!(11.0);
    fs::write(&receipt_file, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(replay_security(&raw, &request, "sh600000").is_err());
    fs::write(&receipt_file, original).unwrap();
    let orphan = raw.join("wire/unlisted.json");
    fs::write(&orphan, b"unlisted").unwrap();
    assert!(replay_security(&raw, &request, "sh600000").is_err());
    fs::remove_file(orphan).unwrap();
    fs::write(raw.join("wire/000003.json"), b"tampered").unwrap();
    assert!(replay_security(&raw, &request, "sh600000").is_err());
}

#[test]
fn closed_date_unit_evidence_scope_and_archive_budget_refuse_collection() {
    for failure in [
        "today", "digest", "scope", "budget", "pages", "rows", "payload", "records",
    ] {
        let temp = TempDir::new().unwrap();
        let server = peer::Peer::new();
        let mut request = setup(temp.path(), server.endpoint);
        match failure {
            "today" => {
                request.through = chrono::Utc::now()
                    .with_timezone(&rustdx_complete::data::exchange_timezone())
                    .date_naive()
            }
            "digest" => fs::write(
                temp.path()
                    .join("facts")
                    .join(&request.source.volume_evidence_sha256),
                b"changed",
            )
            .unwrap(),
            "scope" => request.codes = vec!["sh000001".into()],
            "budget" => request.max_archive_bytes = 1,
            "pages" => request.source.max_pages = 1,
            "rows" => request.max_rows = 1,
            "payload" => request.source.max_payload_bytes = 1,
            "records" => request.source.max_wire_records = 4,
            _ => unreachable!(),
        }
        let context =
            rustdx_complete::operation::OperationContext::new(Duration::from_secs(15), None)
                .unwrap();
        let _scope = context.enter();
        assert!(collect(temp.path(), &request).is_err(), "{failure}");
        assert!(!temp.path().join("collection.json").exists(), "{failure}");
    }
}

#[test]
fn an_enumerated_empty_tail_keeps_missing_dates_visible_to_runner() {
    let temp = TempDir::new().unwrap();
    let server = peer::Peer::new();
    server.missing.store(true, Ordering::SeqCst);
    let request = setup(temp.path(), server.endpoint);
    let context =
        rustdx_complete::operation::OperationContext::new(Duration::from_secs(15), None).unwrap();
    let _scope = context.enter();
    collect(temp.path(), &request).unwrap();
    let rows = replay_security(&temp.path().join("raw/sh600000"), &request, "sh600000").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].date.to_string(), "2026-07-07");
}

#[test]
fn frozen_v1_archives_replay_with_their_original_lossy_volume() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tcp-day-legacy");
    for name in ["tcp-live-0", "tcp-live-1"] {
        let directory = base.join(name);
        let request = Request::load(&directory.join("tcp-request.json")).unwrap();
        assert_eq!(request.schema_version, 1);
        assert_eq!(
            request.source_contract,
            rustdx_cli::tcp_day::LEGACY_SOURCE_CONTRACT
        );
        let rows = replay_security(&directory.join("raw/sh600000"), &request, "sh600000").unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].vol, 54_074_700.0);
        assert!(
            collect(&directory, &request)
                .unwrap_err()
                .to_string()
                .contains("offline replay only")
        );
        let mut mismatched = request;
        mismatched.schema_version = 2;
        assert!(mismatched.validate().is_err());
    }
}

fn scheduled(request: &mut Request, securities: usize, parallel: usize) {
    request.codes = (0..securities)
        .map(|i| format!("sh{:06}", 600000 + i))
        .collect();
    request.max_archive_bytes = 16 * 1024 * 1024;
    request.source.max_wire_records = 1000;
    request.schedule = Some(rustdx_cli::tcp_day::Schedule {
        max_parallel: parallel,
        timeout_seconds: 15,
        max_raw_rows: request.source.max_raw_rows as u64 * securities as u64,
        max_payload_bytes: (request.source.max_payload_bytes as u64 + 2 * u16::MAX as u64)
            * securities as u64,
    });
}

#[test]
fn scheduled_scope_exceeds_32_with_bounded_parallelism_and_ordered_exact_replay() {
    let temp = TempDir::new().unwrap();
    let server = peer::Peer::new();
    server.delay_ms.store(5, Ordering::SeqCst);
    let mut request = setup(temp.path(), server.endpoint);
    scheduled(&mut request, 40, 3);
    fs::write(
        temp.path().join("tcp-request.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    collect(temp.path(), &request).unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("collection.json")).unwrap()).unwrap();
    assert_eq!(report["acquisition"]["securities"], 40);
    assert_eq!(report["acquisition"]["max_parallel"], 3);
    let peak = server.peak.load(Ordering::SeqCst);
    assert!((2..=3).contains(&peak), "{peak}");
    rustdx_cli::tcp_day::verify_archive_budget(&temp.path().join("raw"), &request).unwrap();
    let csv: Vec<rustdx_cli::tcp_day::Row> =
        csv::Reader::from_path(temp.path().join("collected.csv"))
            .unwrap()
            .deserialize()
            .collect::<Result<_, _>>()
            .unwrap();
    assert_eq!(csv.len(), 80);
    for (code, rows) in request.codes.iter().zip(csv.chunks_exact(2)) {
        assert_eq!(
            rows,
            replay_security(&temp.path().join("raw").join(code), &request, code).unwrap()
        );
    }
    let mut legacy = request.clone();
    legacy.schedule = None;
    assert!(legacy.validate().is_err());
    let mut insufficient = request;
    insufficient.schedule.as_mut().unwrap().max_raw_rows = 1;
    assert!(
        rustdx_cli::tcp_day::verify_archive_budget(&temp.path().join("raw"), &insufficient)
            .is_err()
    );
}

#[test]
fn total_reservations_csv_limit_failure_and_parent_deadline_never_complete() {
    for failure in [
        "raw", "payload", "archive", "wire", "csv", "worker", "deadline",
    ] {
        let temp = TempDir::new().unwrap();
        let server = peer::Peer::new();
        server.delay_ms.store(10, Ordering::SeqCst);
        let mut request = setup(temp.path(), server.endpoint);
        scheduled(&mut request, 5, 2);
        match failure {
            "raw" => request.schedule.as_mut().unwrap().max_raw_rows = 100,
            "payload" => {
                request.schedule.as_mut().unwrap().max_payload_bytes = 65536 + 2 * u16::MAX as u64
            }
            "archive" => request.max_archive_bytes = 100,
            "wire" => request.source.max_wire_records = 12,
            "csv" => request.max_csv_bytes = 40,
            "worker" => *server.failed_code.lock().unwrap() = Some("600001".into()),
            "deadline" => server.delay_ms.store(300, Ordering::SeqCst),
            _ => unreachable!(),
        }
        let parent = rustdx_complete::operation::OperationContext::new(
            if failure == "deadline" {
                Duration::from_millis(150)
            } else {
                Duration::from_secs(15)
            },
            None,
        )
        .unwrap();
        let _guard = parent.enter();
        assert!(collect(temp.path(), &request).is_err(), "{failure}");
        assert!(!temp.path().join("collection.json").exists(), "{failure}");
        if failure != "deadline" {
            parent.remaining().unwrap();
        }
        assert!(server.peak.load(Ordering::SeqCst) <= 2);
        if temp.path().join("collected.csv").exists() {
            assert!(
                fs::metadata(temp.path().join("collected.csv"))
                    .unwrap()
                    .len()
                    <= request.max_csv_bytes
            );
        }
    }
}
