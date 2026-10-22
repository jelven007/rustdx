//! 通达信日线完整包（hsjday.zip）的自动下载、解压与解析目录发现。
//!
//! 供 `rustdx day`（无目录参数）使用：
//! 1. 从默认地址 <https://data.tdx.com.cn/vipdoc/hsjday.zip> 下载完整包；
//! 2. 下载失败时自动重试，仍失败则交互询问用户提供备选下载地址（URL）或
//!    本地 zip 文件路径；
//! 3. 断点续传：中断后重跑从已下载位置继续（服务器支持 `Accept-Ranges`）；
//! 4. 缓存跳过：按 ETag/Last-Modified/大小比对服务器文件，未变化则跳过
//!    下载与解压，二次运行接近秒开；
//! 5. 解压后收集 `sh/lday`、`sz/lday`、`bj/lday` 等目录，返回给解析流程。

use std::io::{IsTerminal, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use eyre::{Result, bail, eyre};
use rustdx_cli::http::{self, HttpOperation};
use rustdx_complete::operation::{self, CheckedIo};
use serde::{Deserialize, Serialize};

/// 通达信官网日线完整包默认地址。
pub const DEFAULT_HSJDAY_URL: &str = "https://data.tdx.com.cn/vipdoc/hsjday.zip";

/// 下载/解压缓存目录下保存的元信息（与服务器文件比对，决定是否跳过下载/解压）。
#[derive(Serialize, Deserialize, Default)]
struct Meta {
    url: String,
    etag: Option<String>,
    last_modified: Option<String>,
    size: u64,
    downloaded_ok: bool,
    extracted_ok: bool,
    downloaded_at: String,
}

/// HEAD 探测到的服务器文件信息。
struct HeadInfo {
    etag: Option<String>,
    last_modified: Option<String>,
    size: Option<u64>,
}

struct CacheLock(std::fs::File);

/// 缓存锁覆盖整个来源消费周期，防止其他进程在解析时替换解压目录。
pub struct PreparedSource {
    pub dirs: Vec<PathBuf>,
    _lock: CacheLock,
}

impl CacheLock {
    fn acquire(dir: &Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("cache.lock"))?;
        file.try_lock()
            .map_err(|e| eyre!("该下载缓存已被其他进程使用: {e}"))?;
        Ok(Self(file))
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// 自动从默认地址下载并解压，返回解压出的 `lday` 目录列表。
pub fn prepare_hsjday_dirs() -> Result<PreparedSource> {
    prepare_hsjday_dirs_from(DEFAULT_HSJDAY_URL)
}

/// 从指定 URL 下载并解压（带缓存跳过 / 断点续传 / 自动重试 / 失败交互）。
pub fn prepare_hsjday_dirs_from(url: &str) -> Result<PreparedSource> {
    operation::check()?;
    let workdir = cache_dir_for(url);
    std::fs::create_dir_all(&workdir)?;
    let lock = CacheLock::acquire(&workdir)?;
    let zip_path = workdir.join("hsjday.zip");
    let extract_dir = workdir.join("extracted");
    let meta_path = workdir.join("meta.json");
    let mut meta = read_meta(&meta_path)?;

    // 1. 下载（服务器文件未变化 → 跳过下载）
    let head = head_info(url).ok(); // 探测失败不阻塞，见下方判断
    operation::check()?;
    if zip_cached(url, &zip_path, &meta, head.as_ref()) {
        log::info!(
            "缓存命中：服务器文件未变化，跳过下载 {}",
            zip_path.display()
        );
    } else {
        let (actual_source, actual_head) = download_with_fallback(url, &zip_path)?;
        let zip_len = std::fs::metadata(&zip_path)?.len();
        meta = Meta {
            url: actual_source,
            etag: actual_head.etag,
            last_modified: actual_head.last_modified,
            size: zip_len,
            downloaded_ok: true,
            extracted_ok: false,
            downloaded_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        write_meta(&meta_path, &meta)?;
    }

    // 2. 解压（已解压且目录完整 → 跳过解压）
    if meta.extracted_ok && extract_dir.is_dir() && !find_lday_dirs(&extract_dir).is_empty() {
        log::info!("解压缓存命中：{}", extract_dir.display());
    } else {
        extract_and_discover(&zip_path, &extract_dir, url)?;
        meta.extracted_ok = true;
        write_meta(&meta_path, &meta)?;
    }

    // 3. 收集 lday 目录（sh/lday、sz/lday、bj/lday ...）
    let dirs = find_lday_dirs(&extract_dir);
    if dirs.is_empty() {
        bail!(
            "解压后未找到含 *.day 文件的 lday 目录，来源: {url}（解压根: {}）",
            extract_dir.display()
        );
    }
    for d in &dirs {
        log::info!("发现解析目录: {}", d.display());
    }
    Ok(PreparedSource { dirs, _lock: lock })
}

/// 直接解压本地 zip 文件，返回解压出的 `lday` 目录列表。
pub fn prepare_hsjday_dirs_from_zip(zip_path: &Path) -> Result<PreparedSource> {
    let canonical = zip_path.canonicalize()?;
    let metadata = canonical.metadata()?;
    let identity = format!(
        "local:{}:{}:{:?}",
        canonical.display(),
        metadata.len(),
        metadata.modified()?
    );
    let workdir = cache_dir_for(&identity);
    std::fs::create_dir_all(&workdir)?;
    let lock = CacheLock::acquire(&workdir)?;
    let meta_path = workdir.join("meta.json");
    let extract_dir = workdir.join("extracted");
    let meta = read_meta(&meta_path)?;
    if meta.url == identity && meta.extracted_ok && !find_lday_dirs(&extract_dir).is_empty() {
        return Ok(PreparedSource {
            dirs: find_lday_dirs(&extract_dir),
            _lock: lock,
        });
    }
    let dirs = extract_and_discover(zip_path, &extract_dir, &zip_path.display().to_string())?;
    write_meta(
        &meta_path,
        &Meta {
            url: identity,
            size: metadata.len(),
            extracted_ok: true,
            ..Default::default()
        },
    )?;
    Ok(PreparedSource { dirs, _lock: lock })
}

/// 下载（首次失败自动重试 2 次；仍失败 → 交互提供备选 URL / 本地 zip 路径）。
fn download_with_fallback(initial_url: &str, zip_path: &Path) -> Result<(String, HeadInfo)> {
    let mut last_err: Option<eyre::Report> = None;
    for attempt in 1..=3 {
        match download(initial_url, zip_path) {
            Ok(info) => return Ok((initial_url.to_owned(), info)),
            Err(e) => {
                operation::check()?;
                log::warn!("下载失败（第 {attempt}/3 次）: {e:#}");
                last_err = Some(e);
                if attempt < 3 {
                    operation::sleep(Duration::from_secs(attempt as u64))?;
                }
            }
        }
    }
    log::warn!(
        "连续 3 次下载失败，进入交互模式：{}",
        last_err
            .as_ref()
            .map(|e| format!("{e:#}"))
            .unwrap_or_default()
    );

    let fallback = prompt_fallback()?;
    if fallback.starts_with("http://") || fallback.starts_with("https://") {
        let info = download(&fallback, zip_path).map_err(|e2| eyre!("备选地址下载失败: {e2:#}"))?;
        Ok((fallback, info))
    } else {
        let p = PathBuf::from(&fallback);
        if !p.is_file() {
            bail!("提供的文件不存在: {}", p.display());
        }
        log::info!("使用本地 zip: {}", p.display());
        // 先校验本地 ZIP 目录，使用同一文件句柄复制，失败保留旧缓存。
        let mut source = zip::ZipArchive::new(std::fs::File::open(&p)?)?.into_inner();
        source.rewind()?;
        let (atomic, mut file) = rustdx_complete::atomic_file::AtomicFile::create(zip_path)?;
        std::io::copy(&mut CheckedIo(&mut source), &mut CheckedIo(&mut file))?;
        drop(file);
        atomic.commit()?;
        Ok((
            fallback,
            HeadInfo {
                etag: None,
                last_modified: None,
                size: Some(std::fs::metadata(zip_path)?.len()),
            },
        ))
    }
}

#[derive(Serialize, Deserialize)]
struct PartialMeta {
    url: String,
    validator: Option<String>,
}

fn response_header(headers: &ureq::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// 严格验证续传字节边界，返回完整文件的期望大小。
fn validate_content_range(value: &str, offset: u64, length: Option<u64>) -> Result<u64> {
    let value = value
        .strip_prefix("bytes ")
        .ok_or_else(|| eyre!("非法Content-Range"))?;
    let (range, total) = value
        .split_once('/')
        .ok_or_else(|| eyre!("非法Content-Range"))?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| eyre!("非法Content-Range"))?;
    let (start, end, total): (u64, u64, u64) = (start.parse()?, end.parse()?, total.parse()?);
    if start != offset
        || end < start
        || end >= total
        || length.is_some_and(|len| len != end - start + 1)
    {
        bail!("Content-Range与续传位置/响应长度不匹配");
    }
    Ok(total)
}

/// 下载 url 到 dest 文件，支持断点续传（服务器返回 206 时从已下载位置追加）。
fn download(url: &str, dest: &Path) -> Result<HeadInfo> {
    let mut operation = HttpOperation::new(Duration::from_secs(2 * 60 * 60))?;
    let part = dest.with_extension("zip.part");
    let part_meta = dest.with_extension("zip.part.json");
    let previous: Option<PartialMeta> = std::fs::read(&part_meta)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let validator = previous
        .as_ref()
        .filter(|p| p.url == url)
        .and_then(|p| p.validator.as_ref());
    let done = if validator.is_some() {
        std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    if done > 0 {
        log::info!(
            "正在下载 {url} ...（断点续传，已下载 {:.1} MiB）",
            done as f64 / (1024.0 * 1024.0)
        );
    } else {
        log::info!("正在下载 {url} ...");
    }

    let mut req = http::get(url)
        .config()
        .timeout_global(Some(operation.timeout()))
        .build()
        .header("Accept-Encoding", "identity");
    if done > 0 {
        req = req
            .header("Range", &format!("bytes={done}-"))
            .header("If-Range", validator.unwrap());
    }
    let mut resp = match req.call() {
        Ok(resp) => resp,
        Err(ureq::Error::StatusCode(416)) if done > 0 => {
            let _ = std::fs::remove_file(&part);
            let _ = std::fs::remove_file(&part_meta);
            drop(operation);
            return download(url, dest);
        }
        Err(e) => return Err(eyre!("{e}")),
    };

    let mut start = 0u64;
    let length = response_header(resp.headers(), "content-length").and_then(|s| s.parse().ok());
    let mut expected = length;
    match resp.status().as_u16() {
        200 if done > 0 => {
            // 服务器不支持 Range（或文件已变）：从头下载
            log::warn!("服务器返回 200（不支持断点续传），从头下载");
        }
        206 if done > 0 => {
            let range = response_header(resp.headers(), "content-range")
                .ok_or_else(|| eyre!("续传响应缺少Content-Range"))?;
            expected = Some(match validate_content_range(&range, done, length) {
                Ok(size) => size,
                Err(e) => {
                    let _ = std::fs::remove_file(&part_meta);
                    return Err(e);
                }
            });
            start = done;
        }
        200 => {}
        s => {
            bail!("HTTP {s}");
        }
    }

    let etag = response_header(resp.headers(), "etag").filter(|s| !s.starts_with("W/"));
    let last_modified = response_header(resp.headers(), "last-modified");
    let response_validator = etag
        .clone()
        .or_else(|| response_header(resp.headers(), "last-modified"));
    if start > 0
        && response_validator
            .as_ref()
            .is_some_and(|value| Some(value) != validator)
    {
        let _ = std::fs::remove_file(&part_meta);
        bail!("续传响应版本已变化");
    }
    let metadata = PartialMeta {
        url: url.to_owned(),
        validator: response_validator.or_else(|| validator.cloned()),
    };
    let (atomic, mut metadata_file) = rustdx_complete::atomic_file::AtomicFile::create(&part_meta)?;
    serde_json::to_writer(&mut metadata_file, &metadata)?;
    drop(metadata_file);
    atomic.commit()?;
    let mut out = if start > 0 {
        std::fs::OpenOptions::new().append(true).open(&part)?
    } else {
        std::fs::File::create(&part)?
    };

    let mut reader = resp.body_mut().as_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut total = start;
    let mut last_mark = start / (32 * 1024 * 1024);
    loop {
        operation::check()?;
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        total += n as u64;
        let mark = total / (32 * 1024 * 1024);
        if mark > last_mark {
            last_mark = mark;
            log::info!("已下载 {:.0} MiB", mark as f64 * 32.0);
        }
    }
    out.flush()?;
    out.sync_all()?;
    drop(out);
    if expected.is_some_and(|size| total != size) {
        bail!("下载长度不完整: {total}/{expected:?}");
    }
    zip::ZipArchive::new(std::fs::File::open(&part)?).map_err(|e| eyre!("下载ZIP目录损坏: {e}"))?;
    operation::check()?;
    std::fs::rename(&part, dest)?;
    operation.complete(false)?;
    let _ = std::fs::remove_file(&part_meta);
    log::info!(
        "下载完成: {} ({:.1} MiB)",
        dest.display(),
        total as f64 / (1024.0 * 1024.0)
    );
    Ok(HeadInfo {
        etag,
        last_modified,
        size: Some(total),
    })
}

/// 下载失败后的交互：询问用户提供备选 URL 或本地 zip 文件路径。
fn prompt_fallback() -> Result<String> {
    operation::check()?;
    // A terminal read cannot be safely interrupted with portable synchronous I/O.
    // Task-governed commands require an explicit fallback on the next invocation.
    if rustdx_complete::operation::OperationContext::current().is_some() {
        bail!("下载重试已耗尽；请重新运行并显式提供备选URL或本地ZIP");
    }
    if !std::io::stdin().is_terminal() {
        bail!("下载重试已耗尽；非交互环境请显式提供可用URL或本地ZIP");
    }
    eprintln!("下载失败。请提供备选下载地址（URL）或本地 zip 文件路径（直接回车退出）：");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let s = line.trim().to_string();
    if s.is_empty() {
        bail!("未提供备选来源，已退出。可重跑：`rustdx day <下载地址或本地zip路径>`");
    }
    Ok(s)
}

/// 解压 zip 到 dest。
///
/// 通达信 zip 的条目名使用 Windows 反斜杠分隔（如 `sh\lday\sh000001.day`），
/// 这里统一归一化为 `/`，并做路径穿越（zip-slip）安全检查。
fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    log::info!("解压 {} 个文件 ...", archive.len());

    for i in 0..archive.len() {
        operation::check()?;
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let normalized = entry.name().replace('\\', "/");
        let out_path = safe_join(dest, &normalized)
            .ok_or_else(|| eyre!("zip 内包含非法路径条目: {}", entry.name()))?;
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path)?);
        std::io::copy(&mut CheckedIo(&mut entry), &mut CheckedIo(&mut out))?;
        out.flush()?;
    }
    Ok(())
}

/// 解压 zip 并收集 `lday` 目录。
fn extract_and_discover(zip_path: &Path, extract_dir: &Path, source: &str) -> Result<Vec<PathBuf>> {
    let staging = extract_dir.with_extension("staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    if let Err(e) = extract_zip(zip_path, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    if find_lday_dirs(&staging).is_empty() {
        bail!("ZIP中未找到含day文件的lday目录: {source}");
    }
    if extract_dir.exists() {
        operation::check()?;
        std::fs::remove_dir_all(extract_dir)?;
    }
    std::fs::rename(&staging, extract_dir)?;
    log::info!("解压完成: {}", extract_dir.display());

    let dirs = find_lday_dirs(extract_dir);
    if dirs.is_empty() {
        bail!(
            "解压后未找到含 *.day 文件的 lday 目录，来源: {source}（解压根: {}）",
            extract_dir.display()
        );
    }
    for d in &dirs {
        log::info!("发现解析目录: {}", d.display());
    }
    Ok(dirs)
}

/// 安全拼接：只允许普通目录/文件名组件，拒绝 `..`、绝对路径、前缀等。
fn safe_join(base: &Path, name: &str) -> Option<PathBuf> {
    if name.contains(':') || name.is_empty() {
        return None;
    }
    let mut out = PathBuf::from(base);
    for comp in Path::new(name).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            _ => return None,
        }
    }
    Some(out)
}

/// 在解压根目录下收集所有名为 `lday` 的子目录（sh/lday、sz/lday、bj/lday ...）。
fn find_lday_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return dirs;
    };
    for e in entries.flatten() {
        let lday = e.path().join("lday");
        if lday.is_dir()
            && std::fs::read_dir(&lday).is_ok_and(|mut files| {
                files.any(|f| f.is_ok_and(|f| f.path().extension().is_some_and(|s| s == "day")))
            })
        {
            dirs.push(lday);
        }
    }
    dirs.sort();
    dirs
}

/// 按 URL 独立缓存目录（不同来源互不覆盖）。
fn cache_dir_for(url: &str) -> PathBuf {
    std::env::temp_dir()
        .join("rustdx_hsjday")
        .join(rustdx_complete::cache::stable_key(url))
}

/// HEAD 探测服务器文件信息（ETag / Last-Modified / Content-Length）。
fn head_info(url: &str) -> Result<HeadInfo> {
    let mut operation = HttpOperation::new(Duration::from_secs(30))?;
    let resp = http::head(url)
        .config()
        .timeout_global(Some(operation.timeout()))
        .build()
        .call()
        .map_err(|e| eyre!("{e}"))?;
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };
    let info = HeadInfo {
        etag: header("etag"),
        last_modified: header("last-modified"),
        size: header("content-length").and_then(|s| s.parse::<u64>().ok()),
    };
    operation.complete(false)?;
    Ok(info)
}

/// 判断本地 zip 缓存是否仍与服务器一致（命中则跳过下载）。
fn zip_cached(url: &str, zip_path: &Path, meta: &Meta, head: Option<&HeadInfo>) -> bool {
    if std::fs::metadata(zip_path).is_err()
        || std::fs::metadata(zip_path).is_ok_and(|m| m.len() != meta.size)
        || !meta.downloaded_ok
        || meta.url != url
    {
        return false;
    }
    let Some(head) = head else {
        log::warn!("无法核实远端ZIP新鲜度，本次不能使用下载缓存；可显式指定本地ZIP");
        return false;
    };
    let size_ok = head.size.map(|s| meta.size == s).unwrap_or(true);
    let id_ok = match (&meta.etag, &head.etag) {
        (Some(a), Some(b)) => a == b,
        _ => match (&meta.last_modified, &head.last_modified) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        },
    };
    size_ok && id_ok
}

fn read_meta(path: &Path) -> Result<Meta> {
    if !path.exists() {
        return Ok(Meta::default());
    }
    let s = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s).unwrap_or_default())
}

fn write_meta(path: &Path, meta: &Meta) -> Result<()> {
    let (atomic, mut file) = rustdx_complete::atomic_file::AtomicFile::create(path)?;
    serde_json::to_writer_pretty(&mut file, meta)?;
    drop(file);
    atomic.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_resume_uses_validator_and_publishes_complete_archive() {
        use std::net::TcpListener;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(
            "sh/lday/sh600000.day",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&[0; 32]).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let split = bytes.len() / 2;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hsjday.zip", listener.local_addr().unwrap());
        let dir = std::env::temp_dir().join(format!("rustdx-resume-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("hsjday.zip");
        std::fs::write(dest.with_extension("zip.part"), &bytes[..split]).unwrap();
        std::fs::write(
            dest.with_extension("zip.part.json"),
            serde_json::to_vec(&PartialMeta {
                url: url.clone(),
                validator: Some("\"version1\"".into()),
            })
            .unwrap(),
        )
        .unwrap();
        let response_bytes = bytes.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_lowercase();
            assert!(request.contains(&format!("range: bytes={split}-")));
            assert!(request.contains("if-range: \"version1\""));
            let header = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nETag: \"version1\"\r\nConnection: close\r\n\r\n",
                response_bytes.len() - split,
                split,
                response_bytes.len() - 1,
                response_bytes.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&response_bytes[split..]).unwrap();
        });
        download(&url, &dest).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
        assert!(!dest.with_extension("zip.part").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("/tmp/base");
        assert!(safe_join(base, "sh/lday/sh000001.day").is_some());
        assert!(safe_join(base, "../evil").is_none());
        assert!(safe_join(base, "/abs").is_none());
        assert!(safe_join(base, "a/../../b").is_none());
        assert!(safe_join(base, "C:/evil").is_none());
    }

    #[test]
    fn content_range_requires_exact_offset_and_length() {
        assert_eq!(
            validate_content_range("bytes 10-19/20", 10, Some(10)).unwrap(),
            20
        );
        for range in [
            "bytes 0-19/20",
            "bytes 10-20/20",
            "bytes 10-19/*",
            "10-19/20",
        ] {
            assert!(validate_content_range(range, 10, Some(10)).is_err());
        }
    }

    #[test]
    fn local_archives_have_isolated_cache_and_failure_preserves_extraction() {
        let dir = std::env::temp_dir().join(format!("rustdx-zip-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file(
            "sh\\lday\\sh600000.day",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&[0; 32]).unwrap();
        zip.finish().unwrap();
        let source = prepare_hsjday_dirs_from_zip(&path).unwrap();
        assert!(prepare_hsjday_dirs_from_zip(&path).is_err());
        assert!(source.dirs[0].join("sh600000.day").exists());
        drop(source);
        drop(prepare_hsjday_dirs_from_zip(&path).unwrap());
        let extracted = dir.join("extracted");
        let dirs = extract_and_discover(&path, &extracted, "fixture").unwrap();
        assert_eq!(dirs.len(), 1);
        std::fs::write(&path, b"invalid zip").unwrap();
        assert!(extract_and_discover(&path, &extracted, "fixture").is_err());
        assert!(dirs[0].join("sh600000.day").exists());
        let lock = CacheLock::acquire(&dir).unwrap();
        assert!(CacheLock::acquire(&dir).is_err());
        assert_ne!(cache_dir_for("local:a"), cache_dir_for("local:b"));
        drop(lock);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn find_lday_dirs_empty_when_missing() {
        assert!(find_lday_dirs(Path::new("/nonexistent_dir_xyz")).is_empty());
    }

    #[test]
    fn zip_cached_logic() {
        let zip = Path::new("/tmp/rustdx_hsjday_test/hsjday.zip");
        // 无 zip → 不命中
        let m = Meta {
            url: "https://x/hsjday.zip".into(),
            downloaded_ok: true,
            size: 100,
            ..Default::default()
        };
        assert!(!zip_cached("https://x/hsjday.zip", zip, &m, None));
        // 同 etag → 命中
        std::fs::create_dir_all("/tmp/rustdx_hsjday_test").unwrap();
        std::fs::write(zip, b"x").unwrap();
        let m2 = Meta {
            url: "https://x/hsjday.zip".into(),
            etag: Some("\"abc\"".into()),
            downloaded_ok: true,
            size: 1,
            ..Default::default()
        };
        let h = HeadInfo {
            etag: Some("\"abc\"".into()),
            last_modified: None,
            size: Some(1),
        };
        assert!(zip_cached("https://x/hsjday.zip", zip, &m2, Some(&h)));
        // etag 变了 → 不命中
        let h2 = HeadInfo {
            etag: Some("\"def\"".into()),
            last_modified: None,
            size: Some(1),
        };
        assert!(!zip_cached("https://x/hsjday.zip", zip, &m2, Some(&h2)));
        // 不同 url → 不命中
        assert!(!zip_cached("https://y/hsjday.zip", zip, &m2, Some(&h)));
        let _ = std::fs::remove_dir_all("/tmp/rustdx_hsjday_test");
    }
}
