//! 智能缓存层
//!
//! 提供高性能的数据缓存功能，避免重复请求通达信服务器
//!
//! # 设计原则
//!
//! - **零侵入性**：不修改现有 TCP 代码
//! - **可选功能**：用户按需启用缓存
//! - **TTL 自动过期**：支持灵活的过期策略
//! - **多种后端**：内存缓存、文件缓存
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use rustdx_complete::cache::{Cache, MemoryCache};
//! use rustdx_complete::tcp::stock::Kline;
//!
//! # fn fetch_from_server() -> Vec<u8> { vec![] }
//!
//! // 创建内存缓存（5分钟TTL）
//! let cache = Cache::memory(std::time::Duration::from_secs(300));
//!
//! // 尝试从缓存获取数据
//! let key = "kline:1:600000:9";
//! let cached_data = cache.get(key);
//!
//! if cached_data.is_none() {
//!     // 缓存未命中，从服务器获取
//!     let data = fetch_from_server();
//!     cache.set(key, &data);
//! }
//! ```
//!
//! # 缓存策略
//!
//! - **内存缓存**：适合开发/测试，进程重启后丢失
//! - **文件缓存**：适合生产环境，持久化存储
//! - **TTL 策略**：推荐 5-10 分钟（实时行情数据）

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 稳定文件键（FNV-1a两组种子）；文件内部另存原键校验，碰撞不会误读其他键。
pub fn stable_key(key: &str) -> String {
    let hash = |seed: u64| {
        key.bytes()
            .fold(seed, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
    };
    format!(
        "{:016x}{:016x}",
        hash(0xcbf29ce484222325),
        hash(0x84222325cbf29ce4)
    )
}

// ============================================================================
// Cache Backend Trait
// ============================================================================

/// 缓存后端 Trait
///
/// 定义缓存的基本操作，支持多种实现（内存、文件、Redis等）
pub trait CacheBackend: Send + Sync {
    /// 从缓存获取数据
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    ///
    /// # 返回
    ///
    /// - `Some(Vec<u8>)`: 缓存命中，返回数据
    /// - `None`: 缓存未命中或已过期
    fn get(&self, key: &str) -> Option<Vec<u8>>;

    /// 设置缓存数据
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    /// - `ttl`: 过期时间
    fn set(&self, key: &str, value: &[u8], ttl: Duration);

    /// 删除缓存数据
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    fn remove(&self, key: &str);

    /// 清空所有缓存
    fn clear(&self);
}

// ============================================================================
// Memory Cache Implementation
// ============================================================================

/// 内存缓存实现
///
/// 使用 HashMap 存储数据，适合开发和测试场景
///
/// # 特点
///
/// - ✅ 快速访问（O(1)）
/// - ✅ 线程安全（使用 RwLock）
/// - ❌ 进程重启后数据丢失
///
/// # 使用示例
///
/// ```rust
/// use rustdx_complete::cache::{CacheBackend, MemoryCache};
/// use std::time::Duration;
///
/// let cache = MemoryCache::new();
///
/// // 设置缓存（5分钟TTL）
/// cache.set("key1", b"data", Duration::from_secs(300));
///
/// // 获取缓存
/// if let Some(data) = cache.get("key1") {
///     println!("缓存命中: {:?}", data);
/// }
/// ```
#[derive(Debug)]
pub struct MemoryCache {
    /// 存储缓存数据的 HashMap
    /// 键: String, 值: (数据, 过期时间)
    data: std::sync::RwLock<MemoryState>,
    max_entries: usize,
    max_bytes: usize,
}

#[derive(Debug, Default)]
struct MemoryState {
    values: HashMap<String, (Arc<[u8]>, Instant)>,
    /// Earliest expiry first; ties are ordered by key.
    expiry: BTreeSet<(Instant, String)>,
    bytes: usize,
}

impl MemoryState {
    fn remove(&mut self, key: &str) {
        if let Some((value, expiry)) = self.values.remove(key) {
            self.bytes -= key.len() + value.len();
            self.expiry.remove(&(expiry, key.to_owned()));
        }
    }

    fn cleanup(&mut self, now: Instant) -> usize {
        let mut removed = 0;
        while self
            .expiry
            .first()
            .is_some_and(|(expiry, _)| *expiry <= now)
        {
            if let Some((_, key)) = self.expiry.pop_first()
                && let Some((value, _)) = self.values.remove(&key)
            {
                self.bytes -= key.len() + value.len();
                removed += 1;
            }
        }
        removed
    }
}

impl MemoryCache {
    /// 创建新的内存缓存
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::cache::MemoryCache;
    ///
    /// let cache = MemoryCache::new();
    /// ```
    pub fn new() -> Self {
        Self::with_capacity(4096, 64 * 1024 * 1024)
    }

    pub fn with_capacity(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            data: std::sync::RwLock::new(MemoryState::default()),
            max_entries,
            max_bytes,
        }
    }

    pub fn cleanup_expired(&self) -> usize {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        data.cleanup(Instant::now())
    }

    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.cleanup_expired();
        self.data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values
            .len()
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Logical live key/value bytes; caller-held Arc views are not cache-owned.
    pub fn bytes(&self) -> usize {
        self.cleanup_expired();
        self.data.read().unwrap_or_else(|e| e.into_inner()).bytes
    }

    /// A shared view avoids copying the value. Eviction does not invalidate it.
    pub fn get_shared(&self, key: &str) -> Option<Arc<[u8]>> {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        data.values
            .get(key)
            .filter(|(_, expiry)| Instant::now() < *expiry)
            .map(|(value, _)| Arc::clone(value))
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheBackend for MemoryCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.get_shared(key).map(|value| value.to_vec())
    }

    fn set(&self, key: &str, value: &[u8], ttl: Duration) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        data.cleanup(Instant::now());
        data.remove(key);
        if value.len().saturating_add(key.len()) > self.max_bytes
            || self.max_entries == 0
            || ttl.is_zero()
        {
            return;
        }
        let Some(expiry) = Instant::now().checked_add(ttl) else {
            return;
        };
        while data.values.len() >= self.max_entries
            || data.bytes > self.max_bytes - value.len() - key.len()
        {
            let oldest = data.expiry.first().map(|(_, key)| key.clone());
            if let Some(oldest) = oldest {
                data.remove(&oldest);
            } else {
                break;
            }
        }
        data.bytes += key.len() + value.len();
        data.expiry.insert((expiry, key.to_owned()));
        data.values.insert(key.to_owned(), (value.into(), expiry));
    }

    fn remove(&self, key: &str) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        data.remove(key);
    }

    fn clear(&self) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        *data = MemoryState::default();
    }
}

// ============================================================================
// File Cache Implementation
// ============================================================================

/// 文件缓存实现
///
/// 将缓存数据持久化到文件系统，适合生产环境
///
/// # 特点
///
/// - ✅ 持久化存储
/// - ✅ 进程重启后数据仍然存在
/// - ✅ 支持多进程共享
/// - ⚠️ 访问速度慢于内存缓存
///
/// # 文件结构
///
/// ```text
/// cache_dir/
///   ├── key1.cache
///   ├── key2.cache
///   └── ...
/// ```
///
/// # 使用示例
///
/// ```rust
/// use rustdx_complete::cache::{CacheBackend, FileCache};
/// use std::time::Duration;
///
/// let cache = FileCache::new("/tmp/rustdx_cache");
///
/// // 设置缓存
/// cache.set("key1", b"data", Duration::from_secs(300));
///
/// // 获取缓存
/// if let Some(data) = cache.get("key1") {
///     println!("缓存命中: {:?}", data);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct FileCache {
    /// 缓存目录
    dir: PathBuf,
    max_entries: usize,
    max_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileCacheUsage {
    pub entries: usize,
    pub bytes: u64,
}

struct CacheFile {
    path: PathBuf,
    size: u64,
    expiry: u64,
}

impl FileCache {
    /// 创建新的文件缓存
    ///
    /// # 参数
    ///
    /// - `dir`: 缓存目录路径
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::cache::FileCache;
    ///
    /// let cache = FileCache::new("/tmp/my_cache");
    /// ```
    pub fn new<P: AsRef<Path>>(dir: P) -> Self {
        let dir = dir.as_ref().to_path_buf();

        // 确保缓存目录存在
        fs::create_dir_all(&dir).ok();

        Self {
            dir,
            max_entries: 4096,
            max_bytes: 256 * 1024 * 1024,
        }
    }

    /// 获取缓存文件路径
    fn cache_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.cache", stable_key(key)))
    }

    pub fn try_new(dir: impl AsRef<Path>) -> io::Result<Self> {
        Self::try_with_capacity(dir, 4096, 256 * 1024 * 1024)
    }

    /// Limits committed cache files; atomic staging adds at most one 64MiB
    /// item. All cooperating instances/processes serialize maintenance/writes.
    /// Use identical budgets for every instance sharing this directory.
    pub fn try_with_capacity(
        dir: impl AsRef<Path>,
        max_entries: usize,
        max_bytes: u64,
    ) -> io::Result<Self> {
        if max_entries == 0 || max_entries > 100_000 || max_bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "文件缓存容量非法",
            ));
        }
        fs::create_dir_all(dir.as_ref())?;
        Ok(Self {
            dir: dir.as_ref().to_owned(),
            max_entries,
            max_bytes,
        })
    }

    fn lock(&self) -> io::Result<fs::File> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join(".rustdx-cache.lock"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            crate::operation::check()?;
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    crate::operation::sleep(Duration::from_millis(20))?;
                }
                Err(fs::TryLockError::WouldBlock) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "文件缓存锁等待超时",
                    ));
                }
                Err(fs::TryLockError::Error(error)) => return Err(error),
            }
        }
    }

    fn owned_name(path: &Path) -> bool {
        path.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
            s.len() == 38
                && s.ends_with(".cache")
                && s.as_bytes()[..32].iter().all(u8::is_ascii_hexdigit)
        })
    }

    fn staged_name(path: &Path) -> bool {
        path.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
            if s.len() < 39
                || !s
                    .get(..38)
                    .is_some_and(|name| Self::owned_name(Path::new(name)))
            {
                return false;
            }
            let parts: Vec<_> = s[38..].split('.').collect();
            parts.len() == 4
                && parts[0].is_empty()
                && parts[3] == "tmp"
                && parts[1..3]
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
    }

    fn files(&self, cleanup: bool) -> io::Result<(Vec<CacheFile>, usize)> {
        use std::io::Read;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis();
        let mut files = Vec::new();
        let mut removed = 0;
        for (index, entry) in fs::read_dir(&self.dir)?.enumerate() {
            crate::operation::check()?;
            if index >= 200_000 {
                return Err(io::Error::other("缓存目录条目过多"));
            }
            let entry = entry?;
            let path = entry.path();
            if cleanup && Self::staged_name(&path) {
                if !entry.file_type()?.is_file() {
                    return Err(io::Error::other("缓存暂存条目不是普通文件"));
                }
                fs::remove_file(path)?;
                removed += 1;
                continue;
            }
            if !Self::owned_name(&path) {
                continue;
            }
            if !entry.file_type()?.is_file() {
                return Err(io::Error::other("缓存条目不是普通文件"));
            }
            let mut header = [0; 16];
            let mut file = fs::File::open(&path)?;
            let size = file.metadata()?.len();
            let read = file.read_exact(&mut header);
            drop(file);
            let expiry = match read {
                Ok(()) if &header[..8] == b"RDXC0002" => {
                    u64::from_le_bytes(header[8..16].try_into().expect("8字节到期时间"))
                }
                Ok(()) => 0,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => 0,
                Err(e) => return Err(e),
            };
            if cleanup && now >= u128::from(expiry) {
                fs::remove_file(path)?;
                removed += 1;
            } else {
                files.push(CacheFile { path, size, expiry });
            }
        }
        Ok((files, removed))
    }

    pub fn usage(&self) -> io::Result<FileCacheUsage> {
        let _lock = self.lock()?;
        let (files, _) = self.files(false)?;
        Ok(FileCacheUsage {
            entries: files.len(),
            bytes: files.iter().try_fold(0u64, |n, file| {
                n.checked_add(file.size)
                    .ok_or_else(|| io::Error::other("缓存容量计量溢出"))
            })?,
        })
    }

    pub fn try_set(&self, key: &str, value: &[u8], ttl: Duration) -> io::Result<()> {
        if key.len() > 65536 || value.len() > 64 * 1024 * 1024 {
            return Err(io::Error::other("缓存项超出大小限制"));
        }
        let size = (20 + key.len() + value.len()) as u64;
        if size > self.max_bytes {
            return Err(io::Error::other("缓存项超过目录总字节预算"));
        }
        let _lock = self.lock()?;
        if ttl.is_zero() {
            return match fs::remove_file(self.cache_path(key)) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
        }
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .checked_add(ttl)
            .ok_or_else(|| io::Error::other("缓存TTL溢出"))?;
        let expiry = u64::try_from(expiry.as_millis()).map_err(io::Error::other)?;
        let destination = self.cache_path(key);
        let (mut files, _) = self.files(true)?;
        let mut entries = files.len();
        let mut bytes = files.iter().try_fold(0u64, |n, file| {
            n.checked_add(file.size)
                .ok_or_else(|| io::Error::other("缓存容量计量溢出"))
        })?;
        if let Some(previous) = files.iter().find(|f| f.path == destination) {
            bytes -= previous.size;
            entries -= 1;
        }
        files.sort_by(|a, b| a.expiry.cmp(&b.expiry).then_with(|| a.path.cmp(&b.path)));
        for file in files {
            if entries < self.max_entries && bytes <= self.max_bytes - size {
                break;
            }
            if file.path == destination {
                continue;
            }
            fs::remove_file(file.path)?;
            entries -= 1;
            bytes -= file.size;
        }
        if entries >= self.max_entries || bytes > self.max_bytes - size {
            return Err(io::Error::other("文件缓存容量无法满足"));
        }
        let (atomic, mut file) = crate::atomic_file::AtomicFile::create(self.cache_path(key))?;
        file.write_all(b"RDXC0002")?;
        file.write_all(&expiry.to_le_bytes())?;
        file.write_all(&(key.len() as u32).to_le_bytes())?;
        file.write_all(key.as_bytes())?;
        file.write_all(value)?;
        drop(file);
        atomic.commit()
    }

    pub fn try_get(&self, key: &str) -> io::Result<Option<Vec<u8>>> {
        let _lock = self.lock()?;
        let path = self.cache_path(key);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if file.metadata()?.len() > 64 * 1024 * 1024 + 65556 {
            return Err(io::Error::other("缓存文件超出大小限制"));
        }
        let data = {
            use std::io::Read;
            let mut data = Vec::new();
            file.take(64 * 1024 * 1024 + 65557).read_to_end(&mut data)?;
            data
        };
        if data.len() < 20 || &data[..8] != b"RDXC0002" {
            return Err(io::Error::other("缓存格式损坏或版本过旧"));
        }
        let length = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        if length > 65536 || data.get(20..20 + length) != Some(key.as_bytes()) {
            return Err(io::Error::other("缓存键不匹配"));
        }
        let expiry = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis();
        if now >= expiry as u128 {
            fs::remove_file(&path)?;
            return Ok(None);
        }
        Ok(Some(data[20 + length..].to_vec()))
    }

    /// 清理过期的缓存文件
    ///
    /// # 说明
    ///
    /// 扫描缓存目录，删除所有已过期的缓存文件
    pub fn cleanup_expired(&self) -> io::Result<usize> {
        let _lock = self.lock()?;
        self.files(true).map(|(_, removed)| removed)
    }
}

impl CacheBackend for FileCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.try_get(key).unwrap_or_else(|e| {
            log::warn!("缓存读取失败: {e}");
            None
        })
    }

    fn set(&self, key: &str, value: &[u8], ttl: Duration) {
        if let Err(e) = self.try_set(key, value, ttl) {
            log::warn!("缓存写入失败: {e}");
        }
    }

    fn remove(&self, key: &str) {
        if let Ok(_lock) = self.lock() {
            let _ = fs::remove_file(self.cache_path(key));
        }
    }

    fn clear(&self) {
        if let Ok(_lock) = self.lock()
            && let Ok(entries) = fs::read_dir(&self.dir)
        {
            for entry in entries.flatten() {
                if Self::owned_name(&entry.path()) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

// ============================================================================
// Cache Manager
// ============================================================================

/// 缓存管理器
///
/// 提供统一的缓存访问接口，支持 get_or_fetch 模式
///
/// # 泛型参数
///
/// - `B`: 缓存后端类型（MemoryCache 或 FileCache）
///
/// # 使用示例
///
/// ```rust,no_run
/// use rustdx_complete::cache::{Cache, MemoryCache};
/// use std::time::Duration;
///
/// let cache = Cache::new(
///     MemoryCache::new(),
///     Duration::from_secs(300) // 5分钟TTL
/// );
///
/// // 模式一：手动检查缓存
/// if let Some(data) = cache.get("key") {
///     println!("缓存命中");
/// } else {
///     let new_data = vec![1, 2, 3];
///     cache.set("key", &new_data);
/// }
///
/// // 模式二：get_or_fetch（推荐）
/// let data: std::result::Result<Vec<u8>, std::convert::Infallible> =
///     cache.get_or_fetch("key", || {
///         println!("缓存未命中，从服务器获取");
///         Ok(vec![1, 2, 3])
///     });
/// ```
pub struct Cache<B: CacheBackend> {
    /// 缓存后端
    backend: B,
    /// 默认TTL
    ttl: Duration,
    flights: [std::sync::Mutex<()>; 64],
}

impl<B: CacheBackend> Cache<B> {
    /// 创建新的缓存管理器
    ///
    /// # 参数
    ///
    /// - `backend`: 缓存后端
    /// - `ttl`: 默认TTL
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::cache::{Cache, MemoryCache};
    /// use std::time::Duration;
    ///
    /// let cache = Cache::new(
    ///     MemoryCache::new(),
    ///     Duration::from_secs(300)
    /// );
    /// ```
    pub fn new(backend: B, ttl: Duration) -> Self {
        Self {
            backend,
            ttl,
            flights: std::array::from_fn(|_| std::sync::Mutex::new(())),
        }
    }

    /// 获取缓存数据
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    ///
    /// # 返回
    ///
    /// - `Some(Vec<u8>)`: 缓存命中
    /// - `None`: 缓存未命中
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.backend.get(key)
    }

    /// 设置缓存数据（使用默认TTL）
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    pub fn set(&self, key: &str, value: &[u8]) {
        self.backend.set(key, value, self.ttl);
    }

    /// 设置缓存数据（自定义TTL）
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    /// - `ttl`: 过期时间
    pub fn set_with_ttl(&self, key: &str, value: &[u8], ttl: Duration) {
        self.backend.set(key, value, ttl);
    }

    /// 删除缓存数据
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    pub fn remove(&self, key: &str) {
        self.backend.remove(key);
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        self.backend.clear();
    }

    /// 获取或获取数据（推荐使用）
    ///
    /// 如果缓存命中，直接返回缓存数据；
    /// 如果缓存未命中，执行 fetch 函数获取数据，并存入缓存
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `fetch`: 数据获取函数
    ///
    /// # 返回
    ///
    /// 缓存数据或新获取的数据
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use rustdx_complete::cache::{Cache, MemoryCache};
    /// # use std::time::Duration;
    /// # let cache = Cache::new(MemoryCache::new(), Duration::from_secs(300));
    /// let data: std::result::Result<Vec<u8>, std::convert::Infallible> =
    ///     cache.get_or_fetch("my_key", || {
    ///         // 这个闭包只在缓存未命中时执行
    ///         Ok(vec![1, 2, 3, 4, 5])
    ///     });
    /// ```
    pub fn get_or_fetch<F, E>(&self, key: &str, fetch: F) -> Result<Vec<u8>, E>
    where
        F: FnOnce() -> Result<Vec<u8>, E>,
    {
        // 尝试从缓存获取
        if let Some(data) = self.get(key) {
            return Ok(data);
        }
        let stripe = key
            .bytes()
            .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize))
            % self.flights.len();
        let _flight = self.flights[stripe]
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(data) = self.get(key) {
            return Ok(data);
        }

        // 缓存未命中，执行 fetch
        let data = fetch()?;

        // 存入缓存
        self.set(key, &data);

        Ok(data)
    }

    /// 获取TTL
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// 获取后端引用
    pub fn backend(&self) -> &B {
        &self.backend
    }
}

// 便捷构造函数
impl Cache<MemoryCache> {
    /// 创建内存缓存（5分钟TTL）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let cache = Cache::memory(Duration::from_secs(300));
    /// ```
    pub fn memory(ttl: Duration) -> Self {
        Self::new(MemoryCache::new(), ttl)
    }
}

impl Cache<FileCache> {
    /// 创建文件缓存
    ///
    /// # 参数
    ///
    /// - `dir`: 缓存目录
    /// - `ttl`: 默认TTL
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rustdx_complete::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let cache = Cache::file("/tmp/rustdx_cache", Duration::from_secs(300));
    /// ```
    pub fn file<P: AsRef<Path>>(dir: P, ttl: Duration) -> Self {
        Self::new(FileCache::new(dir), ttl)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_memory_cache_basic() {
        let cache = MemoryCache::new();

        // 测试 set 和 get
        cache.set("key1", b"data1", Duration::from_secs(60));

        let result = cache.get("key1");
        assert_eq!(result, Some(b"data1".to_vec()));

        // 测试不存在的键
        let result = cache.get("key2");
        assert_eq!(result, None);
    }

    #[test]
    fn test_memory_cache_expiry() {
        let cache = MemoryCache::new();

        cache.set("key1", b"data1", Duration::from_secs(60));

        // 立即获取应该成功
        let result = cache.get("key1");
        assert_eq!(result, Some(b"data1".to_vec()));

        {
            let mut data = cache.data.write().unwrap();
            let expiry = data.values["key1"].1;
            data.expiry.remove(&(expiry, "key1".into()));
            let expired = Instant::now() - Duration::from_secs(1);
            data.values.get_mut("key1").unwrap().1 = expired;
            data.expiry.insert((expired, "key1".into()));
        }

        // 过期后应该返回 None
        let result = cache.get("key1");
        assert_eq!(result, None);
    }

    #[test]
    fn test_memory_cache_remove() {
        let cache = MemoryCache::new();

        cache.set("key1", b"data1", Duration::from_secs(60));

        // 确认数据存在
        assert!(cache.get("key1").is_some());

        // 删除数据
        cache.remove("key1");

        // 确认数据已删除
        assert!(cache.get("key1").is_none());
    }

    #[test]
    fn test_memory_cache_clear() {
        let cache = MemoryCache::new();

        cache.set("key1", b"data1", Duration::from_secs(60));
        cache.set("key2", b"data2", Duration::from_secs(60));

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_manager() {
        let cache = Cache::memory(Duration::from_secs(60));

        // 测试 set 和 get
        cache.set("key1", b"data1");

        let result = cache.get("key1");
        assert_eq!(result, Some(b"data1".to_vec()));
    }

    #[test]
    fn test_get_or_fetch() {
        let cache = Cache::memory(Duration::from_secs(60));
        let mut call_count = 0;

        // 第一次调用：缓存未命中，执行 fetch
        let result1: Result<Vec<u8>, ()> = cache.get_or_fetch("key1", || {
            call_count += 1;
            Ok(vec![1, 2, 3])
        });

        assert_eq!(result1.unwrap(), vec![1, 2, 3]);
        assert_eq!(call_count, 1);

        // 第二次调用：缓存命中，不执行 fetch
        let result2: Result<Vec<u8>, ()> = cache.get_or_fetch("key1", || {
            call_count += 1;
            Ok(vec![1, 2, 3])
        });

        assert_eq!(result2.unwrap(), vec![1, 2, 3]);
        assert_eq!(call_count, 1); // 没有增加
    }

    #[test]
    fn test_file_cache() {
        let temp_dir = std::env::temp_dir().join("rustdx_test_cache");
        let cache = FileCache::new(&temp_dir);

        // 测试 set 和 get
        cache.set("key1", b"data1", Duration::from_secs(60));

        let result = cache.get("key1");
        assert_eq!(result, Some(b"data1".to_vec()));

        // 清理
        cache.clear();
    }

    #[test]
    fn test_file_cache_expiry() {
        let temp_dir = std::env::temp_dir().join("rustdx_test_cache_expiry");
        let cache = FileCache::new(&temp_dir);

        // 使用长期TTL验证写入，再用零TTL验证过期，避免依赖线程调度。
        cache.set("key1", b"data1", Duration::from_secs(60));

        // 立即获取应该成功
        let result = cache.get("key1");
        assert_eq!(result, Some(b"data1".to_vec()));

        cache.set("key1", b"data1", Duration::ZERO);

        // 过期后应该返回 None
        let result = cache.get("key1");
        assert_eq!(result, None);

        // 清理
        cache.clear();
    }

    #[test]
    fn capacity_and_file_keys_do_not_alias() {
        let cache = MemoryCache::with_capacity(2, 20);
        for key in ["a", "b", "c"] {
            cache.set(key, b"1", Duration::from_secs(60));
        }
        assert_eq!(cache.len(), 2);
        cache.set("large", &[0; 30], Duration::from_secs(60));
        assert_eq!(cache.len(), 2);
        let dir = std::env::temp_dir().join(format!("rustdx-cache-keys-{}", std::process::id()));
        let cache = FileCache::try_new(&dir).unwrap();
        cache
            .try_set("a/b", b"one", Duration::from_secs(60))
            .unwrap();
        cache
            .try_set("a_b", b"two", Duration::from_secs(60))
            .unwrap();
        assert_eq!(cache.try_get("a/b").unwrap().unwrap(), b"one");
        assert_eq!(cache.try_get("a_b").unwrap().unwrap(), b"two");
        fs::write(dir.join("unrelated"), b"preserve").unwrap();
        cache.clear();
        assert!(dir.join("unrelated").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_fetch_is_coalesced() {
        let cache = Cache::memory(Duration::from_secs(60));
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    let result: Result<_, ()> = cache.get_or_fetch("same", || {
                        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(vec![42])
                    });
                    assert_eq!(result.unwrap(), vec![42]);
                });
            }
        });
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn incremental_bytes_and_shared_views_survive_replacement_and_eviction() {
        let cache = MemoryCache::with_capacity(2, 9);
        cache.set("a", b"123", Duration::from_secs(60));
        let view = cache.get_shared("a").unwrap();
        cache.set("a", b"1", Duration::from_secs(120));
        assert_eq!(cache.bytes(), 2);
        cache.set("b", b"123", Duration::from_secs(60));
        assert_eq!(cache.bytes(), 6);
        cache.set("c", b"12", Duration::from_secs(180));
        assert_eq!(cache.bytes(), 5);
        assert!(cache.get("b").is_none());
        assert_eq!(&*view, b"123");
        cache.remove("a");
        assert_eq!(cache.bytes(), 3);
        cache.clear();
        assert_eq!(cache.bytes(), 0);
        assert_eq!(&*view, b"123");
    }

    #[test]
    fn file_total_budget_replacement_expiry_and_stale_staging_are_governed() {
        let dir = std::env::temp_dir().join(format!("rustdx-cache-budget-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cache = FileCache::try_with_capacity(&dir, 2, 50).unwrap();
        cache.try_set("a", b"123", Duration::from_secs(60)).unwrap();
        cache
            .try_set("b", b"123", Duration::from_secs(120))
            .unwrap();
        cache
            .try_set("c", b"123", Duration::from_secs(180))
            .unwrap();
        assert!(cache.try_get("a").unwrap().is_none());
        assert_eq!(
            cache.usage().unwrap(),
            FileCacheUsage {
                entries: 2,
                bytes: 48
            }
        );
        cache.try_set("b", b"1", Duration::from_secs(240)).unwrap();
        assert_eq!(cache.usage().unwrap().bytes, 46);
        assert!(
            cache
                .try_set("b", &[1; 30], Duration::from_secs(60))
                .is_err()
        );
        assert_eq!(cache.try_get("b").unwrap().unwrap(), b"1");
        cache.try_set("b", b"1", Duration::ZERO).unwrap();
        assert_eq!(cache.usage().unwrap().entries, 1);
        let staging = dir.join(format!("{}.cache.12.34.tmp", stable_key("dead")));
        fs::write(&staging, b"interrupted").unwrap();
        fs::write(dir.join("unrelated.cache"), b"keep").unwrap();
        assert_eq!(cache.cleanup_expired().unwrap(), 1);
        assert!(!staging.exists());
        assert!(dir.join("unrelated.cache").exists());
        cache.clear();
        assert!(dir.join("unrelated.cache").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_cache_process_writer() {
        let Some(dir) = std::env::var_os("RUSTDX_CACHE_TEST_DIR") else {
            return;
        };
        let worker = std::env::var("RUSTDX_CACHE_TEST_WORKER").unwrap();
        let cache = FileCache::try_with_capacity(dir, 3, 96).unwrap();
        for index in 0..10 {
            cache
                .try_set(
                    &format!("{worker}-{index}"),
                    b"value",
                    Duration::from_secs(60),
                )
                .unwrap();
            let usage = cache.usage().unwrap();
            assert!(usage.entries <= 3 && usage.bytes <= 96);
        }
    }

    #[test]
    fn independent_processes_share_one_file_capacity_limit() {
        let dir =
            std::env::temp_dir().join(format!("rustdx-cache-processes-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let executable = std::env::current_exe().unwrap();
        let mut children = Vec::new();
        for worker in 0..3 {
            children.push(
                std::process::Command::new(&executable)
                    .args(["--exact", "cache::tests::file_cache_process_writer"])
                    .env("RUSTDX_CACHE_TEST_DIR", &dir)
                    .env("RUSTDX_CACHE_TEST_WORKER", worker.to_string())
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        for child in &mut children {
            assert!(child.wait().unwrap().success());
        }
        let cache = FileCache::try_with_capacity(&dir, 3, 96).unwrap();
        assert!(cache.usage().unwrap().entries <= 3);
        assert!(cache.usage().unwrap().bytes <= 96);
        fs::remove_dir_all(dir).unwrap();
    }
}
