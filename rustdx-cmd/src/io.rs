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
    let hm = cmd.stocklist()?;
    let mut total_files = 0usize;
    let mut total_rows = 0usize;
    let mut failures = Vec::new();
    let mut report = CollectionReport::new(&cmd.output);
    for dir in &cmd.path {
        // 收集 (完整代码, 文件路径)，代码含市场前缀（如 `sz000001`、`sz200b07`）
        let work: Vec<(String, std::path::PathBuf)> = filter_file(dir)?
            .filter_map(|f| {
                let s = f.to_str().unwrap();
                let (b, code) = cmd.filter_ec(s);
                filter(b, f.as_path(), hm.as_ref(), dir)
                    .unwrap_or(false)
                    .then_some((code, f))
            })
            .take(
                cmd.amount
                    .unwrap_or_else(|| filter_file(dir).map(|it| it.count()).unwrap_or(0)),
            )
            .collect();
        let n = work.len();
        info!("dir: {dir:?} day 文件数量：{n}");
        let take = cmd.amount.unwrap_or(n);

        let count = AtomicUsize::new(0); // 成功解析的文件数
        let next = AtomicUsize::new(0); // 任务队列：原子取下一个文件
        let workers = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
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
                        let result = parse(code, src);
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
                let quality = crate::quality::inspect(path.clone(), code, &v);
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
                report.files.push(quality);
                if invalid {
                    continue;
                }
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
    let out_size = fs::metadata(&cmd.output)
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
#[rustfmt::skip]
fn filter_file(dir: &Path) -> Result<impl Iterator<Item = std::path:: PathBuf>> {
    Ok(dir.read_dir()?
          .filter_map(|e| e.map(|f| f.path()).ok())
          .filter(|p| p.extension().map(|s| s == "day").unwrap_or_default()))
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

/// 需要日线 clickhouse csv 文件
#[test]
fn test_insert_clickhouse() -> Result<()> {
    if std::env::var_os("RUSTDX_SKIP_INTEGRATION_TESTS").is_some() {
        return Ok(());
    }
    // 本地未安装 clickhouse-client 时跳过（该测试依赖本机 ClickHouse 服务）
    if std::process::Command::new("clickhouse-client")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("⚠️  跳过：未安装 clickhouse-client");
        return Ok(());
    }
    setup_clickhouse(true, "rustdx.tmp")?;
    insert_clickhouse(&"clickhouse", "rustdx.tmp", true)
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

/// TODO: 与数据库有关的，把库名、表名可配置
pub fn run_mongodb(cmd: &DayCmd) -> Result<()> {
    cmd.run_csv()?;
    // TODO:排查为什么 date 列无法变成 date 类型 date.date(2006-01-02)
    let (database_name, table_name) = database_table(&cmd.table)?;
    let args = [
        "--db",
        database_name,
        "--collection",
        table_name,
        "--type=csv",
        "--file",
        &cmd.output,
        "--columnsHaveTypes",
        "--fields=code.string()",
    ];
    let output = Command::new("mongoimport").args(args).output()?;
    check_output(output);
    keep_csv(&cmd.output, cmd.keep_csv)?;
    Ok(())
}

fn check_output(output: std::process::Output) {
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
    assert!(output.status.success());
}

fn keep_csv(fname: &impl AsRef<Path>, keep: bool) -> io::Result<()> {
    if keep {
        fs::rename(fname, fname.as_ref().with_extension("csv"))
    } else {
        fs::remove_file(fname)
    }
}

/// 读取本地 xls(x) 文件
pub fn read_xlsx(path: &str, col: usize, prefix: &str) -> Option<StockList> {
    use calamine::{Data, Reader, open_workbook_auto};
    let mut workbook = open_workbook_auto(path).ok()?;
    let format_ = |x: &str| format!("{}{}", crate::cmd::auto_prefix(prefix, x), x);
    // 每个单元格被解析的类型可能会不一样，所以把股票代码统一转化成字符型
    if let Some(Ok(range)) = workbook.worksheet_range_at(0) {
        Some(
            range
                .rows()
                .skip(1)
                .map(|r| match &r[col] {
                    Data::Int(x) => format_(&x.to_string()),
                    Data::Float(x) => format_(&(*x as i64).to_string()),
                    Data::String(x) => format_(x),
                    _ => unreachable!(),
                })
                .collect(),
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
