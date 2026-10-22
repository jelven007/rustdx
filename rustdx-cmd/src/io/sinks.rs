//! Database sinks; manifests are checkpointed before any potentially partial write.
use super::{ClickHouseConfig, database_table, keep_csv};
use crate::{cmd::DayCmd, quality::CollectionReport};
use eyre::Result;
use std::{
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
    report.begin_database(table)?;
    ClickHouseConfig::from_env().insert_csv_tracked(
        table,
        Path::new(output),
        report.query_id.as_deref(),
    )?;
    report.database_status = "acknowledged";
    report.checkpoint()?;
    retain_or_remove(output, keep, report)
}

pub fn import_mongodb(cmd: &DayCmd, report: &mut CollectionReport) -> Result<()> {
    let (database, collection) = database_table(&cmd.table)?;
    report.begin_database(&cmd.table)?;
    let mut command = Command::new("mongoimport");
    command.args([
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
    ]);
    run_bounded(&mut command, Duration::from_secs(300))?;
    report.database_status = "acknowledged";
    report.checkpoint()?;
    retain_or_remove(&cmd.output, cmd.keep_csv, report)
}

fn retain_or_remove(output: &str, keep: bool, report: &mut CollectionReport) -> Result<()> {
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
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let start = Instant::now();
    loop {
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
}
