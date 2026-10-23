use std::{fs, path::Path};

use rustdx_runner::{
    Error,
    config::{CONTRACT, Config},
    ledger::Ledger,
    model::{Evidence, Pointer},
    ops, pipeline, storage,
};
use serde_json::json;
use tempfile::TempDir;

fn evidence() -> Evidence {
    Evidence {
        path: "test.json".into(),
        sha256: "a".repeat(64),
        bytes: 1,
    }
}

fn queued(ledger: &mut Ledger, job: &str, attempt: &str) {
    ledger.register(job, "fixture", "{}").unwrap();
    ledger
        .begin_attempt(job, attempt, Path::new(attempt))
        .unwrap();
    ledger
        .seal(attempt, attempt, "fixture", &evidence(), "fixture-cli")
        .unwrap();
}

#[test]
fn deduplication_and_one_active_attempt_survive_reopen() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("ledger.sqlite");
    let mut ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.register("job", "fixture", "{}").unwrap(), "queued");
    ledger
        .begin_attempt("job", "first", Path::new("first"))
        .unwrap();
    drop(ledger);
    let mut ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.register("job", "fixture", "{}").unwrap(), "running");
    assert!(
        ledger
            .register("job", "fixture", "{\"different\":true}")
            .is_err()
    );
    assert!(
        ledger
            .begin_attempt("job", "second", Path::new("second"))
            .is_err()
    );
    assert_eq!(ledger.unfinished().unwrap().len(), 1);
}

#[test]
fn catalog_cas_preserves_previous_pointer_and_append_only_events() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("ledger.sqlite");
    let mut ledger = Ledger::open(&path).unwrap();
    queued(&mut ledger, "job1", "release1");
    queued(&mut ledger, "job2", "release2");
    let pointer = Pointer {
        version: 1,
        manifest: evidence(),
    };
    ledger.publish("fixture", 0, "release1", &pointer).unwrap();
    assert!(matches!(
        ledger.publish("fixture", 0, "release2", &pointer),
        Err(Error::Conflict(1))
    ));
    assert_eq!(ledger.pointer("fixture", None).unwrap().unwrap().version, 1);
    assert_eq!(
        ledger
            .statuses()
            .unwrap()
            .iter()
            .filter(|j| j.state == "released")
            .count(),
        1
    );
    assert!(
        ledger
            .finish("release1", "quarantined", "invalid late failure")
            .is_err()
    );
    ledger
        .finish("release2", "quarantined", "rejected candidate")
        .unwrap();
    assert!(
        ledger
            .publish(
                "fixture",
                1,
                "release2",
                &Pointer {
                    version: 2,
                    manifest: evidence()
                }
            )
            .is_err()
    );
    assert_eq!(ledger.pointer("fixture", None).unwrap().unwrap().version, 1);
    let connection = rusqlite::Connection::open(path).unwrap();
    assert!(connection.execute("DELETE FROM events", []).is_err());
    assert!(
        connection
            .execute("UPDATE events SET detail='changed'", [])
            .is_err()
    );
}

#[test]
fn failed_publication_event_rolls_back_pointer_and_job_state() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("ledger.sqlite");
    let mut ledger = Ledger::open(&path).unwrap();
    queued(&mut ledger, "job", "release");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_release BEFORE INSERT ON events WHEN NEW.stage='released'
         BEGIN SELECT RAISE(ABORT,'injected durability failure'); END;",
        )
        .unwrap();
    assert!(
        ledger
            .publish(
                "fixture",
                0,
                "release",
                &Pointer {
                    version: 1,
                    manifest: evidence()
                }
            )
            .is_err()
    );
    assert!(ledger.pointer("fixture", None).unwrap().is_none());
    assert_eq!(ledger.statuses().unwrap()[0].state, "evaluated");
    connection
        .execute_batch("DROP TRIGGER fail_release")
        .unwrap();
    ledger
        .publish(
            "fixture",
            0,
            "release",
            &Pointer {
                version: 1,
                manifest: evidence(),
            },
        )
        .unwrap();
    drop(ledger);
    assert_eq!(
        Ledger::open(&path).unwrap().statuses().unwrap()[0].state,
        "released"
    );
}

#[test]
fn wal_backup_restore_and_corruption_detection() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("live");
    storage::private_directory(&root).unwrap();
    let mut ledger = Ledger::open(&root.join("ledger.sqlite")).unwrap();
    ledger.register("job", "fixture", "{}").unwrap();
    let backup = temp.path().join("backup");
    ops::backup(&root, &backup, 1024 * 1024).unwrap();
    let restored = temp.path().join("restored");
    ops::restore(&backup, &restored, 1024 * 1024).unwrap();
    assert_eq!(
        Ledger::open(&restored.join("ledger.sqlite"))
            .unwrap()
            .statuses()
            .unwrap()
            .len(),
        1
    );
    fs::write(backup.join("ledger.sqlite"), b"corrupted").unwrap();
    let bad = temp.path().join("bad");
    assert!(ops::restore(&backup, &bad, 1024 * 1024).is_err());
    assert!(!bad.exists());
}

#[test]
fn evidence_limits_path_traversal_and_controller_exclusion() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("data"), b"12345").unwrap();
    assert!(storage::read_limited(&temp.path().join("data"), 4).is_err());
    assert!(storage::safe_join(temp.path(), "../data").is_err());
    let lock = storage::RootLock::acquire(temp.path()).unwrap();
    assert!(storage::RootLock::acquire(temp.path()).is_err());
    drop(lock);
    storage::RootLock::acquire(temp.path()).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temp.path().join("data"), temp.path().join("link")).unwrap();
        assert!(storage::safe_join(temp.path(), "link").is_err());
    }
}

#[test]
#[cfg(unix)]
fn managed_deadline_kills_worker_and_recovery_does_not_rerun() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let temp = TempDir::new().unwrap();
    let mut config = fixture(temp.path(), Path::new("/not-used"));
    config.timeout_seconds = 2;
    config.publish_reserve_seconds = 1;
    config.cancel_grace_seconds = 0;
    storage::private_directory(&config.root).unwrap();
    let directory = config.root.join("worker");
    storage::private_directory(&directory).unwrap();
    let binary = directory.join("collector.bin");
    fs::write(&binary, b"#!/bin/sh\nexec /bin/sleep 10\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let mut ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    ledger.register("job", "synthetic", "{}").unwrap();
    ledger.begin_attempt("job", "attempt", &directory).unwrap();
    let operation = pipeline::operation(&config, None).unwrap();
    let _scope = operation.enter();
    let start = Instant::now();
    assert!(
        rustdx_runner::supervisor::collect(&config, &directory, "attempt", &ledger, &operation)
            .unwrap_err()
            .is_cancelled()
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(ledger.unfinished().unwrap()[0].pid.is_none());
    drop(ledger);
    let actions = pipeline::recover(&config.root).unwrap();
    assert!(actions[0].contains("without replaying collection"));
    assert!(
        Ledger::open(&config.root.join("ledger.sqlite"))
            .unwrap()
            .unfinished()
            .unwrap()
            .is_empty()
    );
}

fn fixture(base: &Path, cli: &Path) -> Config {
    let source = base.join("source");
    let facts = base.join("facts");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&facts).unwrap();
    // Explicitly synthetic facts, used only to exercise control-plane invariants.
    let digest = storage::sha256(b"SYNTHETIC TEST ANNOUNCEMENT; NOT MARKET FACT");
    fs::write(
        facts.join(&digest),
        b"SYNTHETIC TEST ANNOUNCEMENT; NOT MARKET FACT",
    )
    .unwrap();
    fs::write(
        base.join("calendar.json"),
        serde_json::to_vec(&json!({
            "version":"synthetic-calendar","source":"unit-test",
            "start":"2026-07-06","end":"2026-07-07","holidays":[]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        base.join("history.json"),
        serde_json::to_vec(&json!({
            "version":"synthetic-history","checked_through":"2026-07-07",
            "intervals":[{"code":"sh600000","start":"2026-07-06","end":"2026-07-07",
                "board":"Main","st":false,"stage":"Seasoned","state":"Trading",
                "announcement":{"url":"https://example.org/synthetic-test","sha256":digest,
                    "published":"2026-07-06","checked":"2026-07-07"}}]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        base.join("universe.json"),
        serde_json::to_vec(&json!({
            "version":"synthetic-universe","checked_through":"2026-07-07","source_sha256":digest,
            "members":[{"code":"sh600000","from":"2026-07-06","through":"2026-07-07"}]
        }))
        .unwrap(),
    )
    .unwrap();
    write_day(&source, &[20260706, 20260707], 1000);
    Config {
        schema_version: 1,
        root: base.join("live"),
        cli: cli.into(),
        dataset_id: "synthetic".into(),
        contract_version: CONTRACT.into(),
        policy_revision: 1,
        source_revision: 1,
        source_as_of: "2026-07-07".parse().unwrap(),
        planned_date: "2026-07-07".parse().unwrap(),
        from: "2026-07-06".parse().unwrap(),
        through: "2026-07-07".parse().unwrap(),
        day_directories: vec![source],
        calendar: base.join("calendar.json"),
        calendar_source_sha256: digest,
        security_history: base.join("history.json"),
        universe: base.join("universe.json"),
        facts_directory: facts,
        timeout_seconds: 60,
        publish_reserve_seconds: 10,
        cancel_grace_seconds: 1,
        workers: 1,
        max_source_bytes: 1024 * 1024,
        max_workspace_bytes: 256 * 1024 * 1024,
        max_rows: 1000,
        min_free_bytes: 0,
        database_targets: Vec::new(),
    }
}

fn write_day(source: &Path, dates: &[u32], close: u32) {
    let mut bytes = Vec::new();
    for date in dates {
        for value in [*date, 1000, 1200, 900, close] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(12345.5_f32.to_le_bytes());
        bytes.extend(101_u32.to_le_bytes());
        bytes.extend(0_u32.to_le_bytes());
    }
    fs::write(source.join("sh600000.day"), bytes).unwrap();
}

#[test]
fn strict_config_and_unknown_history_refuse_before_spawn() {
    let temp = TempDir::new().unwrap();
    let mut config = fixture(temp.path(), Path::new("/not-needed"));
    assert!(config.validate().is_ok());
    config.workers = 0;
    assert!(config.validate().is_err());
    config.workers = 1;
    let mut json = serde_json::to_value(&config).unwrap();
    json["unknown_option"] = json!(true);
    assert!(serde_json::from_value::<Config>(json).is_err());
    let mut history: serde_json::Value =
        serde_json::from_slice(&fs::read(&config.security_history).unwrap()).unwrap();
    history["intervals"][0]["end"] = json!("2026-07-06");
    fs::write(
        &config.security_history,
        serde_json::to_vec(&history).unwrap(),
    )
    .unwrap();
    let operation = pipeline::operation(&config, None).unwrap();
    let _scope = operation.enter();
    assert!(
        pipeline::execute(&config, &operation)
            .unwrap_err()
            .to_string()
            .contains("Unknown security state")
    );
    assert!(
        Ledger::open(&config.root.join("ledger.sqlite"))
            .unwrap()
            .unfinished()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[cfg(unix)]
fn all_explicitly_exempt_keys_publish_empty_partitions_without_a_collector() {
    let temp = TempDir::new().unwrap();
    // false would fail immediately if a worker were accidentally started.
    let config = fixture(temp.path(), Path::new("/usr/bin/false"));
    fs::remove_file(config.day_directories[0].join("sh600000.day")).unwrap();
    let mut history: serde_json::Value =
        serde_json::from_slice(&fs::read(&config.security_history).unwrap()).unwrap();
    history["intervals"][0]["state"] = json!("Suspended");
    fs::write(
        &config.security_history,
        serde_json::to_vec(&history).unwrap(),
    )
    .unwrap();
    let operation = pipeline::operation(&config, None).unwrap();
    let _scope = operation.enter();
    assert_eq!(
        pipeline::execute(&config, &operation)
            .unwrap()
            .unwrap()
            .version,
        1
    );
    let ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    let catalog = pipeline::fixed_catalog(&config.root, &ledger, "synthetic", None)
        .unwrap()
        .unwrap();
    for partition in catalog.partitions.values() {
        assert_eq!(partition.rows, 0);
        assert_eq!(partition.exemptions, 1);
    }
    let manifest = pipeline::load_release(
        &config.root,
        &ledger
            .release_evidence(&catalog.partitions.values().next().unwrap().release_id)
            .unwrap(),
    )
    .unwrap();
    assert!(!manifest.collector_executed);
}

#[test]
#[ignore = "requires RUSTDX_TEST_CLI pointing at the built rustdx executable"]
fn real_cli_release_revision_missing_coverage_and_isolated_restore() {
    let cli = std::env::var_os("RUSTDX_TEST_CLI").expect("RUSTDX_TEST_CLI required");
    let temp = TempDir::new().unwrap();
    let mut config = fixture(temp.path(), Path::new(&cli));
    let operation = pipeline::operation(&config, None).unwrap();
    let scope = operation.enter();
    assert_eq!(
        pipeline::execute(&config, &operation)
            .unwrap()
            .unwrap()
            .version,
        1
    );
    assert!(pipeline::execute(&config, &operation).unwrap().is_none());
    let ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    let pinned = pipeline::fixed_catalog(&config.root, &ledger, "synthetic", Some(1))
        .unwrap()
        .unwrap();
    assert_eq!(pinned.partitions.len(), 2);
    let read_session =
        rustdx_runner::consumer::PinnedDataset::open(&config.root, "synthetic", Some(1)).unwrap();
    let release = &pinned.partitions.values().next().unwrap().release_id;
    let manifest =
        pipeline::load_release(&config.root, &ledger.release_evidence(release).unwrap()).unwrap();
    assert_eq!(manifest.partitions.values().map(|p| p.rows).sum::<u64>(), 2);
    drop(ledger);
    config.source_revision = 2;
    write_day(&config.day_directories[0], &[20260706, 20260707], 1100);
    assert_eq!(
        pipeline::execute(&config, &operation)
            .unwrap()
            .unwrap()
            .version,
        2
    );
    let ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    assert_eq!(
        pipeline::fixed_catalog(&config.root, &ledger, "synthetic", Some(1))
            .unwrap()
            .unwrap()
            .partitions
            .values()
            .next()
            .unwrap()
            .release_id,
        *release
    );
    let export = temp.path().join("pinned.csv");
    assert_eq!(
        read_session
            .export(config.from, config.through, &export, 10, 1024)
            .unwrap(),
        2
    );
    let rows: Vec<rustdx_runner::consumer::CanonicalRow> = csv::Reader::from_path(&export)
        .unwrap()
        .deserialize()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows[0].close, 10.0);
    assert_eq!(rows[0].volume_shares, 101.0);
    let original = fs::read(&export).unwrap();
    assert!(
        read_session
            .export(config.from, config.through, &export, 1, 1024)
            .is_err()
    );
    assert_eq!(fs::read(&export).unwrap(), original);
    drop(ledger);
    config.source_revision = 3;
    write_day(&config.day_directories[0], &[20260706], 1100);
    assert!(
        pipeline::execute(&config, &operation)
            .unwrap_err()
            .to_string()
            .contains("missing=1")
    );
    let ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    assert_eq!(
        ledger.pointer("synthetic", None).unwrap().unwrap().version,
        2
    );
    let backup = temp.path().join("backup");
    ops::backup(&config.root, &backup, 1024 * 1024 * 1024).unwrap();
    let restored = temp.path().join("restored");
    ops::restore(&backup, &restored, 1024 * 1024 * 1024).unwrap();
    assert_eq!(
        Ledger::open(&restored.join("ledger.sqlite"))
            .unwrap()
            .pointer("synthetic", None)
            .unwrap()
            .unwrap()
            .version,
        2
    );
    drop(scope);
    #[cfg(unix)]
    thaw_test_directory(temp.path());
}

#[test]
#[ignore = "requires RUSTDX_TEST_CLI and RUSTDX_TEST_CH_USER/PASSWORD"]
fn ambiguous_database_write_preserves_evidence_and_recovery_never_reinserts() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for response in [
            "{\"engine\":\"MergeTree\",\"sorting_key\":\"dataset_id, release_id, contract_version, date, market, code, instrument_class\",\"create_table_query\":\"CREATE TABLE\"}\n",
            "0\n",
            "DB::Exception: injected failure after partial write",
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("query_id="));
            let mut length = 0;
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).unwrap();
        }
    });
    let cli = std::env::var_os("RUSTDX_TEST_CLI").unwrap();
    let temp = TempDir::new().unwrap();
    let mut config = fixture(temp.path(), Path::new(&cli));
    config
        .database_targets
        .push(rustdx_runner::config::DatabaseTarget::Clickhouse {
            id: "fault".into(),
            url,
            table: "rustdx_versioned.bars".into(),
            user_env: "RUSTDX_TEST_CH_USER".into(),
            password_env: "RUSTDX_TEST_CH_PASSWORD".into(),
        });
    let operation = pipeline::operation(&config, None).unwrap();
    let _scope = operation.enter();
    assert!(
        pipeline::execute(&config, &operation)
            .unwrap_err()
            .to_string()
            .contains("rejected")
    );
    server.join().unwrap();
    let ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    assert!(ledger.pointer("synthetic", None).unwrap().is_none());
    let attempt = ledger.unfinished().unwrap()[0].attempt_id.clone();
    assert!(
        pipeline::recover(&config.root)
            .unwrap_err()
            .to_string()
            .contains("unknown database write")
    );
    let connection = rusqlite::Connection::open(config.root.join("ledger.sqlite")).unwrap();
    let writes: i64 = connection
        .query_row("SELECT count(*) FROM target_io WHERE is_write=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(writes, 1);
    pipeline::quarantine(
        &config.root,
        &attempt,
        "injected unknown write; abandoned unique release",
    )
    .unwrap();
    assert!(
        pipeline::publish(
            &config.root,
            &mut Ledger::open(&config.root.join("ledger.sqlite")).unwrap(),
            &attempt
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires RUSTDX_TEST_CLI"]
fn durable_seal_recovers_without_recollection_and_older_revision_can_be_quarantined() {
    let cli = std::env::var_os("RUSTDX_TEST_CLI").unwrap();
    let temp = TempDir::new().unwrap();
    let mut config = fixture(temp.path(), Path::new(&cli));
    storage::private_directory(&config.root).unwrap();
    Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    let db = rusqlite::Connection::open(config.root.join("ledger.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER inject_before_commit BEFORE INSERT ON events WHEN NEW.stage='released'
        BEGIN SELECT RAISE(ABORT,'injected crash boundary'); END;",
    )
    .unwrap();
    let operation = pipeline::operation(&config, None).unwrap();
    let _scope = operation.enter();
    assert!(pipeline::execute(&config, &operation).is_err());
    assert_eq!(
        Ledger::open(&config.root.join("ledger.sqlite"))
            .unwrap()
            .unfinished()
            .unwrap()[0]
            .state,
        "reconciliation_required"
    );
    // A changed input must not affect recovery of the previously sealed release.
    write_day(&config.day_directories[0], &[20260706, 20260707], 1100);
    db.execute_batch("DROP TRIGGER inject_before_commit")
        .unwrap();
    assert!(pipeline::recover(&config.root).unwrap()[0].contains("catalog 1"));
    let pinned =
        rustdx_runner::consumer::PinnedDataset::open(&config.root, "synthetic", Some(1)).unwrap();
    let output = temp.path().join("recovered.csv");
    pinned
        .export(config.from, config.through, &output, 100, 4096)
        .unwrap();
    let rows: Vec<rustdx_runner::consumer::CanonicalRow> = csv::Reader::from_path(output)
        .unwrap()
        .deserialize()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows[0].close, 10.0);
    // Same source revision but a different policy is a new Job, not an allowed replacement.
    config.policy_revision = 2;
    assert!(
        pipeline::execute(&config, &operation)
            .unwrap_err()
            .to_string()
            .contains("advance explicit")
    );
    let mut ledger = Ledger::open(&config.root.join("ledger.sqlite")).unwrap();
    let attempt = ledger.unfinished().unwrap()[0].attempt_id.clone();
    pipeline::quarantine(&config.root, &attempt, "explicit revision did not advance").unwrap();
    assert!(pipeline::publish(&config.root, &mut ledger, &attempt).is_err());
    assert_eq!(
        ledger.pointer("synthetic", None).unwrap().unwrap().version,
        1
    );
}

#[test]
#[ignore = "requires provisioned isolated ClickHouse/MongoDB and RUSTDX_TEST_* settings"]
fn real_databases_isolate_versions_reject_extra_keys_and_restore_local_evidence() {
    use rustdx_runner::{config::DatabaseTarget, consumer::PinnedDataset};
    let cli = std::env::var_os("RUSTDX_TEST_CLI").unwrap();
    let temp = TempDir::new().unwrap();
    let mut config = fixture(temp.path(), Path::new(&cli));
    config.timeout_seconds = 180;
    config.max_rows = 5000;
    let mut history: serde_json::Value =
        serde_json::from_slice(&fs::read(&config.security_history).unwrap()).unwrap();
    let mut universe: serde_json::Value =
        serde_json::from_slice(&fs::read(&config.universe).unwrap()).unwrap();
    for index in 1..600 {
        let code = format!("sh{}", 600000 + index);
        let mut interval = history["intervals"][0].clone();
        interval["code"] = json!(code);
        history["intervals"].as_array_mut().unwrap().push(interval);
        universe["members"].as_array_mut().unwrap().push(json!({
            "code":code,"from":"2026-07-06","through":"2026-07-07"}));
        let other = temp.path().join("single-source");
        fs::create_dir_all(&other).unwrap();
        write_day(&other, &[20260706, 20260707], 1000);
        fs::rename(
            other.join("sh600000.day"),
            config.day_directories[0].join(format!("{code}.day")),
        )
        .unwrap();
    }
    fs::write(
        &config.security_history,
        serde_json::to_vec(&history).unwrap(),
    )
    .unwrap();
    fs::write(&config.universe, serde_json::to_vec(&universe).unwrap()).unwrap();
    config.dataset_id = format!(
        "runner_synthetic_{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    );
    let url = std::env::var("RUSTDX_TEST_CH_URL").unwrap();
    let mongosh = std::env::var_os("RUSTDX_TEST_MONGOSH").unwrap();
    config.database_targets = vec![
        DatabaseTarget::Clickhouse {
            id: "ch".into(),
            url: url.clone(),
            table: "rustdx_versioned.bars".into(),
            user_env: "RUSTDX_TEST_CH_USER".into(),
            password_env: "RUSTDX_TEST_CH_PASSWORD".into(),
        },
        DatabaseTarget::Mongodb {
            id: "mongo".into(),
            database: "rustdx_versioned".into(),
            collection: "bars".into(),
            uri_env: "RUSTDX_TEST_MONGO_URI".into(),
            mongosh: mongosh.clone().into(),
        },
    ];
    let operation = pipeline::operation(&config, None).unwrap();
    let _scope = operation.enter();
    pipeline::execute(&config, &operation).unwrap();
    let pinned = PinnedDataset::open(&config.root, &config.dataset_id, Some(1)).unwrap();
    config.source_revision = 2;
    write_day(&config.day_directories[0], &[20260706, 20260707], 1100);
    let db = rusqlite::Connection::open(config.root.join("ledger.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER inject_db_catalog_commit BEFORE INSERT ON events WHEN NEW.stage='released'
        BEGIN SELECT RAISE(ABORT,'injected commit after database verification'); END;",
    )
    .unwrap();
    assert!(pipeline::execute(&config, &operation).is_err());
    assert_eq!(
        Ledger::open(&config.root.join("ledger.sqlite"))
            .unwrap()
            .pointer(&config.dataset_id, None)
            .unwrap()
            .unwrap()
            .version,
        1
    );
    db.execute_batch("DROP TRIGGER inject_db_catalog_commit")
        .unwrap();
    assert!(pipeline::recover(&config.root).unwrap()[0].contains("catalog 2"));
    let mut originals = Vec::new();
    for target in ["ch", "mongo"] {
        let path = temp.path().join(format!("{target}.csv"));
        assert_eq!(
            pinned
                .export_database(
                    config.from,
                    config.through,
                    &path,
                    2000,
                    1024 * 1024,
                    target
                )
                .unwrap(),
            1200
        );
        let rows: Vec<rustdx_runner::consumer::CanonicalRow> = csv::Reader::from_path(&path)
            .unwrap()
            .deserialize()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows[0].close, 10.0);
        originals.push((path.clone(), fs::read(path).unwrap()));
    }
    let backup = temp.path().join("backup");
    ops::backup(&config.root, &backup, 1024 * 1024 * 1024).unwrap();
    let restored = temp.path().join("restored");
    ops::restore(&backup, &restored, 1024 * 1024 * 1024).unwrap();
    PinnedDataset::open(&restored, &config.dataset_id, Some(1)).unwrap();
    // DBA-only corruption deliberately appends data to an already released namespace.
    let release = &pinned
        .catalog
        .partitions
        .values()
        .next()
        .unwrap()
        .release_id;
    let sql = format!(
        "INSERT INTO rustdx_versioned.bars SELECT * FROM rustdx_versioned.bars WHERE dataset_id='{}' AND release_id='{release}' LIMIT 1",
        config.dataset_id
    );
    let mut response = rustdx_cli::http::post(&url).send(sql.as_str()).unwrap();
    let mut body = String::new();
    std::io::Read::read_to_string(&mut response.body_mut().as_reader(), &mut body).unwrap();
    assert!(body.is_empty());
    let script = format!(
        "const d=connect(process.env.RUSTDX_TEST_MONGO_URI).getSiblingDB('rustdx_versioned'); const x=d.bars.findOne({{dataset_id:{},release_id:{}}}); delete x._id; x.code='601999'; d.bars.insertOne(x);",
        serde_json::to_string(&config.dataset_id).unwrap(),
        serde_json::to_string(release).unwrap()
    );
    assert!(
        std::process::Command::new(mongosh)
            .args(["--nodb", "--quiet", "--norc", "--eval", &script])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    for (target, (path, original)) in ["ch", "mongo"].into_iter().zip(originals) {
        assert!(
            pinned
                .export_database(
                    config.from,
                    config.through,
                    &path,
                    2000,
                    1024 * 1024,
                    target
                )
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), original);
    }
    // Corrupted remote targets cannot alter the durable published catalog pointer.
    assert_eq!(
        Ledger::open(&config.root.join("ledger.sqlite"))
            .unwrap()
            .pointer(&config.dataset_id, None)
            .unwrap()
            .unwrap()
            .version,
        2
    );
}

#[cfg(unix)]
fn thaw_test_directory(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            thaw_test_directory(&entry.path());
        }
    }
}
