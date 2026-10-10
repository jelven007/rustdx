use crate::cmd::DayCmd;
use crate::quality::{CollectionReport, QualityRow};
use eyre::{Result, anyhow, eyre};
use rustdx_cli::fetch_code::StockList;
use rustdx_complete::file::{
    day::fq::Day,
    gbbq::{Factor, Gbbq},
};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

const BUFFER_SIZE: usize = 32 * (1 << 20); // 32M

/// 并发解析目录下的 day 文件并写入 CSV（利用多核优势）。
///
/// - 解析阶段：多个工作线程（默认逻辑核心数）并行读取并解析 `.day` 文件；
/// - 写入阶段：主线程从 channel 顺序接收解析结果并写入 CSV，避免并发写文件。
///
/// `parse` 负责把单个文件解析成 `Vec<T>`，`T` 为可序列化的日线类型。
fn run_par<F, T>(cmd: &DayCmd, wtr: &mut csv::Writer<File>, parse: F) -> Result<()>
where
    F: Fn(&str, &Path) -> eyre::Result<Vec<T>> + Send + Sync,
    T: serde::Serialize + QualityRow + Send,
{
    let start = Instant::now();
    if cmd.workers.is_some_and(|n| !(1..=64).contains(&n)) {
        eyre::bail!("workers必须在1–64之间");
    }
    let hm = cmd.stocklist()?;
    let mut total_files = 0usize;
    let mut total_rows = 0usize;
    let mut failures = Vec::new();
    let mut report = CollectionReport::new(&cmd.output);
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

        let count = AtomicUsize::new(0); // 成功解析的文件数
        let next = AtomicUsize::new(0); // 任务队列：原子取下一个文件
        let workers = cmd
            .workers
            .unwrap_or_else(|| {
                thread::available_parallelism()
                    .map(|n| n.get().min(8))
                    .unwrap_or(4)
            })
            .min(n.max(1));
        let (tx, rx) = mpsc::sync_channel::<(String, eyre::Result<Vec<T>>)>(workers.max(1) * 2);

        // 所有 worker 共享的引用：原子计数器与任务列表
        let count = &count;
        let next = &next;
        let work = &work;
        let parse = &parse;

        thread::scope(|s| {
            for _ in 0..workers {
                let tx = tx.clone();
                s.spawn(move || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= work.len() {
                            break;
                        }
                        let (code, src) = &work[i];
                        let result =
                            fs::metadata(src)
                                .map_err(eyre::Report::from)
                                .and_then(|metadata| {
                                    if metadata.len() > 32 * 1024 * 1024 {
                                        eyre::bail!("单个day文件超出32MiB限制");
                                    }
                                    parse(code, src)
                                });
                        if result.is_ok() {
                            count.fetch_add(1, Ordering::Relaxed);
                        }
                        if tx.send((src.display().to_string(), result)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx); // 关闭发送端，主线程的迭代才能结束
            for (path, result) in rx {
                let v = match result {
                    Ok(v) => v,
                    Err(e) => {
                        failures.push(format!("{path}: {e:#}"));
                        report.files.push(crate::quality::FileReport {
                            source: path,
                            code: String::new(),
                            rows: 0,
                            first_date: None,
                            last_date: None,
                            errors: vec![e.to_string()],
                            warnings: vec![],
                        });
                        continue;
                    }
                };
                let code = Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_owned();
                let mut quality = crate::quality::inspect(path.clone(), code, &v);
                let invalid =
                    !quality.errors.is_empty() || (cmd.strict && !quality.warnings.is_empty());
                if invalid {
                    failures.push(format!(
                        "{path}: {}",
                        quality
                            .errors
                            .iter()
                            .chain(&quality.warnings)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                }
                if invalid {
                    report.files.push(quality);
                    continue;
                }
                let source_rows = v.len();
                let v = normalize_rows(v)?;
                if v.len() != source_rows {
                    quality.warnings.push(format!(
                        "输出前按日期去重，保留源文件最后一条：{}条 -> {}条",
                        source_rows,
                        v.len()
                    ));
                }
                report.files.push(quality);
                total_rows += v.len();
                for t in v {
                    wtr.serialize(t)?;
                }
            }
            Ok::<(), eyre::Report>(())
        })?;

        total_files += count.load(Ordering::Relaxed);
        print(dir, count.load(Ordering::Relaxed), take);
    }
    wtr.flush()?;
    report.files.sort_by(|a, b| a.source.cmp(&b.source));
    if let Some(path) = &cmd.report {
        report.write(path, failures.is_empty() && total_files > 0)?;
    }
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

fn normalize_rows<T: QualityRow>(rows: Vec<T>) -> Result<Vec<T>> {
    let mut dated = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| Ok((row.date()?, index, row)))
        .collect::<Result<Vec<_>>>()?;
    dated.sort_by_key(|(date, index, _)| (*date, *index));

    let mut seen = std::collections::HashSet::with_capacity(dated.len());
    let mut normalized = Vec::with_capacity(dated.len());
    for (date, _, row) in dated.into_iter().rev() {
        if seen.insert(date) {
            normalized.push(row);
        }
    }
    normalized.reverse();
    Ok(normalized)
}

/// 并发解析并输出 CSV（无复权）。
pub fn run_csv(cmd: &DayCmd) -> Result<()> {
    let (atomic, file) = rustdx_complete::atomic_file::AtomicFile::create(&cmd.output)?;
    let mut wtr = csv::WriterBuilder::new()
        .buffer_capacity(BUFFER_SIZE)
        .from_writer(file);
    run_par(cmd, &mut wtr, |code, src| {
        Ok(rustdx_complete::file::day::Day::from_file_into_vec(
            code, src,
        )?)
    })?;
    drop(wtr);
    atomic.commit()?;
    Ok(())
}

/// 并发解析并输出 CSV（复权，基于上市日）。
pub fn run_csv_fq(cmd: &DayCmd) -> Result<()> {
    // 股本变迁
    let mut source = rustdx_complete::file::gbbq::Gbbqs::from_file(cmd.gbbq.as_ref().unwrap())?;
    let gbbq = Gbbq::filter_hashmap(source.to_vec().into_iter());

    let (atomic, file) = rustdx_complete::atomic_file::AtomicFile::create(&cmd.output)?;
    let mut wtr = csv::WriterBuilder::new()
        .buffer_capacity(BUFFER_SIZE)
        .from_writer(file);
    run_par(cmd, &mut wtr, |code, src| {
        Ok(Day::new(code, src, gbbq.get(code).map(Vec::as_slice))?)
    })?;
    drop(wtr);
    atomic.commit()?;
    Ok(())
}

/// 并发解析并输出 CSV（复权，基于前一日的因子）。
pub fn run_csv_fq_previous(cmd: &DayCmd) -> Result<()> {
    // 股本变迁
    let mut source = rustdx_complete::file::gbbq::Gbbqs::from_file(cmd.gbbq.as_ref().unwrap())?;
    let gbbq = Gbbq::filter_hashmap(source.to_vec().into_iter());

    // 前收
    let previous = previous_csv_table(&cmd.previous, &cmd.table, cmd.keep_factor)?;

    let (atomic, file) = rustdx_complete::atomic_file::AtomicFile::create(&cmd.output)?;
    let mut wtr = csv::WriterBuilder::new()
        .buffer_capacity(BUFFER_SIZE)
        .from_writer(file);
    run_par(cmd, &mut wtr, |code, src| {
        let factor = previous
            .get(code)
            .ok_or_else(|| eyre!("{}缺少前因子", code))?;
        Ok(Day::concat(
            code,
            src,
            // 无分红数据并不意味着无复权数据
            gbbq.get(code).map(Vec::as_slice),
            Some(factor),
        )?)
    })?;
    drop(wtr);
    atomic.commit()?;
    Ok(())
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
        let mut req = ureq::post(&self.url).header("Content-Type", "text/plain; charset=utf-8");
        // 认证 header（ClickHouse 原生 X-ClickHouse-User/Key）
        if self.user != "default" || !self.password.is_empty() {
            req = req.header("X-ClickHouse-User", &self.user);
        }
        if !self.password.is_empty() {
            req = req.header("X-ClickHouse-Key", &self.password);
        }
        let mut resp = req.send(sql.as_bytes()).map_err(|e| match e {
            // 401/403 等鉴权类失败在 send 阶段即返回 StatusCode（ureq 3 不携带
            // body）——实测多因 cwd 不在部署目录、CLICKHOUSE_* env 缺失
            ureq::Error::StatusCode(code) => eyre!(
                "ClickHouse HTTP {code}\
                （鉴权/权限失败常见：CLICKHOUSE_URL/CLICKHOUSE_USER/\
                 CLICKHOUSE_PASSWORD 环境变量须与部署 .env 一致，在部署目录下执行）"
            ),
            other => eyre!("{other}"),
        })?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            eyre::bail!("ClickHouse HTTP {status}: {body}");
        }
        Ok(body)
    }

    /// 数据流导入（CSV 文件 → `INSERT INTO <table> FORMAT CSVWithNames`）。
    fn insert_csv(&self, table: &str, csv: &std::path::Path) -> Result<()> {
        database_table(table)?;
        let file = std::fs::File::open(csv)?;
        let len = file.metadata()?.len();
        let sql = urlencode(&format!(
            "INSERT INTO {table} SETTINGS input_format_defaults_for_omitted_fields=1 FORMAT CSVWithNames"
        ));
        let mut req = ureq::post(&format!("{}/?query={sql}", self.url));
        if self.user != "default" || !self.password.is_empty() {
            req = req.header("X-ClickHouse-User", &self.user);
        }
        if !self.password.is_empty() {
            req = req.header("X-ClickHouse-Key", &self.password);
        }
        let mut file = file;
        let resp = req
            .send(ureq::SendBody::from_reader(&mut file))
            .map_err(|e| eyre!("{e}"))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let mut resp = resp;
            let msg = resp.body_mut().read_to_string().unwrap_or_default();
            eyre::bail!("ClickHouse HTTP {status}: {msg}");
        }
        debug!("clickhouse 导入完成：{table}（{len} 字节 CSV）");
        // 输出数据边界：缓存解压目录可能滞后于最新交易日（实测 09-10 命中
        // 09-06 的旧缓存，包行数相近极易误判为新包）——一眼确认新鲜度
        match self.exec(&format!(
            "SELECT concat(toString(count()), ' 行，数据截至 ', toString(max(date))) FROM {table} FINAL"
        )) {
            Ok(info) => info!("day_file 数据边界: {info}"),
            Err(e) => warn!("数据已写入，边界查询失败（无需重新导入）：{e}"),
        }
        Ok(())
    }
}

/// URL 查询参数转义（导入 SQL 仅含表名/FORMAT 关键字，字符集有限）。
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
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
    let engine = cfg.exec(&format!("SELECT engine_full FROM system.tables WHERE database='{database}' AND name='{name}' FORMAT TabSeparated"))?;
    if !engine.trim().is_empty() && !engine.contains("ReplacingMergeTree(version)") {
        eyre::bail!(
            "{table}使用旧或不兼容的表引擎；请按 docs/MIGRATION.md 创建带version的新表后迁移，不会自动修改原表"
        );
    }
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
    ch_execute(&cfg, &create_table)
}

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
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&csv_path);
    result?;
    cleanup?;
    Ok(())
}

type Previous = Result<std::collections::HashMap<String, Factor>>;

pub fn previous_csv_table(
    path: &Option<std::path::PathBuf>,
    table: &str,
    keep_factor: bool,
) -> Previous {
    if let Some(Some(path)) = path.as_ref().map(|p| p.to_str()) {
        if path == "clickhouse" {
            clickhouse_factor_csv(table, keep_factor)
        } else {
            previous_csv(path, keep_factor)
        }
    } else {
        Err(anyhow!("请检查 gbbq 路径"))
    }
}

/// 读取前收盘价（前 factor ）数据。
///
/// 注意：`code` 列需为**含市场前缀的完整代码**（如 `sh600000`），
/// 与 day 命令输出的 code 格式一致，否则无法匹配。
pub fn previous_csv(p: impl AsRef<Path>, _keep_factor: bool) -> Previous {
    let path = p.as_ref();
    let rows: Vec<Factor> = csv::Reader::from_reader(File::open(path)?)
        .deserialize::<Factor>()
        .collect::<std::result::Result<_, _>>()?;
    let mut prev = std::collections::HashMap::new();
    for f in rows {
        f.code.parse::<rustdx_complete::data::Symbol>()?;
        chrono::NaiveDate::parse_from_str(&f.date, "%Y-%m-%d")?;
        if !f.factor.is_finite() || f.factor <= 0.0 || !f.preclose.is_finite() || f.preclose <= 0.0
        {
            eyre::bail!("{}前因子或收盘价非法", f.code);
        }
        if prev.insert(f.code.clone(), f).is_some() {
            eyre::bail!("前因子文件包含重复代码");
        }
    }
    Ok(prev)
}

/// 获取当前最新 factor
fn clickhouse_factor_csv(table: &str, keep_factor: bool) -> Previous {
    database_table(table)?;
    let query = format!(
        "SELECT code, t.1 AS date, t.2 AS close, t.3 AS factor FROM \
        (SELECT code, argMax(tuple(date, close, factor), date) AS t FROM {table} FINAL GROUP BY code) \
        FORMAT CSVWithNames"
    );
    let text = ClickHouseConfig::from_env().exec(&query)?;
    let rows: Vec<Factor> = csv::Reader::from_reader(text.as_bytes())
        .deserialize()
        .collect::<std::result::Result<_, _>>()?;
    if keep_factor {
        fs::write("factor.csv", text)?;
    }
    Ok(rows.into_iter().map(|f| (f.code.clone(), f)).collect())
}

pub fn run_mongodb(cmd: &DayCmd) -> Result<()> {
    let (database_name, table_name) = database_table(&cmd.table)?;
    cmd.run_csv()?;
    let args = [
        "--db",
        database_name,
        "--collection",
        table_name,
        "--type=csv",
        "--file",
        &cmd.output,
        "--headerline",
        "--mode=upsert",
        "--upsertFields=date,code",
    ];
    let output = Command::new("mongoimport").args(args).output()?;
    check_output(output)?;
    keep_csv(&cmd.output, cmd.keep_csv)?;
    Ok(())
}

fn check_output(output: std::process::Output) -> Result<()> {
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    if !output.status.success() {
        eyre::bail!("数据库导入失败: {}", output.status);
    }
    Ok(())
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
    fn output_rows_are_sorted_and_duplicate_dates_keep_last_source_row() {
        let row = |date, close| rustdx_complete::file::day::Day {
            date,
            code: "sh000001".into(),
            open: close,
            high: close,
            low: close,
            close,
            amount: 1.0,
            vol: 1,
        };
        let rows = normalize_rows(vec![
            row(20250107, 7.0),
            row(20250106, 6.0),
            row(20250107, 70.0),
        ])
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].date, rows[0].close), (20250106, 6.0));
        assert_eq!((rows[1].date, rows[1].close), (20250107, 70.0));
    }

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

    #[test]
    fn previous_inputs_are_preserved_and_invalid_rows_fail() {
        let dir = std::env::temp_dir().join(format!("rustdx-factor-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("factor.csv");
        fs::write(&path, "date,code,close,factor\n2026-09-01,sh600000,10,2\n").unwrap();
        assert_eq!(previous_csv(&path, false).unwrap()["sh600000"].factor, 2.0);
        assert!(path.exists());
        fs::write(
            &path,
            "date,code,close,factor\n2026-09-01,sh600000,invalid,2\n",
        )
        .unwrap();
        assert!(previous_csv(&path, false).is_err());
        assert!(path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
