//! Real local ledger and filesystem boundaries; no external services.
use rustdx_runner::{ledger::Ledger, ops, storage};
use std::{fs, path::Path};

fn ledger(root: &Path) {
    fs::create_dir(root).unwrap();
    drop(Ledger::open(&root.join("ledger.sqlite")).unwrap());
}

#[test]
fn readonly_requires_existing_current_schema_and_never_initializes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite");
    assert!(Ledger::open_existing_readonly(&path).is_err());
    assert!(!path.exists());
    let writer = Ledger::open(&path).unwrap();
    drop(writer);
    let before = fs::read(&path).unwrap();
    let reader = Ledger::open_existing_readonly(&path).unwrap();
    assert!(reader.statuses().unwrap().is_empty());
    drop(reader);
    assert_eq!(fs::read(&path).unwrap(), before);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE attempt_observations;
         DROP TABLE observation_counters; DROP TABLE observation_stages;
         DROP INDEX attempts_by_job; DROP INDEX jobs_by_created; PRAGMA user_version=2;",
    )
    .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    assert!(
        Ledger::open_existing_readonly(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("migrate")
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    let lock = storage::RootLock::acquire(temp.path()).unwrap();
    assert!(Ledger::open(&path).is_err());
    drop(Ledger::open_writer(&path, &lock).unwrap());
    assert!(Ledger::open_existing_readonly(&path).is_ok());
}

#[test]
fn readonly_sees_committed_wal_while_writer_holds_lock() {
    let temp = tempfile::tempdir().unwrap();
    let lock = storage::RootLock::acquire(temp.path()).unwrap();
    let mut writer = Ledger::open_writer(&temp.path().join("ledger.sqlite"), &lock).unwrap();
    writer.register("j", "d", "{}").unwrap();
    let reader = Ledger::open_existing_readonly(&temp.path().join("ledger.sqlite")).unwrap();
    assert_eq!(reader.statuses().unwrap()[0].job_id, "j");
    assert!(
        reader
            .record_observations("a", "worker", &serde_json::json!({}))
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn backup_and_restore_reject_parent_aliases_and_traversal_before_writing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("live");
    ledger(&root);
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    assert!(ops::backup(&root, &alias.join("backup"), u64::MAX).is_err());
    assert!(!root.join("backup").exists());
    assert!(ops::backup(&root, &root.join("../sibling"), u64::MAX).is_err());
    let backup = temp.path().join("backup");
    ops::backup(&root, &backup, 1024 * 1024).unwrap();
    let alias = temp.path().join("backup-alias");
    std::os::unix::fs::symlink(&backup, &alias).unwrap();
    assert!(ops::restore(&backup, &alias.join("restored"), u64::MAX).is_err());
    assert!(!backup.join("restored").exists());
}

#[test]
fn total_backup_and_restore_budgets_include_completion_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("live");
    ledger(&root);
    let bytes = fs::metadata(root.join("ledger.sqlite")).unwrap().len();
    let small = temp.path().join("small");
    assert!(ops::backup(&root, &small, bytes).is_err());
    assert!(!small.exists());
    let backup = temp.path().join("backup");
    ops::backup(&root, &backup, bytes + 1024).unwrap();
    let usage = storage::tree_bytes(&backup).unwrap();
    assert!(usage <= bytes + 1024);
    let exact = temp.path().join("exact");
    ops::backup(&root, &exact, usage).unwrap();
    assert!(storage::tree_bytes(&exact).unwrap() <= usage);
    let small = temp.path().join("small-restore");
    assert!(ops::restore(&backup, &small, bytes).is_err());
    assert!(!small.exists());
    let restored = temp.path().join("restored");
    ops::restore(&backup, &restored, bytes + 1024).unwrap();
    assert!(storage::tree_bytes(&restored).unwrap() <= bytes + 1024);
    assert!(restored.join("restore-validated.json").is_file());
}

#[test]
fn prior_schema_backup_restores_unchanged_and_requires_explicit_migration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("live");
    ledger(&root);
    let backup = temp.path().join("backup");
    ops::backup(&root, &backup, 1024 * 1024).unwrap();
    // Construct an old-schema fixture, not a modification of frozen historical evidence.
    let path = backup.join("ledger.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE observation_counters; DROP TABLE observation_stages;
         DROP INDEX attempts_by_job; DROP INDEX jobs_by_created;
         DROP TRIGGER immutable_observations_update; DROP TRIGGER immutable_observations_delete;
         PRAGMA user_version=3;",
    )
    .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    let mut inventory: serde_json::Value =
        serde_json::from_slice(&fs::read(backup.join("backup.json")).unwrap()).unwrap();
    inventory["files"][0] =
        serde_json::to_value(storage::evidence(&backup, &path, 1024 * 1024).unwrap()).unwrap();
    fs::write(
        backup.join("backup.json"),
        serde_json::to_vec(&inventory).unwrap(),
    )
    .unwrap();
    let restored = temp.path().join("restored");
    ops::restore(&backup, &restored, 1024 * 1024).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read(restored.join("ledger.sqlite")).unwrap(), before);
    assert!(Ledger::open_existing_readonly(&restored.join("ledger.sqlite")).is_err());
    let lock = storage::RootLock::acquire(&restored).unwrap();
    drop(Ledger::open_writer(&restored.join("ledger.sqlite"), &lock).unwrap());
    assert!(Ledger::open_existing_readonly(&restored.join("ledger.sqlite")).is_ok());
}
