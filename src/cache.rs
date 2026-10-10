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

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
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
    data: std::sync::RwLock<HashMap<String, (Vec<u8>, Instant)>>,
    max_entries: usize,
    max_bytes: usize,
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
            data: std::sync::RwLock::new(HashMap::new()),
            max_entries,
            max_bytes,
        }
    }

    pub fn cleanup_expired(&self) -> usize {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let before = data.len();
        data.retain(|_, (_, expiry)| *expiry > Instant::now());
        before - data.len()
    }

    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.cleanup_expired();
        self.data.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheBackend for MemoryCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());

        if let Some((value, expiry)) = data.get(key) {
            // 检查是否过期
            if Instant::now() < *expiry {
                // 返回克隆的数据
                return Some(value.clone());
            } else {
                // 数据已过期
                return None;
            }
        }

        None
    }

    fn set(&self, key: &str, value: &[u8], ttl: Duration) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        data.retain(|_, (_, expiry)| *expiry > Instant::now());
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
        while data.len() >= self.max_entries
            || data
                .iter()
                .map(|(k, (v, _))| k.len() + v.len())
                .sum::<usize>()
                + value.len()
                + key.len()
                > self.max_bytes
        {
            let oldest = data
                .iter()
                .min_by_key(|(_, (_, expiry))| *expiry)
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                data.remove(&oldest);
            } else {
                break;
            }
        }
        data.insert(key.to_string(), (value.to_vec(), expiry));
    }

    fn remove(&self, key: &str) {
        let mut data = self.data.write().unwrap();
        data.remove(key);
    }

    fn clear(&self) {
        let mut data = self.data.write().unwrap();
        data.clear();
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

        Self { dir }
    }

    /// 获取缓存文件路径
    fn cache_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.cache", stable_key(key)))
    }

    pub fn try_new(dir: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        Ok(Self {
            dir: dir.as_ref().to_owned(),
        })
    }

    pub fn try_set(&self, key: &str, value: &[u8], ttl: Duration) -> io::Result<()> {
        if key.len() > 65536 || value.len() > 64 * 1024 * 1024 {
            return Err(io::Error::other("缓存项超出大小限制"));
        }
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .checked_add(ttl)
            .ok_or_else(|| io::Error::other("缓存TTL溢出"))?;
        let expiry = u64::try_from(expiry.as_millis()).map_err(io::Error::other)?;
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
        let mut removed = 0;

        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();

            // 只处理 .cache 文件
            if path.extension().and_then(|s| s.to_str()) != Some("cache") {
                continue;
            }

            // 读取过期时间
            if self.read_expiry(&path).unwrap_or(true) {
                // 文件已过期，删除
                fs::remove_file(&path)?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// 从文件读取过期时间
    fn read_expiry(&self, path: &Path) -> io::Result<bool> {
        use std::io::Read;
        let mut header = [0; 16];
        fs::File::open(path)?.read_exact(&mut header)?;
        if &header[..8] != b"RDXC0002" {
            return Ok(true);
        }
        let expiry = u64::from_le_bytes(header[8..16].try_into().unwrap());
        Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis()
            >= expiry as u128)
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
        let path = self.cache_path(key);
        fs::remove_file(&path).ok();
    }

    fn clear(&self) {
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|s| s == "cache") {
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

        cache.data.write().unwrap().get_mut("key1").unwrap().1 =
            Instant::now() - Duration::from_secs(1);

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
}
