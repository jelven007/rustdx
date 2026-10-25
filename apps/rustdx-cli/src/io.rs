use crate::cmd::DayCmd;
use crate::quality::{CollectionReport, QualityRow};
use eyre::{Result, eyre};
use rustdx_cli::fetch_code::StockList;
use rustdx_cli::http::{self, HttpOperation};
use rustdx_cli::observation::{Observations, Stage, TemporaryFile, TrackedWriter, measure};
use rustdx_complete::file::{
    day::fq::Day,
    gbbq::{Factor, Gbbq},
};
use rustdx_complete::operation::{self, CheckedIo, OperationContext};
use std::{fs, io, path::Path, sync::mpsc, thread, time::Instant};
#[cfg(test)]
use std::{fs::File, io::Write};

const BUFFER_SIZE: usize = 128 * 1024;
mod factors;
mod mongo_readback;
mod rowbinary;
mod schema;
pub(crate) mod sinks;
mod streaming;
mod verify;
pub use factors::tracked_previous;
use schema::validate_clickhouse_schema;

/// Fixed striped workers spool bounded chunks; consume in directory/file order.
fn run_par<F, T>(
    cmd: &DayCmd,
    wtr: &mut csv::Writer<TrackedWriter>,
    parse: F,
    report: &mut CollectionReport,
) -> Result<()>
where
    F: Fn(&str, &[u8], Option<&Factor>) -> eyre::Result<Vec<T>> + Send + Sync,
    T: serde::Serialize + QualityRow + Send,
{
    let start = Instant::now();
    let calendar = crate::quality::tracked_calendar(cmd.calendar.as_deref(), report)?;
    report.calendar = serde_json::to_value(calendar.metadata())?;
    if cmd.workers.is_some_and(|n| !(1..=64).contains(&n)) {
        eyre::bail!("workers必须在1–64之间");
    }
    let hm = cmd.stocklist()?;
    let mut total_files = 0usize;
    let mut total_rows = 0usize;
    let mut failures = Vec::new();
    let mut header = None;
    for dir in &cmd.path {
        // 收集 (完整代码, 文件路径)，代码含市场前缀（如 `sz000001`、`sz200b07`）
        let work: Vec<(String, std::path::PathBuf)> = filter_file(dir)?
            .filter_map(|f| {
                let s = f.to_string_lossy();
                let (b, code) = cmd.filter_ec(&s);
                filter(b, f.as_path(), hm.as_ref(), dir)
                    .unwrap_or(false)
                    .then_some((code, f))
            })
            .take(cmd.amount.unwrap_or(usize::MAX))
            .collect();
        let n = work.len();
        info!("dir: {dir:?} day 文件数量：{n}");
        let take = cmd.amount.unwrap_or(n);

        let mut count = 0;
        let workers = cmd
            .workers
            .unwrap_or_else(|| {
                thread::available_parallelism()
                    .map(|n| n.get().min(8))
                    .unwrap_or(4)
            })
            .min(n.max(1));
        let work = &work;
        let parse = &parse;
        let calendar = calendar.as_ref();
        let context = OperationContext::current();
        let observations = report.observations.clone();

        thread::scope(|s| {
            // Receivers belong to the scope closure: an early error drops them
            // before scope joins workers blocked in send, preventing deadlock.
            let mut receivers = Vec::with_capacity(workers);
            for worker in 0..workers {
                let (tx, rx) = mpsc::sync_channel(1);
                receivers.push(rx);
                let context = context.clone();
                let observations = observations.clone();
                s.spawn(move || {
                    let _guard = context.as_ref().map(OperationContext::enter);
                    let _observations = observations.enter();
                    for i in (worker..work.len()).step_by(workers) {
                        if operation::check().is_err() {
                            break;
                        }
                        let (code, src) = &work[i];
                        let parsed = streaming::parse_source(
                            code,
                            src,
                            &cmd.output,
                            cmd.strict,
                            calendar,
                            parse,
                        );
                        if tx.send(parsed).is_err() {
                            break;
                        }
                    }
                });
            }
            for index in 0..work.len() {
                operation::check()?;
                let parsed = receivers[index % workers].recv().map_err(|e| {
                    operation::check()
                        .err()
                        .map(eyre::Report::from)
                        .unwrap_or_else(|| eyre!("解析线程提前结束: {e}"))
                })?;
                report.sources.extend(parsed.evidence);
                let mut quality = parsed.quality;
                let invalid =
                    !quality.errors.is_empty() || (cmd.strict && !quality.warnings.is_empty());
                if invalid {
                    failures.push(format!(
                        "{}: {}",
                        quality.source,
                        quality
                            .errors
                            .iter()
                            .chain(&quality.warnings)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                    report.files.push(quality);
                    continue;
                }
                if let (Some(first), Some(last)) = (&quality.first_date, &quality.last_date)
                    && let Err(e) = report.record_rows(&quality.code, parsed.rows, first, last)
                {
                    quality.errors.push(e.to_string());
                    failures.push(format!("{}: {e}", quality.source));
                    report.files.push(quality);
                    continue;
                }
                report.files.push(quality);
                count += 1;
                total_rows += parsed.rows;
                if let Some(spool) = parsed.spool {
                    measure(Stage::Write, || -> Result<()> {
                        let mut reader = csv::Reader::from_path(spool.temporary_path())?;
                        if let Some(header) = &header {
                            if reader.headers()? != header {
                                eyre::bail!("来源CSV列定义不一致");
                            }
                        } else {
                            let columns = reader.headers()?.clone();
                            wtr.write_record(&columns)?;
                            header = Some(columns);
                        }
                        let mut record = csv::StringRecord::new();
                        while reader.read_record(&mut record)? {
                            operation::check()?;
                            wtr.write_record(&record)?;
                        }
                        Ok(())
                    })?;
                }
            }
            Ok::<(), eyre::Report>(())
        })?;

        operation::check()?;
        total_files += count;
        print(dir, count, take);
    }
    measure(Stage::Write, || wtr.flush())?;
    report.files.sort_by(|a, b| a.source.cmp(&b.source));
    if !failures.is_empty() {
        eyre::bail!(
            "{}个文件解析失败：\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    if total_files == 0 {
        eyre::bail!("没有符合条件的day文件");
    }
    if total_rows == 0 {
        eyre::bail!("没有可发布的日线记录");
    }

    // 解析后校验报告
    let elapsed = start.elapsed();
    let out_size = wtr
        .get_ref()
        .metadata()
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    info!(
        "解析完成 → 文件 {} 个 / 行 {} 行 / 输出 {:.1} MiB / 耗时 {:.1}s",
        total_files,
        total_rows,
        out_size,
        elapsed.as_secs_f64()
    );
    Ok(())
}

/// 并发解析并输出 CSV（无复权）。
pub fn run_csv(cmd: &DayCmd) -> Result<()> {
    with_report(cmd, |report| {
        export_csv(cmd, report, |code, src, _| {
            Ok(rustdx_complete::file::day::Day::from_bytes_into_vec(
                code, src,
            )?)
        })
    })
}

/// 报告在输出提交及数据库导入之后定案；任何一步失败均记录失败状态。
fn with_report(cmd: &DayCmd, run: impl FnOnce(&mut CollectionReport) -> Result<()>) -> Result<()> {
    let _target = sinks::target_lock(&cmd.output, &cmd.table)?;
    let mut report = CollectionReport::start(&cmd.output, cmd.report.as_deref())?;
    let observations = Observations::current().unwrap_or_else(|| report.observations.clone());
    let _observations = observations.enter();
    let result = run(&mut report);
    report.finish(result)
}

fn export_csv<F, T>(cmd: &DayCmd, report: &mut CollectionReport, parse: F) -> Result<()>
where
    F: Fn(&str, &[u8], Option<&Factor>) -> eyre::Result<Vec<T>> + Send + Sync,
    T: serde::Serialize + QualityRow + Send,
{
    if matches!(cmd.output.as_str(), "clickhouse" | "mongodb") {
        verify::BatchRows::from_env()?;
    }
    if cmd.output == "clickhouse" {
        measure(Stage::Write, || {
            setup_clickhouse(cmd.gbbq.is_some(), &cmd.table)
        })?;
    } else if cmd.output == "mongodb" {
        measure(Stage::Write, || sinks::prepare_mongodb(&cmd.table))?;
    }
    let (atomic, file) = TemporaryFile::create(&cmd.output)?;
    let mut wtr = csv::WriterBuilder::new()
        .buffer_capacity(BUFFER_SIZE)
        .from_writer(file);
    run_par(cmd, &mut wtr, parse, report)?;
    drop(wtr);
    measure(Stage::Readback, || {
        report.verify_csv(atomic.temporary_path())
    })?;
    measure(Stage::Readback, || {
        report.stage_artifact(atomic.temporary_path())
    })?;
    measure(Stage::Write, || atomic.commit())?;
    report.published()?;
    match cmd.output.as_str() {
        "clickhouse" => sinks::import_clickhouse(&cmd.output, &cmd.table, cmd.keep_csv, report),
        "mongodb" => sinks::import_mongodb(cmd, report),
        _ => Ok(()),
    }
}

/// 并发解析并输出 CSV（复权，基于上市日）。
pub fn run_csv_fq(cmd: &DayCmd) -> Result<()> {
    with_report(cmd, |report| {
        // 股本变迁
        let (evidence, bytes) =
            crate::quality::source_snapshot(cmd.gbbq.as_ref().unwrap(), 64 * 1024 * 1024)?;
        report.sources.push(evidence);
        let mut source = rustdx_complete::file::gbbq::Gbbqs::from_bytes(bytes)?;
        let gbbq = Gbbq::filter_hashmap(source.try_to_vec()?.into_iter());

        export_csv(cmd, report, |code, src, carry| {
            let events = gbbq.get(code).map(Vec::as_slice);
            Ok(if let Some(carry) = carry {
                Day::concat_bytes(code, src, events, Some(carry))?
            } else {
                Day::from_bytes(code, src, events)?
            })
        })
    })
}

/// 并发解析并输出 CSV（复权，基于前一日的因子）。
pub fn run_csv_fq_previous(cmd: &DayCmd) -> Result<()> {
    with_report(cmd, |report| {
        // 股本变迁
        let (evidence, bytes) =
            crate::quality::source_snapshot(cmd.gbbq.as_ref().unwrap(), 64 * 1024 * 1024)?;
        report.sources.push(evidence);
        let mut source = rustdx_complete::file::gbbq::Gbbqs::from_bytes(bytes)?;
        let gbbq = Gbbq::filter_hashmap(source.try_to_vec()?.into_iter());

        // 前收
        let previous = tracked_previous(&cmd.previous, &cmd.table, cmd.keep_factor, report)?;

        export_csv(cmd, report, |code, src, carry| {
            let factor = carry
                .or_else(|| previous.get(code))
                .ok_or_else(|| eyre!("{}缺少前因子", code))?;
            Ok(Day::concat_bytes(
                code,
                src,
                // 无分红数据并不意味着无复权数据
                gbbq.get(code).map(Vec::as_slice),
                Some(factor),
            )?)
        })
    })
}

/// 筛选 day 文件
fn filter_file(dir: &Path) -> Result<impl Iterator<Item = std::path::PathBuf>> {
    let mut files = dir
        .read_dir()?
        .map(|e| e.map(|f| f.path()))
        .collect::<io::Result<Vec<_>>>()?;
    files.retain(|p| p.is_file() && p.extension().is_some_and(|s| s == "day"));
    files.sort();
    Ok(files.into_iter())
}

/// 筛选存在于股票列表的股票
#[inline]
fn filter(b: bool, src: &Path, hm: Option<&StockList>, dir: &Path) -> Option<bool> {
    let src = src.strip_prefix(dir).ok()?.to_str()?.strip_suffix(".day")?;
    Some(b && hm.map(|m| m.contains(src)).unwrap_or(true))
}

fn print(dir: &Path, count: usize, take: usize) {
    if count == 0 && take != 0 {
        error!("{dir:?} 目录下无 `.day` 文件符合要求");
    } else if take == 0 {
        error!("请输入大于 0 的文件数量");
    } else {
        info!("{dir:?}\t已完成：{count}");
    }
}

fn database_table(table: &str) -> Result<(&str, &str)> {
    let (database, name) = table
        .split_once('.')
        .ok_or_else(|| eyre!("表名必须为 database.table"))?;
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
    };
    if !valid(database) || !valid(name) {
        eyre::bail!("数据库及表名只允许字母、数字和下划线，不能以数字开头");
    }
    Ok((database, name))
}

/// ClickHouse HTTP 连接配置（环境变量，约定与生态一致）。
#[derive(Debug, Clone)]
struct ClickHouseConfig {
    url: String,
    user: String,
    password: String,
}

impl ClickHouseConfig {
    fn from_env() -> Self {
        Self {
            url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into()),
            user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into()),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default(),
        }
    }

    /// 执行 SQL（DDL/DML，body 即语句）。
    fn exec(&self, sql: &str) -> Result<String> {
        let mut operation = HttpOperation::new(std::time::Duration::from_secs(300))?;
        let mut req = http::post(&self.url)
            .query("wait_end_of_query", "1")
            // A maximum-size readback includes up to 16,384 validated keys.
            .query("max_query_size", "1048576")
            // Bounded readbacks do not need a server-side formatter pool.
            // Also avoids a formatter scheduling stall observed on macOS CH.
            .query("output_format_parallel_formatting", "0")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(operation.timeout()))
            .build()
            .header("Content-Type", "text/plain; charset=utf-8");
        // 认证 header（ClickHouse 原生 X-ClickHouse-User/Key）
        if self.user != "default" || !self.password.is_empty() {
            req = req.header("X-ClickHouse-User", &self.user);
        }
        if !self.password.is_empty() {
            req = req.header("X-ClickHouse-Key", &self.password);
        }
        let resp = req.send(sql.as_bytes())?;
        let body = Self::response_body(resp)?;
        // Empty acknowledgments for DDL/INSERT are expected, not empty bars.
        operation.complete(false)?;
        Ok(body)
    }

    fn response_body(mut resp: ureq::http::Response<ureq::Body>) -> Result<String> {
        let status = resp.status().as_u16();
        let exception = resp.headers().get("X-ClickHouse-Exception-Code").cloned();
        // 必须读到完整正文；200响应仍可能在正文末尾携带ClickHouse异常。
        let body = http::text(resp.body_mut(), 16 * 1024 * 1024)?;
        if !(200..300).contains(&status) || exception.is_some() || body.contains("DB::Exception:") {
            eyre::bail!("ClickHouse HTTP {status}: {body}");
        }
        Ok(body)
    }

    /// CSV流式转为RowBinary；避免服务端十进制解析改变Float64最低位。
    #[cfg(test)]
    fn insert_csv(&self, table: &str, csv: &std::path::Path) -> Result<()> {
        self.insert_csv_tracked(table, csv, None)
    }

    fn insert_csv_tracked(&self, table: &str, csv: &Path, query_id: Option<&str>) -> Result<()> {
        database_table(table)?;
        let mut operation = HttpOperation::new(std::time::Duration::from_secs(300))?;
        let file = std::fs::File::open(csv)?;
        let len = file.metadata()?.len();
        let mut rows = rowbinary::RowBinary::new(file)?;
        let sql = format!(
            "INSERT INTO {table} ({}) SETTINGS input_format_defaults_for_omitted_fields=1 FORMAT RowBinary",
            rows.columns()
        );
        let mut request = http::post(&self.url)
            .query("query", sql)
            .query("wait_end_of_query", "1");
        if let Some(query_id) = query_id {
            request = request.query("query_id", query_id);
        }
        let mut req = request
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(operation.timeout()))
            .build();
        if self.user != "default" || !self.password.is_empty() {
            req = req.header("X-ClickHouse-User", &self.user);
        }
        if !self.password.is_empty() {
            req = req.header("X-ClickHouse-Key", &self.password);
        }
        let mut rows = CheckedIo(&mut rows);
        let resp = req.send(ureq::SendBody::from_reader(&mut rows))?;
        let body = Self::response_body(resp)?;
        if !body.trim().is_empty() {
            eyre::bail!("ClickHouse INSERT 返回非空响应，写入状态需核实：{body}");
        }
        operation.complete(false)?;
        debug!("clickhouse 导入完成：{table}（{len} 字节 CSV）");
        Ok(())
    }
}

/// ClickHouse HTTP 执行 SQL（建库建表等 DDL）。
fn ch_execute(cfg: &ClickHouseConfig, sql: &str) -> Result<()> {
    cfg.exec(sql).map(|_| ())
}

pub fn setup_clickhouse(_fq: bool, table: &str) -> Result<()> {
    let (database, name) = database_table(table)?;
    let cfg = ClickHouseConfig::from_env();
    let create_database = format!("CREATE DATABASE IF NOT EXISTS {database}");
    ch_execute(&cfg, &create_database)?;
    #[rustfmt::skip]
    let create_table = format!("
            CREATE TABLE IF NOT EXISTS {table}
            (
                `date` Date CODEC(DoubleDelta),
                `code` String,
                `open` Float64,
                `high` Float64,
                `low` Float64,
                `close` Float64,
                `amount` Float64,
                `vol` Float64,
                `preclose` Float64 DEFAULT 0,
                `factor` Float64 DEFAULT 0,
                `version` UInt64 DEFAULT toUnixTimestamp64Nano(now64(9))
            )
            ENGINE = ReplacingMergeTree(version)
            ORDER BY (date, code)
        ");
    ch_execute(&cfg, &create_table)?;
    let metadata = cfg.exec(&format!(
        "SELECT engine_full, sorting_key, partition_key FROM system.tables \
         WHERE database='{database}' AND name='{name}' FORMAT TabSeparated"
    ))?;
    let columns = cfg.exec(&format!(
        "SELECT name, type, default_kind, default_expression FROM system.columns \
         WHERE database='{database}' AND table='{name}' FORMAT TabSeparated"
    ))?;
    validate_clickhouse_schema(&metadata, &columns).map_err(|e| {
        eyre!("{table}表结构不兼容：{e}；请按 docs/release/MIGRATION.md 迁移，不会自动修改原表")
    })
}

#[cfg(test)]
pub fn insert_clickhouse(output: &impl AsRef<Path>, table: &str, keep: bool) -> Result<()> {
    let cfg = ClickHouseConfig::from_env();
    cfg.insert_csv(table, output.as_ref())?;
    info!("成功插入数据到 clickhouse 数据库（{table}）");
    keep_csv(output, keep)?;
    Ok(())
}

/// 需要可用的 ClickHouse HTTP 服务；CI 通过跳过变量禁用外部服务测试。
#[test]
fn test_insert_clickhouse() -> Result<()> {
    if std::env::var_os("RUSTDX_SKIP_INTEGRATION_TESTS").is_some() {
        return Ok(());
    }
    let table = format!("rustdx_test.acceptance_{}", std::process::id());
    let incompatible = format!("{table}_bad_key");
    let path = std::env::temp_dir().join(format!("rustdx-clickhouse-{}.input", std::process::id()));
    let csv_path = path.with_extension("csv");
    fs::write(
        &path,
        "date,code,open,high,low,close,amount,vol\n\
         2026-10-08,sh600000,10,11,9,10.5,1000,100\n\
         2026-10-09,sh600000,10.5,11,10,10.8,1200,120\n",
    )?;
    let config = ClickHouseConfig::from_env();
    let result = (|| -> Result<()> {
        setup_clickhouse(false, &table)?;
        config.exec(&format!(
            "CREATE TABLE {incompatible} AS {table} ENGINE=ReplacingMergeTree(version) ORDER BY (date, code, close)"
        ))?;
        if setup_clickhouse(false, &incompatible).is_ok() {
            eyre::bail!("错误排序键的ClickHouse表未被拒绝");
        }
        insert_clickhouse(&path, &table, true)?;
        insert_clickhouse(&csv_path, &table, false)?;
        let logical_count = config
            .exec(&format!("SELECT count() FROM {table} FINAL"))?
            .trim()
            .parse::<usize>()?;
        config.exec(&format!("OPTIMIZE TABLE {table} FINAL"))?;
        let merged_count = config
            .exec(&format!("SELECT count() FROM {table}"))?
            .trim()
            .parse::<usize>()?;
        if logical_count != 2 || merged_count != 2 {
            eyre::bail!("ClickHouse重复写入去重错误：FINAL={logical_count}，合并后={merged_count}");
        }
        Ok(())
    })();
    let cleanup = config.exec(&format!("DROP TABLE IF EXISTS {table}"));
    let cleanup_incompatible = config.exec(&format!("DROP TABLE IF EXISTS {incompatible}"));
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&csv_path);
    result?;
    cleanup?;
    cleanup_incompatible?;
    Ok(())
}

pub fn run_mongodb(cmd: &DayCmd) -> Result<()> {
    cmd.run_csv()
}

fn keep_csv(fname: &impl AsRef<Path>, keep: bool) -> io::Result<()> {
    if keep {
        fs::rename(fname, fname.as_ref().with_extension("csv"))
    } else {
        fs::remove_file(fname)
    }
}

/// 读取本地 xls(x) 文件
pub fn read_xlsx(path: &str, col: usize, prefix: &str) -> Result<StockList> {
    use calamine::{Data, Reader, open_workbook_auto};
    let mut workbook = open_workbook_auto(path)?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| eyre!("Excel没有工作表"))??;
    if col >= range.width() {
        eyre::bail!("Excel列索引{col}超出列数{}", range.width());
    }
    let mut result = StockList::new();
    for (index, row) in range.rows().enumerate().skip(1) {
        operation::check()?;
        let code = match row.get(col) {
            None | Some(Data::Empty) => continue,
            Some(Data::Int(x)) if (0..=999999).contains(x) => format!("{x:06}"),
            Some(Data::Float(x))
                if x.is_finite() && x.fract() == 0.0 && (0.0..=999999.0).contains(x) =>
            {
                format!("{:06}", *x as u32)
            }
            Some(Data::String(x)) => {
                let value = x.trim();
                if value.is_empty() {
                    continue;
                }
                if value.len() <= 6 && value.bytes().all(|b| b.is_ascii_digit()) {
                    format!("{:0>6}", value)
                } else {
                    value.to_owned()
                }
            }
            _ => {
                eyre::bail!("Excel第{}行不是合法证券代码", index + 1);
            }
        };
        let full = if code.len() == 8 {
            code
        } else {
            format!("{}{}", crate::cmd::auto_prefix(prefix, &code), code)
        };
        full.parse::<rustdx_complete::data::Symbol>()?;
        result.insert(full);
    }
    if result.is_empty() {
        eyre::bail!("Excel股票列表为空");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excel_codes_keep_leading_zero_and_invalid_columns_fail() {
        let dir = std::env::temp_dir().join(format!("rustdx-excel-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stocks.xlsx");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, content) in [
            (
                "_rels/.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Stocks" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:A2"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>code</t></is></c></row><row r="2"><c r="A2"><v>1</v></c></row></sheetData></worksheet>"#,
            ),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        assert!(
            read_xlsx(path.to_str().unwrap(), 0, "sz")
                .unwrap()
                .contains("sz000001")
        );
        assert!(read_xlsx(path.to_str().unwrap(), 10, "sz").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_collection_preserves_last_success_and_records_failure() {
        use argh::FromArgs;
        let dir = std::env::temp_dir().join(format!("rustdx-quality-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("sh600000.day");
        fs::write(source, [0; 31]).unwrap();
        let output = dir.join("output.csv");
        let report = dir.join("report.json");
        fs::write(&output, b"previous success").unwrap();
        let args = [
            dir.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ];
        let cmd = DayCmd::from_args(&["rustdx", "day"], &args).unwrap();
        assert!(run_csv(&cmd).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous success");
        let result: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(result["status"], "failed");
        assert!(!result["files"][0]["errors"].as_array().unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn all_empty_collection_preserves_output_and_fails_report() {
        use argh::FromArgs;
        let dir = std::env::temp_dir().join(format!("rustdx-empty-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("sh600000.day"), []).unwrap();
        let output = dir.join("output.csv");
        let report = dir.join("report.json");
        fs::write(&output, b"previous success").unwrap();
        let cmd = DayCmd::from_args(
            &["rustdx", "day"],
            &[
                dir.to_str().unwrap(),
                "-o",
                output.to_str().unwrap(),
                "--report",
                report.to_str().unwrap(),
            ],
        )
        .unwrap();
        assert!(run_csv(&cmd).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous success");
        let result: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(result["status"], "failed");
        assert!(!result["errors"].as_array().unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn external_calendar_enables_strict_collection_without_guessing_dates() {
        use argh::FromArgs;
        let dir = std::env::temp_dir().join(format!("rustdx-calendar-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut record = Vec::new();
        for value in [20230103u32, 1000, 1100, 900, 1050] {
            record.extend(value.to_le_bytes());
        }
        record.extend(1000.0f32.to_le_bytes());
        record.extend(100u32.to_le_bytes());
        record.extend(0u32.to_le_bytes());
        fs::write(dir.join("sh600000.day"), record).unwrap();
        let output = dir.join("output.csv");
        let report_path = dir.join("report.json");
        let calendar_path = dir.join("calendar.json");
        fs::write(&output, "previous success").unwrap();
        let mut args = vec![
            dir.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--report",
            report_path.to_str().unwrap(),
            "--strict",
        ];
        let cmd = DayCmd::from_args(&["rustdx", "day"], &args).unwrap();
        assert!(run_csv(&cmd).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous success");
        fs::write(
            &calendar_path,
            r#"{"version":"test-local-day","source":"unit-test",
            "start":"2023-01-03","end":"2023-01-03","holidays":[]}"#,
        )
        .unwrap();
        args.extend(["--calendar", calendar_path.to_str().unwrap()]);
        let cmd = DayCmd::from_args(&["rustdx", "day"], &args).unwrap();
        run_csv(&cmd).unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["status"], "complete");
        assert_eq!(report["calendar"]["version"], "test-local-day");
        let published = fs::read(&output).unwrap();
        fs::write(
            &calendar_path,
            r#"{"version":"","source":"","start":"2023-01-03","end":"2023-01-03","holidays":[]}"#,
        )
        .unwrap();
        assert!(run_csv(&cmd).is_err());
        assert_eq!(fs::read(&output).unwrap(), published);
        fs::remove_dir_all(dir).unwrap();
    }

    pub(super) fn mock_http(
        response: &'static str,
    ) -> (ClickHouseConfig, std::thread::JoinHandle<String>) {
        use std::io::{BufRead, BufReader, Read};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let cfg = ClickHouseConfig {
            url: format!(
                "http://{}/?database=default",
                listener.local_addr().unwrap()
            ),
            user: "default".into(),
            password: String::new(),
        };
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            let header = String::from_utf8(header).unwrap();
            // Acknowledge only after consuming the upload. Closing with unread
            // RowBinary chunks can reset the connection before the client sends EOF.
            {
                let mut reader = BufReader::new(&mut socket);
                if header
                    .lines()
                    .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
                {
                    loop {
                        let mut line = String::new();
                        assert!(reader.read_line(&mut line).unwrap() > 0);
                        let length = usize::from_str_radix(line.trim_end(), 16).unwrap();
                        assert!(length <= 1024 * 1024);
                        if length == 0 {
                            // Consume the empty trailer section before responding.
                            let mut trailer = String::new();
                            reader.read_line(&mut trailer).unwrap();
                            assert_eq!(trailer, "\r\n");
                            break;
                        }
                        let mut chunk = vec![0; length];
                        reader.read_exact(&mut chunk).unwrap();
                        let mut end = [0; 2];
                        reader.read_exact(&mut end).unwrap();
                        assert_eq!(end, *b"\r\n");
                    }
                } else if let Some(length) = header.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                }) {
                    assert!(length <= 1024 * 1024);
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                }
            }
            socket.write_all(response.as_bytes()).unwrap();
            header
        });
        (cfg, server)
    }

    #[test]
    fn clickhouse_rejects_error_status_late_exception_and_truncated_body() {
        for response in [
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\nConnection: close\r\n\r\nfail",
            "HTTP/1.1 200 OK\r\nContent-Length: 19\r\nConnection: close\r\n\r\nDB::Exception: fail",
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial",
            "HTTP/1.1 200 OK\r\nX-ClickHouse-Exception-Code: 60\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ] {
            let (cfg, server) = mock_http(response);
            assert!(cfg.exec("SELECT 1").is_err());
            let request = server.join().unwrap();
            assert!(request.contains("wait_end_of_query=1"));
            assert!(request.contains("database=default"));
        }
        let path =
            std::env::temp_dir().join(format!("rustdx-http-insert-{}.csv", std::process::id()));
        fs::write(
            &path,
            "date,code,open,high,low,close,amount,vol\n\
             2005-01-13,sh600000,6.95,7.02,6.94,6.96,8976785,12848.27\n",
        )
        .unwrap();
        let (cfg, server) = mock_http(
            "HTTP/1.1 200 OK\r\nContent-Length: 19\r\nConnection: close\r\n\r\nDB::Exception: fail",
        );
        assert!(
            cfg.insert_csv_tracked("rustdx.bars", &path, Some("rowbinary-regression"))
                .is_err()
        );
        let request = server.join().unwrap();
        assert!(request.contains("wait_end_of_query=1"));
        assert!(request.contains("query=INSERT"));
        assert!(request.contains("RowBinary"));
        assert!(!request.contains("CSVWithNames"));
        assert!(request.contains("query_id=rowbinary-regression"));
        assert!(path.exists());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn clickhouse_schema_must_preserve_key_precision_and_import_versions() {
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
        let metadata = "ReplacingMergeTree(version)\tdate, code\t\n";
        validate_clickhouse_schema(metadata, &columns).unwrap();
        validate_clickhouse_schema(
            "ReplacingMergeTree(version) ORDER BY (date, code) SETTINGS index_granularity = 8192\tdate, code\t\n",
            &columns,
        ).unwrap();
        validate_clickhouse_schema(
            "ReplacingMergeTree(version)\tdate, code\ttoYYYYMM(date)\n",
            &columns,
        )
        .unwrap();
        for invalid in [
            "ReplacingMergeTree()\tdate, code\t\n",
            "ReplacingMergeTree(version)\tdate, code, close\t\n",
            "ReplacingMergeTree(version)\tdate, code\tversion\n",
        ] {
            assert!(validate_clickhouse_schema(invalid, &columns).is_err());
        }
        assert!(
            validate_clickhouse_schema(metadata, &columns.replace("Float64", "Float32")).is_err()
        );
        assert!(
            validate_clickhouse_schema(
                metadata,
                &columns.replace("toUnixTimestamp64Nano(now64(9))", "0")
            )
            .is_err()
        );
    }

    #[test]
    fn table_names_cannot_be_sql() {
        assert_eq!(database_table("rustdx.tmp").unwrap(), ("rustdx", "tmp"));
        for s in [
            "tmp",
            ".tmp",
            "db.",
            "db.t;DROP TABLE x",
            "db.t.extra",
            "db.1table",
        ] {
            assert!(database_table(s).is_err());
        }
    }
}
