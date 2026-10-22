//! Database sinks; manifests are checkpointed before any potentially partial write.
use super::{ClickHouseConfig, database_table, keep_csv};
use crate::{cmd::DayCmd, quality::CollectionReport};
use eyre::Result;
use rustdx_cli::observation::{Stage, measure};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn import_clickhouse(
    output: &str,
    table: &str,
    keep: bool,
    report: &mut CollectionReport,
) -> Result<()> {
    let batch = super::verify::BatchRows::from_env()?;
    let cfg = ClickHouseConfig::from_env();
    measure(Stage::Readback, || {
        report.verify_artifact(Path::new(output))
    })?;
    report.begin_database(table)?;
    measure(Stage::Write, || {
        cfg.insert_csv_tracked(table, Path::new(output), report.query_id.as_deref())
    })?;
    report.database_status = "acknowledged";
    report.checkpoint()?;
    let evidence = measure(Stage::Readback, || {
        super::verify::verify(Path::new(output), table, batch, |fields, keys| {
            // Identifiers are selected from a fixed allowlist; dates and symbols have
            // been parsed before constructing this independent SELECT.
            let limit = keys.len() + 1;
            let keys = keys
                .iter()
                .map(|(date, code)| format!("('{date}','{code}')"))
                .collect::<Vec<_>>()
                .join(",");
            let text = cfg.exec(&format!(
                "SELECT {} FROM {table} FINAL WHERE (date,code) IN ({keys}) \
             LIMIT {} FORMAT CSVWithNames",
                fields.join(","),
                limit
            ))?;
            super::verify::csv_rows(fields, &text)
        })
    })?;
    report.database_verified(evidence)?;
    retain_or_remove(output, keep, report)
}

pub(super) fn mongodb_uri() -> String {
    std::env::var("MONGODB_URI").unwrap_or_else(|_| "mongodb://127.0.0.1:27017".into())
}

pub(crate) fn target_lock(output: &str, table: &str) -> Result<Option<crate::paths::TargetLock>> {
    let origin = match output {
        "clickhouse" => ClickHouseConfig::from_env().url,
        "mongodb" => mongodb_uri(),
        _ => return Ok(None),
    };
    database_table(table)?;
    Ok(Some(crate::paths::TargetLock::acquire(
        output, &origin, table,
    )?))
}

/// A simple, full unique index makes upsert cardinality enforceable. Conflicting
/// indexes or existing duplicate data fail before any data import.
pub(super) fn prepare_mongodb(table: &str) -> Result<()> {
    let (database, collection) = database_table(table)?;
    let script = format!(
        r#"const d=db.getSiblingDB("{database}"); const c=d.getCollection("{collection}");
        const info=d.getCollectionInfos({{name:"{collection}"}})[0]; const opts=info?.options;
        if(opts?.collation && opts.collation.locale!=="simple") throw Error("collection must use simple collation");
        const valid=i=>i.unique===true && !i.sparse && !i.partialFilterExpression &&
          !i.expireAfterSeconds && (!i.collation || i.collation.locale==="simple") &&
          JSON.stringify(i.key)===JSON.stringify({{date:1,code:1}});
        if(!info || !c.getIndexes().some(valid)) c.createIndex({{date:1,code:1}},{{unique:true,name:"rustdx_date_code",collation:{{locale:"simple"}}}});
        if(!c.getIndexes().some(valid)) throw Error("missing full unique date/code index");
        print("rustdx_unique_index_verified");"#
    );
    let scratch = Scratch::new("index")?;
    let mut command = Command::new("mongosh");
    command.args([&mongodb_uri(), "--quiet", "--eval", &script]);
    let output = run_capture(&mut command, &scratch.path, 1024 * 1024)?;
    if output.trim() != "rustdx_unique_index_verified" {
        eyre::bail!("MongoDB唯一索引未确认");
    }
    Ok(())
}

pub fn import_mongodb(cmd: &DayCmd, report: &mut CollectionReport) -> Result<()> {
    let batch = super::verify::BatchRows::from_env()?;
    let (database, collection) = database_table(&cmd.table)?;
    let uri = mongodb_uri();
    measure(Stage::Readback, || {
        report.verify_artifact(Path::new(&cmd.output))
    })?;
    measure(Stage::Write, || prepare_mongodb(&cmd.table))?;
    report.begin_database(&cmd.table)?;
    let mut command = Command::new("mongoimport");
    command.args([
        "--uri",
        &uri,
        "--db",
        database,
        "--collection",
        collection,
        "--type=csv",
        "--file",
        &cmd.output,
        "--headerline",
        "--mode=upsert",
        "--upsertFields=date,code",
        "--stopOnError",
    ]);
    measure(Stage::Write, || {
        run_bounded(&mut command, Duration::from_secs(300))
    })?;
    report.database_status = "acknowledged";
    report.checkpoint()?;
    let evidence = measure(Stage::Readback, || -> Result<_> {
        let mut reader = super::mongo_readback::MongoReadback::new(&uri, database, collection)?;
        let evidence =
            super::verify::verify(Path::new(&cmd.output), &cmd.table, batch, |fields, keys| {
                reader.fetch(fields, keys)
            })?;
        reader.finish()?;
        Ok(evidence)
    })?;
    report.database_verified(evidence)?;
    retain_or_remove(&cmd.output, cmd.keep_csv, report)
}

fn retain_or_remove(output: &str, keep: bool, report: &mut CollectionReport) -> Result<()> {
    if report.database_status != "verified" {
        eyre::bail!("数据库未完成独立回读，不能清理CSV");
    }
    // Persist the possible filesystem outcome first, then record the confirmed state.
    if let Some(artifact) = &mut report.artifact {
        artifact.state = "cleanup_pending";
    }
    report.checkpoint()?;
    keep_csv(&output, keep)?;
    if let Some(artifact) = &mut report.artifact {
        artifact.state = if keep { "retained" } else { "removed" };
        if keep {
            artifact.path = Path::new(output)
                .with_extension("csv")
                .display()
                .to_string();
        }
    }
    report.checkpoint()
}

/// Inherited streams avoid unbounded output buffering and pipe deadlock.
/// A timeout kills and reaps the direct importer; database writes may already have occurred.
fn run_bounded(command: &mut Command, timeout: Duration) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    wait_bounded(command, timeout)
}

fn run_capture(command: &mut Command, path: &Path, limit: u64) -> Result<String> {
    command
        .stdin(Stdio::null())
        .stdout(File::create(path)?)
        .stderr(Stdio::inherit());
    wait_bounded(command, Duration::from_secs(300))?;
    let mut text = String::new();
    File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        eyre::bail!("数据库回读超过{limit}字节限制");
    }
    Ok(text)
}

struct Scratch {
    path: std::path::PathBuf,
}

impl Scratch {
    fn new(stage: &str) -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join("rustdx_database_readback");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!(
            "{}-{stage}-{}.tmp",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        Ok(Self { path })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn wait_bounded(command: &mut Command, timeout: Duration) -> Result<()> {
    let timeout = rustdx_complete::operation::timeout(timeout)?;
    let _permit = rustdx_complete::tcp::acquire_network(timeout)?;
    rustdx_complete::operation::check()?;
    let mut child = command.spawn()?;
    let start = Instant::now();
    loop {
        if let Err(error) = rustdx_complete::operation::check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    eyre::bail!("数据库导入失败: {status}");
                }
                return Ok(());
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                match result {
                    Err(error) => return Err(error.into()),
                    _ => eyre::bail!("数据库导入超时，已终止子进程；部分写入状态待核实"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn subprocess_failure_and_timeout_are_bounded() {
        assert!(
            run_bounded(
                Command::new("sh").args(["-c", "exit 7"]),
                Duration::from_secs(1)
            )
            .is_err()
        );
        let start = Instant::now();
        assert!(run_bounded(Command::new("sleep").arg("5"), Duration::from_millis(50)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        let task =
            rustdx_complete::operation::OperationContext::new(Duration::from_millis(50), None)
                .unwrap();
        let _guard = task.enter();
        let start = Instant::now();
        assert!(run_bounded(Command::new("sleep").arg("5"), Duration::from_secs(300)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn late_database_error_leaves_durable_unknown_status_and_csv() {
        let dir = std::env::temp_dir().join(format!("rustdx-manifest-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("bars.csv");
        std::fs::write(&output, "date,code\n2026-10-09,sh600000\n").unwrap();
        let mut report = CollectionReport::start(output.to_str().unwrap(), None).unwrap();
        report
            .record_rows("sh600000", 1, "2026-10-09", "2026-10-09")
            .unwrap();
        report.stage_artifact(&output).unwrap();
        report.published().unwrap();
        report.begin_database("rustdx.bars").unwrap();
        let (cfg, server) = super::super::tests::mock_http(
            "HTTP/1.1 200 OK\r\nContent-Length: 19\r\nConnection: close\r\n\r\nDB::Exception: fail",
        );
        let result = cfg.insert_csv_tracked("rustdx.bars", &output, report.query_id.as_deref());
        assert!(report.finish(result).is_err());
        let request = server.join().unwrap();
        assert!(request.contains("query_id=rustdx-"));
        let saved: serde_json::Value = serde_json::from_slice(
            &std::fs::read(crate::quality::manifest_path(
                output.to_str().unwrap(),
                None,
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(saved["status"], "failed");
        assert_eq!(saved["database_status"], "unknown");
        assert_eq!(saved["artifact"]["state"], "published");
        assert_eq!(saved["artifact"]["logical_keys"], 1);
        assert!(output.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn acknowledged_import_with_failed_readback_retains_csv_and_evidence() {
        let dir =
            std::env::temp_dir().join(format!("rustdx-readback-failure-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let output = dir.join("bars.csv");
        fs::write(
            &output,
            "date,code,open,high,low,close,amount,vol\n2026-10-09,sh600000,10,11,9,10.5,1000,100\n",
        )
        .unwrap();
        let mut report = CollectionReport::start(output.to_str().unwrap(), None).unwrap();
        report
            .record_rows("sh600000", 1, "2026-10-09", "2026-10-09")
            .unwrap();
        report.stage_artifact(&output).unwrap();
        report.published().unwrap();
        report.verify_artifact(&output).unwrap();
        report.begin_database("rustdx.bars").unwrap();
        let (cfg, server) = super::super::tests::mock_http(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        cfg.insert_csv_tracked("rustdx.bars", &output, report.query_id.as_deref())
            .unwrap();
        server.join().unwrap();
        report.database_status = "acknowledged";
        report.checkpoint().unwrap();
        let (cfg, server) = super::super::tests::mock_http(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\nConnection: close\r\n\r\nfail",
        );
        let result = super::super::verify::verify(
            &output,
            "rustdx.bars",
            super::super::verify::BatchRows::from_env().unwrap(),
            |fields, _| {
                super::super::verify::csv_rows(fields, &cfg.exec("SELECT independent_readback")?)
            },
        );
        assert!(report.finish(result.map(|_| ())).is_err());
        server.join().unwrap();
        assert!(retain_or_remove(output.to_str().unwrap(), false, &mut report).is_err());
        let saved: serde_json::Value = serde_json::from_slice(
            &fs::read(crate::quality::manifest_path(
                output.to_str().unwrap(),
                None,
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(saved["status"], "failed");
        assert_eq!(saved["database_status"], "acknowledged");
        assert_eq!(saved["artifact"]["state"], "published");
        assert!(saved["database_verification"].is_null());
        fs::write(&output, "changed").unwrap();
        assert!(report.verify_artifact(&output).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
