//! SQLite is local control state, never a distributed lease.
use std::{path::Path, time::Duration};

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};

use crate::{
    Error, Result, invalid,
    model::{Evidence, JobStatus, Pointer, RecoveryAttempt},
    storage::RootLock,
};

pub struct Ledger {
    pub(crate) connection: Connection,
}

pub const SCHEMA_VERSION: i64 = 4;

pub(crate) fn unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn signed(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| invalid("SQLite integer exceeds i64"))
}

impl Ledger {
    /// Compatibility writer opening. Initialization/migration holds the controller lock.
    /// Long-running writers should retain a RootLock and use `open_writer`.
    pub fn open(path: &Path) -> Result<Self> {
        let lock = RootLock::acquire(
            path.parent()
                .ok_or_else(|| invalid("ledger has no parent"))?,
        )?;
        Self::open_writer(path, &lock)
    }

    /// Open an existing ledger without creating, changing journal mode or migrating.
    /// SQLite may require its WAL shared-memory file; this is not an immutable snapshot.
    pub fn open_existing_readonly(path: &Path) -> Result<Self> {
        Self::open_readonly(path, false)
    }

    /// Restore validates the prior schema without migrating frozen backup bytes.
    pub(crate) fn open_backup_readonly(path: &Path) -> Result<Self> {
        Self::open_readonly(path, true)
    }

    fn open_readonly(path: &Path, previous_backup: bool) -> Result<Self> {
        if !std::fs::symlink_metadata(path)?.is_file() {
            return Err(invalid("ledger must be an existing regular file"));
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA query_only=ON;")?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION && !(previous_backup && version == 3) {
            return Err(invalid(format!(
                "ledger schema {version} requires explicit migrate; current schema is {SCHEMA_VERSION}"
            )));
        }
        Ok(Self { connection })
    }

    /// The caller keeps this root's lock through all state-changing operations.
    pub fn open_writer(path: &Path, lock: &RootLock) -> Result<Self> {
        lock.covers(path)?;
        if let Ok(metadata) = std::fs::symlink_metadata(path)
            && !metadata.is_file()
        {
            return Err(invalid("ledger must be a regular file"));
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(invalid("ledger schema is newer than this runner"));
        }
        if version == 0 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE TABLE jobs(
                    id TEXT PRIMARY KEY, dataset TEXT NOT NULL, spec TEXT NOT NULL,
                    state TEXT NOT NULL, created TEXT NOT NULL, detail TEXT NOT NULL DEFAULT ''
                );
                CREATE TABLE attempts(
                    id TEXT PRIMARY KEY, job TEXT NOT NULL REFERENCES jobs(id),
                    state TEXT NOT NULL, directory TEXT NOT NULL UNIQUE,
                    started TEXT NOT NULL, finished TEXT, pid INTEGER, exit_code INTEGER,
                    cli_run TEXT, detail TEXT NOT NULL DEFAULT ''
                );
                CREATE UNIQUE INDEX one_active_job ON attempts(job)
                    WHERE state IN ('running','evaluated','reconciliation_required');
                CREATE TABLE releases(
                    id TEXT PRIMARY KEY, attempt TEXT NOT NULL UNIQUE REFERENCES attempts(id),
                    dataset TEXT NOT NULL, manifest TEXT NOT NULL, state TEXT NOT NULL
                );
                CREATE TABLE publications(
                    dataset TEXT NOT NULL, version INTEGER NOT NULL,
                    pointer TEXT NOT NULL, release TEXT NOT NULL REFERENCES releases(id),
                    committed TEXT NOT NULL, PRIMARY KEY(dataset,version), UNIQUE(release)
                );
                CREATE TABLE pointers(dataset TEXT PRIMARY KEY, version INTEGER NOT NULL,
                    FOREIGN KEY(dataset,version) REFERENCES publications(dataset,version));
                CREATE TABLE events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    object TEXT NOT NULL, stage TEXT NOT NULL, detail TEXT NOT NULL,
                    created TEXT NOT NULL);
                CREATE TRIGGER immutable_events_update BEFORE UPDATE ON events BEGIN
                    SELECT RAISE(ABORT,'events are append-only'); END;
                CREATE TRIGGER immutable_events_delete BEFORE DELETE ON events BEGIN
                    SELECT RAISE(ABORT,'events are append-only'); END;
                PRAGMA user_version=1; COMMIT;",
            )?;
        }
        if version < 2 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE TABLE targets(
                    release TEXT NOT NULL REFERENCES releases(id), id TEXT NOT NULL,
                    spec TEXT NOT NULL, state TEXT NOT NULL, proof TEXT,
                    PRIMARY KEY(release,id));
                CREATE TABLE target_io(
                    release TEXT NOT NULL, target TEXT NOT NULL, sequence INTEGER NOT NULL,
                    is_write INTEGER NOT NULL, request TEXT NOT NULL, response TEXT,
                    PRIMARY KEY(release,target,sequence),
                    FOREIGN KEY(release,target) REFERENCES targets(release,id));
                PRAGMA user_version=2; COMMIT;",
            )?;
        }
        if version < 3 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE TABLE attempt_observations(
                    attempt TEXT NOT NULL REFERENCES attempts(id),
                    owner TEXT NOT NULL CHECK(owner IN ('worker','controller')),
                    payload TEXT NOT NULL, created TEXT NOT NULL,
                    PRIMARY KEY(attempt,owner));
                PRAGMA user_version=3; COMMIT;",
            )?;
        }
        if version < 4 {
            let tx = connection.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE observation_counters(
                    owner TEXT NOT NULL CHECK(owner IN ('worker','controller')),
                    field TEXT NOT NULL CHECK(field IN ('records_complete','records_failed',
                        'records_unavailable','requests','request_failures','empty_responses',
                        'acquisition_timeouts','queue_wait_micros')),
                    total INTEGER NOT NULL CHECK(typeof(total)='integer' AND total>=0),
                    PRIMARY KEY(owner,field));
                 CREATE TABLE observation_stages(
                    owner TEXT NOT NULL CHECK(owner IN ('worker','controller')),
                    stage TEXT NOT NULL CHECK(stage IN ('source','download','extract','snapshot',
                        'parse','sort','write','readback')),
                    calls INTEGER NOT NULL CHECK(typeof(calls)='integer' AND calls>=0),
                    failures INTEGER NOT NULL CHECK(typeof(failures)='integer' AND failures>=0),
                    completed_seconds REAL NOT NULL CHECK(completed_seconds>=0),
                    PRIMARY KEY(owner,stage));
                 CREATE INDEX attempts_by_job ON attempts(job);
                 CREATE INDEX jobs_by_created ON jobs(created DESC);
                 CREATE TRIGGER immutable_observations_update
                    BEFORE UPDATE ON attempt_observations BEGIN
                    SELECT RAISE(ABORT,'observations are append-only'); END;
                 CREATE TRIGGER immutable_observations_delete
                    BEFORE DELETE ON attempt_observations BEGIN
                    SELECT RAISE(ABORT,'observations are append-only'); END;",
            )?;
            crate::observations::rebuild(&tx)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
        }
        Ok(Self { connection })
    }

    /// One durable observation per process owner and attempt. Missing records
    /// remain missing; they are not interpreted as successful zero-network work.
    pub fn record_observations(
        &self,
        attempt: &str,
        owner: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        if !matches!(owner, "worker" | "controller") {
            return Err(invalid("invalid observation owner"));
        }
        let text = serde_json::to_string(payload)?;
        if text.len() > 1024 * 1024 {
            return Err(invalid("observation record exceeds 1MiB"));
        }
        let contribution = crate::observations::Totals::contribution(owner, payload)?;
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO attempt_observations VALUES(?1,?2,?3,?4)",
            params![attempt, owner, text, crate::now()],
        )?;
        contribution.apply(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Rebuild the projection atomically from the retained raw observations.
    pub fn rebuild_observation_metrics(&mut self) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::observations::rebuild(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn register(&mut self, job: &str, dataset: &str, spec: &str) -> Result<String> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = tx
            .query_row("SELECT state,spec FROM jobs WHERE id=?1", [job], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        if let Some((state, previous)) = existing {
            if previous != spec {
                return Err(invalid(
                    "job identity reused with a different spec; change policy/source revision",
                ));
            }
            return Ok(state);
        }
        tx.execute(
            "INSERT INTO jobs(id,dataset,spec,state,created) VALUES(?1,?2,?3,'queued',?4)",
            params![job, dataset, spec, crate::now()],
        )?;
        event(&tx, job, "queued", "registered explicit job")?;
        tx.commit()?;
        Ok("queued".into())
    }

    pub fn begin_attempt(&mut self, job: &str, attempt: &str, directory: &Path) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state: String =
            tx.query_row("SELECT state FROM jobs WHERE id=?1", [job], |r| r.get(0))?;
        if !matches!(
            state.as_str(),
            "queued" | "failed" | "quarantined" | "cancelled"
        ) {
            return Err(invalid(format!("job cannot start from {state}")));
        }
        tx.execute(
            "INSERT INTO attempts(id,job,state,directory,started) VALUES(?1,?2,'running',?3,?4)",
            params![attempt, job, directory.to_string_lossy(), crate::now()],
        )?;
        tx.execute(
            "UPDATE jobs SET state='running',detail='' WHERE id=?1",
            [job],
        )?;
        event(
            &tx,
            attempt,
            "running",
            "worker starting in private workspace",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_pid(&self, attempt: &str, pid: u32) -> Result<()> {
        if self.connection.execute(
            "UPDATE attempts SET pid=?2 WHERE id=?1 AND state='running'",
            params![attempt, pid],
        )? != 1
        {
            return Err(invalid("attempt not running"));
        }
        Ok(())
    }

    pub fn worker_exited(&self, attempt: &str, code: Option<i32>) -> Result<()> {
        self.connection.execute(
            "UPDATE attempts SET pid=NULL,exit_code=?2 WHERE id=?1",
            params![attempt, code],
        )?;
        Ok(())
    }

    pub fn finish(&mut self, attempt: &str, state: &str, detail: &str) -> Result<()> {
        if !matches!(
            state,
            "failed" | "quarantined" | "cancelled" | "reconciliation_required"
        ) {
            return Err(invalid("invalid failure transition"));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (job, current): (String, String) = tx.query_row(
            "SELECT job,state FROM attempts WHERE id=?1",
            [attempt],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !matches!(
            current.as_str(),
            "running" | "evaluated" | "reconciliation_required"
        ) {
            return Err(invalid("terminal attempt cannot change state"));
        }
        tx.execute(
            "UPDATE attempts SET state=?2,detail=?3,finished=?4 WHERE id=?1",
            params![attempt, state, detail, crate::now()],
        )?;
        tx.execute(
            "UPDATE jobs SET state=?2,detail=?3 WHERE id=?1",
            params![job, state, detail],
        )?;
        if state != "reconciliation_required" {
            tx.execute(
                "UPDATE releases SET state='quarantined' WHERE attempt=?1 AND state='sealed'",
                [attempt],
            )?;
        }
        event(&tx, attempt, state, detail)?;
        tx.commit()?;
        Ok(())
    }

    pub fn seal(
        &mut self,
        release: &str,
        attempt: &str,
        dataset: &str,
        manifest: &Evidence,
        cli_run: &str,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state: String =
            tx.query_row("SELECT state FROM attempts WHERE id=?1", [attempt], |r| {
                r.get(0)
            })?;
        if state != "running" {
            return Err(invalid("only a running attempt can become evaluated"));
        }
        tx.execute(
            "INSERT INTO releases VALUES(?1,?2,?3,?4,'sealed')",
            params![release, attempt, dataset, serde_json::to_string(manifest)?],
        )?;
        tx.execute(
            "UPDATE attempts SET state='evaluated',cli_run=?2 WHERE id=?1",
            params![attempt, cli_run],
        )?;
        tx.execute(
            "UPDATE jobs SET state='evaluated' WHERE id=(SELECT job FROM attempts WHERE id=?1)",
            [attempt],
        )?;
        event(
            &tx,
            attempt,
            "evaluated",
            "coverage verified; immutable release sealed",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn pointer(&self, dataset: &str, version: Option<u64>) -> Result<Option<Pointer>> {
        let version = match version {
            Some(v) => v,
            None => self
                .connection
                .query_row(
                    "SELECT version FROM pointers WHERE dataset=?1",
                    [dataset],
                    |r| unsigned(r, 0),
                )
                .optional()?
                .unwrap_or(0),
        };
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT pointer FROM publications WHERE dataset=?1 AND version=?2",
                params![dataset, signed(version)?],
                |r| r.get(0),
            )
            .optional()?;
        json.map(|text| serde_json::from_str(&text).map_err(Into::into))
            .transpose()
    }

    /// File durability and all content checks precede this sole visibility transaction.
    pub fn publish(
        &mut self,
        dataset: &str,
        expected: u64,
        release: &str,
        pointer: &Pointer,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: u64 = tx
            .query_row(
                "SELECT version FROM pointers WHERE dataset=?1",
                [dataset],
                |r| unsigned(r, 0),
            )
            .optional()?
            .unwrap_or(0);
        if current != expected {
            return Err(Error::Conflict(current));
        }
        if pointer.version
            != expected
                .checked_add(1)
                .ok_or_else(|| invalid("version overflow"))?
            || pointer.version > i64::MAX as u64
        {
            return Err(invalid("invalid next catalog version"));
        }
        let (attempt, target, state): (String, String, String) = tx.query_row(
            "SELECT attempt,dataset,state FROM releases WHERE id=?1",
            [release],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if state != "sealed" || target != dataset {
            return Err(invalid("release must be sealed for this dataset"));
        }
        let pending: u64 = tx.query_row(
            "SELECT count(*) FROM targets WHERE release=?1 AND state!='verified'",
            [release],
            |r| unsigned(r, 0),
        )?;
        if pending != 0 {
            return Err(invalid("database targets are not verified"));
        }
        tx.execute(
            "INSERT INTO publications VALUES(?1,?2,?3,?4,?5)",
            params![
                dataset,
                signed(pointer.version)?,
                serde_json::to_string(pointer)?,
                release,
                crate::now()
            ],
        )?;
        tx.execute("INSERT INTO pointers VALUES(?1,?2) ON CONFLICT(dataset) DO UPDATE SET version=excluded.version",
            params![dataset, signed(pointer.version)?])?;
        tx.execute(
            "UPDATE releases SET state='released' WHERE id=?1",
            [release],
        )?;
        tx.execute(
            "UPDATE attempts SET state='released',finished=?2 WHERE id=?1",
            params![attempt, crate::now()],
        )?;
        tx.execute("UPDATE jobs SET state='released',detail='' WHERE id=(SELECT job FROM attempts WHERE id=?1)", [&attempt])?;
        event(
            &tx,
            &attempt,
            "released",
            &format!("catalog_version={}", pointer.version),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn release_evidence(&self, release: &str) -> Result<Evidence> {
        let text: String = self.connection.query_row(
            "SELECT manifest FROM releases WHERE id=?1",
            [release],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Original roots are audit metadata; restoring never rewrites frozen paths.
    pub(crate) fn original_roots(&self) -> Result<Vec<String>> {
        let mut roots = std::collections::BTreeSet::new();
        let mut query = self
            .connection
            .prepare("SELECT id,directory FROM attempts")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            crate::config::identifier(&id)?;
            let directory: String = row.get(1)?;
            let path = Path::new(&directory);
            if !path.is_absolute()
                || path.file_name().is_none_or(|name| name != id.as_str())
                || path
                    .parent()
                    .and_then(Path::file_name)
                    .is_none_or(|name| name != "attempts")
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(invalid(
                    "historical attempt path is not an absolute attempts/id path",
                ));
            }
            let root = path
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| invalid("attempt root missing"))?;
            roots.insert(root.to_string_lossy().into_owned());
        }
        Ok(roots.into_iter().collect())
    }

    pub fn unfinished(&self) -> Result<Vec<RecoveryAttempt>> {
        let mut query = self.connection.prepare(
            "SELECT id,job,state,directory,pid FROM attempts
             WHERE state IN ('running','evaluated','reconciliation_required') ORDER BY started LIMIT 10001")?;
        let rows = query
            .query_map([], |r| {
                Ok(RecoveryAttempt {
                    attempt_id: r.get(0)?,
                    job_id: r.get(1)?,
                    state: r.get(2)?,
                    directory: r.get(3)?,
                    pid: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.len() > 10_000 {
            return Err(invalid(
                "too many unfinished attempts; operator investigation required",
            ));
        }
        Ok(rows)
    }

    pub fn statuses(&self) -> Result<Vec<JobStatus>> {
        let mut query = self.connection.prepare(
            "SELECT j.id,j.dataset,j.state,(SELECT count(*) FROM attempts a WHERE a.job=j.id),j.detail
             FROM jobs j ORDER BY j.created DESC LIMIT 1000")?;
        Ok(query
            .query_map([], |r| {
                Ok(JobStatus {
                    job_id: r.get(0)?,
                    dataset_id: r.get(1)?,
                    state: r.get(2)?,
                    attempts: unsigned(r, 3)?,
                    detail: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn event(tx: &rusqlite::Transaction<'_>, object: &str, stage: &str, detail: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO events(object,stage,detail,created) VALUES(?1,?2,?3,?4)",
        params![object, stage, detail, crate::now()],
    )?;
    Ok(())
}
