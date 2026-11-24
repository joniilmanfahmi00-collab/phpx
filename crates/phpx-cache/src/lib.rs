//! PHPX cache (M5 phase 3 + Redis follow-up): bounded memory, filesystem and
//! Redis drivers with TTL. Redis needs a server (see `RedisCache::open`,
//! `PHPX_REDIS_URL`); without one, memory/filesystem stay the default.

use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Max key length (safe for memory + filesystem names).
pub const MAX_KEY_LEN: usize = 256;
/// Max value size per entry (1 MiB, keeps memory bounded).
pub const MAX_VALUE_BYTES: usize = 1024 * 1024;
/// Max entries per cache instance (hard bound, never unbounded).
pub const MAX_ENTRIES: usize = 100_000;
/// Max total bytes per cache instance (1 GiB cap).
pub const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub driver: String,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub max_entries: usize,
    pub max_bytes: u64,
}

/// Minimal cache contract (SPEC 7.3). TTL in seconds, 0 = no expiry.
pub trait CacheDriver: Send {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn set(&self, key: &str, value: &[u8], ttl_secs: u64) -> Result<()>;
    fn delete(&self, key: &str) -> Result<bool>;
    fn clear(&self) -> Result<usize>;
    fn stats(&self) -> Result<CacheStats>;
}

pub fn check_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(PhpxError::Config("cache key must not be empty".to_string()));
    }
    if key.len() > MAX_KEY_LEN {
        return Err(PhpxError::Config(format!(
            "cache key too long: {} bytes (max {})",
            key.len(),
            MAX_KEY_LEN
        )));
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
    {
        return Err(PhpxError::Config(format!(
            "cache key {:?} uses unsupported chars (allowed: A-Za-z0-9 . _ - :)",
            key
        )));
    }
    Ok(())
}

fn check_value(value: &[u8]) -> Result<()> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(PhpxError::Config(format!(
            "cache value too large: {} bytes (max {})",
            value.len(),
            MAX_VALUE_BYTES
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Memory driver
// ---------------------------------------------------------------------------

struct Entry {
    value: Vec<u8>,
    expires_at: Option<u64>,
}

struct MemoryInner {
    map: HashMap<String, Entry>,
    order: VecDeque<String>,
    hits: u64,
    misses: u64,
    cur_bytes: u64,
}

pub struct MemoryCache {
    driver_name: String,
    max_entries: usize,
    max_bytes: u64,
    inner: Mutex<MemoryInner>,
}

impl MemoryCache {
    pub fn new(max_entries: usize, max_bytes: u64) -> Self {
        Self {
            driver_name: "memory".to_string(),
            max_entries: max_entries.clamp(1, MAX_ENTRIES),
            max_bytes: max_bytes.clamp(1024, MAX_TOTAL_BYTES),
            inner: Mutex::new(MemoryInner {
                map: HashMap::new(),
                order: VecDeque::new(),
                hits: 0,
                misses: 0,
                cur_bytes: 0,
            }),
        }
    }

    fn locked(&self) -> Result<std::sync::MutexGuard<'_, MemoryInner>> {
        self.inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("memory cache lock poisoned: {e}")))
    }

    fn evict_if_needed(
        inner: &mut MemoryInner,
        need_bytes: u64,
        max_entries: usize,
        max_bytes: u64,
    ) {
        while (inner.map.len() >= max_entries || inner.cur_bytes + need_bytes > max_bytes)
            && !inner.order.is_empty()
        {
            if let Some(old) = inner.order.pop_front() {
                if let Some(e) = inner.map.remove(&old) {
                    inner.cur_bytes = inner.cur_bytes.saturating_sub(e.value.len() as u64);
                }
            } else {
                break;
            }
        }
    }
}

impl CacheDriver for MemoryCache {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        check_key(key)?;
        let mut inner = self.locked()?;
        let now = now_unix();
        if let Some(e) = inner.map.get(key) {
            if let Some(exp) = e.expires_at {
                if exp <= now {
                    let e = inner.map.remove(key).expect("present");
                    inner.cur_bytes = inner.cur_bytes.saturating_sub(e.value.len() as u64);
                    inner.misses += 1;
                    return Ok(None);
                }
            }
            let v = inner.map.get(key).expect("present").value.clone();
            inner.hits += 1;
            Ok(Some(v))
        } else {
            inner.misses += 1;
            Ok(None)
        }
    }

    fn set(&self, key: &str, value: &[u8], ttl_secs: u64) -> Result<()> {
        check_key(key)?;
        check_value(value)?;
        let mut inner = self.locked()?;
        let now = now_unix();
        let expires_at = if ttl_secs == 0 {
            None
        } else {
            Some(now + ttl_secs.min(86400 * 30))
        };
        if let Some(old) = inner.map.remove(key) {
            inner.cur_bytes = inner.cur_bytes.saturating_sub(old.value.len() as u64);
        }
        Self::evict_if_needed(
            &mut inner,
            value.len() as u64,
            self.max_entries,
            self.max_bytes,
        );
        if inner.map.len() >= self.max_entries {
            return Err(PhpxError::Runtime(format!(
                "cache full: {} entries >= bound {} (backpressure, never unbounded)",
                inner.map.len(),
                self.max_entries
            )));
        }
        inner.cur_bytes += value.len() as u64;
        inner.map.insert(
            key.to_string(),
            Entry {
                value: value.to_vec(),
                expires_at,
            },
        );
        inner.order.retain(|k| k != key);
        inner.order.push_back(key.to_string());
        // Opportunistic expiry sweep (cheap, bounded).
        let expired: Vec<String> = inner
            .map
            .iter()
            .filter(|(_, e)| matches!(e.expires_at, Some(exp) if exp <= now))
            .map(|(k, _)| k.clone())
            .take(32)
            .collect();
        for k in expired {
            if let Some(e) = inner.map.remove(&k) {
                inner.cur_bytes = inner.cur_bytes.saturating_sub(e.value.len() as u64);
            }
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool> {
        check_key(key)?;
        let mut inner = self.locked()?;
        if let Some(e) = inner.map.remove(key) {
            inner.cur_bytes = inner.cur_bytes.saturating_sub(e.value.len() as u64);
            inner.order.retain(|k| k != key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn clear(&self) -> Result<usize> {
        let mut inner = self.locked()?;
        let n = inner.map.len();
        inner.map.clear();
        inner.order.clear();
        inner.cur_bytes = 0;
        Ok(n)
    }

    fn stats(&self) -> Result<CacheStats> {
        let inner = self.locked()?;
        Ok(CacheStats {
            driver: self.driver_name.clone(),
            entries: inner.map.len(),
            hits: inner.hits,
            misses: inner.misses,
            max_entries: self.max_entries,
            max_bytes: self.max_bytes,
        })
    }
}

// ---------------------------------------------------------------------------
// Filesystem driver
// ---------------------------------------------------------------------------

pub struct FilesystemCache {
    driver_name: String,
    root: PathBuf,
    max_entries: usize,
    max_bytes: u64,
    counters: Mutex<(u64, u64)>,
}

impl FilesystemCache {
    pub fn open(root: &Path, max_entries: usize, max_bytes: u64) -> Result<Self> {
        if root.as_os_str().is_empty() {
            return Err(PhpxError::Config(
                "cache.driver=filesystem requires [cache] dir (e.g. ./data/cache)".to_string(),
            ));
        }
        std::fs::create_dir_all(root).map_err(|e| {
            PhpxError::Config(format!("cache dir {} not writable: {e}", root.display()))
        })?;
        Ok(Self {
            driver_name: "filesystem".to_string(),
            root: root.to_path_buf(),
            max_entries: max_entries.clamp(1, MAX_ENTRIES),
            max_bytes: max_bytes.clamp(1024, MAX_TOTAL_BYTES),
            counters: Mutex::new((0, 0)),
        })
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.cache"))
    }

    fn read_entry(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let p = self.path_for(key);
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(PhpxError::Io(format!("cache read {}: {e}", p.display()))),
        };
        if bytes.len() < 8 {
            let _ = std::fs::remove_file(&p);
            return Ok(None);
        }
        let exp = u64::from_be_bytes(bytes[0..8].try_into().expect("8 bytes"));
        if exp != 0 && exp <= now_unix() {
            let _ = std::fs::remove_file(&p);
            return Ok(None);
        }
        Ok(Some(bytes[8..].to_vec()))
    }

    fn dir_stats(&self) -> (usize, u64) {
        let mut entries = 0usize;
        let mut bytes = 0u64;
        if let Ok(rd) = std::fs::read_dir(&self.root) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "cache").unwrap_or(false) {
                    entries += 1;
                    bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
        (entries, bytes)
    }

    fn evict_if_needed(&self, need_bytes: u64) -> Result<()> {
        for _ in 0..1024 {
            let (entries, bytes) = self.dir_stats();
            if entries < self.max_entries && bytes + need_bytes <= self.max_bytes {
                return Ok(());
            }
            // Evict oldest by mtime.
            let mut oldest: Option<(PathBuf, std::time::SystemTime)> = None;
            if let Ok(rd) = std::fs::read_dir(&self.root) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().map(|x| x == "cache").unwrap_or(false) {
                        let mt = e
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(UNIX_EPOCH);
                        if oldest.as_ref().map(|(_, t)| mt < *t).unwrap_or(true) {
                            oldest = Some((p, mt));
                        }
                    }
                }
            }
            if let Some((p, _)) = oldest {
                let _ = std::fs::remove_file(p);
            } else {
                break;
            }
        }
        Err(PhpxError::Runtime(format!(
            "cache full: bound {} entries / {} bytes (backpressure, never unbounded)",
            self.max_entries, self.max_bytes
        )))
    }
}

impl CacheDriver for FilesystemCache {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        check_key(key)?;
        match self.read_entry(key)? {
            Some(v) => {
                if let Ok(mut c) = self.counters.lock() {
                    c.0 += 1;
                }
                Ok(Some(v))
            }
            None => {
                if let Ok(mut c) = self.counters.lock() {
                    c.1 += 1;
                }
                Ok(None)
            }
        }
    }

    fn set(&self, key: &str, value: &[u8], ttl_secs: u64) -> Result<()> {
        check_key(key)?;
        check_value(value)?;
        let exp = if ttl_secs == 0 {
            0
        } else {
            now_unix() + ttl_secs.min(86400 * 30)
        };
        self.evict_if_needed(value.len() as u64 + 8)?;
        let p = self.path_for(key);
        let mut buf = Vec::with_capacity(8 + value.len());
        buf.extend_from_slice(&exp.to_be_bytes());
        buf.extend_from_slice(value);
        std::fs::write(&p, buf)
            .map_err(|e| PhpxError::Io(format!("cache write {}: {e}", p.display())))?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool> {
        check_key(key)?;
        let p = self.path_for(key);
        match std::fs::remove_file(&p) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(PhpxError::Io(format!("cache delete {}: {e}", p.display()))),
        }
    }

    fn clear(&self) -> Result<usize> {
        let mut n = 0usize;
        if let Ok(rd) = std::fs::read_dir(&self.root) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "cache").unwrap_or(false)
                    && std::fs::remove_file(&p).is_ok()
                {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    fn stats(&self) -> Result<CacheStats> {
        let (entries, _) = self.dir_stats();
        let (hits, misses) = self.counters.lock().map(|c| *c).unwrap_or((0, 0));
        Ok(CacheStats {
            driver: self.driver_name.clone(),
            entries,
            hits,
            misses,
            max_entries: self.max_entries,
            max_bytes: self.max_bytes,
        })
    }
}

/// Redis driver (M5 follow-up): same envelope as the filesystem driver
/// (8-byte BE expiry + raw value), server-side index for bounded eviction.
/// Hits/misses are process-local counters like the filesystem driver.
/// `move` semantics: entries are read by value; expiry is lazy on read.
/// Keys: `{prefix}{key}` entries, `{prefix}~idx` (ZSET key by mtime micros),
/// `{prefix}~bytes` (STRING total envelope bytes). `~` never appears in
/// `check_key` keys, so data keys and bookkeeping keys cannot collide.
pub struct RedisCache {
    driver_name: String,
    conn: Mutex<redis::Connection>,
    prefix: String,
    max_entries: usize,
    max_bytes: u64,
    counters: Mutex<(u64, u64)>,
}

impl RedisCache {
    /// Open a Redis-backed cache. `namespace` isolates fleets sharing one
    /// server (tests use a uuid per case; CLI uses "default").
    pub fn open(url: &str, namespace: &str, max_entries: usize, max_bytes: u64) -> Result<Self> {
        let ns = namespace.trim();
        if ns.is_empty() {
            return Err(PhpxError::Config(
                "redis cache namespace must not be empty (isolates fleets on one server)"
                    .to_string(),
            ));
        }
        let client = redis::Client::open(url)
            .map_err(|e| PhpxError::Runtime(format!("redis open failed: {e}")))?;
        let mut conn = client
            .get_connection_with_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| {
                PhpxError::Runtime(format!(
                    "redis connect failed (is a server listening?): {e}"
                ))
            })?;
        let _: () = redis::cmd("PING")
            .query(&mut conn)
            .map_err(|e| PhpxError::Runtime(format!("redis ping failed: {e}")))?;
        Ok(Self {
            driver_name: "redis".to_string(),
            conn: Mutex::new(conn),
            prefix: format!("phpx:cache:{ns}:"),
            max_entries: max_entries.clamp(1, MAX_ENTRIES),
            max_bytes: max_bytes.clamp(1024, MAX_TOTAL_BYTES),
            counters: Mutex::new((0, 0)),
        })
    }

    fn locked(&self) -> Result<std::sync::MutexGuard<'_, redis::Connection>> {
        self.conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("redis connection lock poisoned: {e}")))
    }

    fn entry_key(&self, key: &str) -> String {
        format!("{}{key}", self.prefix)
    }
    fn index_key(&self) -> String {
        format!("{}~idx", self.prefix)
    }
    fn bytes_key(&self) -> String {
        format!("{}~bytes", self.prefix)
    }

    fn evict_if_needed(&self, need_bytes: u64) -> Result<()> {
        use redis::Commands;
        let mut conn = self.locked()?;
        for _ in 0..1024 {
            let entries: usize = conn
                .zcard(self.index_key())
                .map_err(|e| PhpxError::Runtime(format!("redis eviction count failed: {e}")))?;
            let bytes: i64 = conn
                .get(self.bytes_key())
                .map(|v: Option<i64>| v.unwrap_or(0))
                .map_err(|e| PhpxError::Runtime(format!("redis eviction count failed: {e}")))?;
            if entries < self.max_entries && (bytes.max(0) as u64) + need_bytes <= self.max_bytes {
                return Ok(());
            }
            let oldest: Vec<(String, f64)> = conn
                .zpopmin(self.index_key(), 1)
                .map_err(|e| PhpxError::Runtime(format!("redis eviction failed: {e}")))?;
            let Some((member, _)) = oldest.into_iter().next() else {
                break;
            };
            let entry = format!("{}{member}", self.prefix);
            let len: usize = conn
                .strlen(&entry)
                .map_err(|e| PhpxError::Runtime(format!("redis eviction stat failed: {e}")))?;
            let _: usize = conn
                .del(&entry)
                .map_err(|e| PhpxError::Runtime(format!("redis eviction delete failed: {e}")))?;
            let _: i64 = conn.decr(self.bytes_key(), len as i64).map_err(|e| {
                PhpxError::Runtime(format!("redis eviction accounting failed: {e}"))
            })?;
        }
        Err(PhpxError::Runtime(format!(
            "cache full: bound {} entries / {} bytes (backpressure, never unbounded)",
            self.max_entries, self.max_bytes
        )))
    }

    /// Delete every key under this namespace (test hygiene; never touches
    /// other namespaces on the shared server).
    pub fn flush_namespace(&self) -> Result<()> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let pattern = format!("{}*", self.prefix);
        let keys: Vec<String> = conn
            .scan_match(&pattern)
            .map_err(|e| PhpxError::Runtime(format!("redis scan failed: {e}")))?
            .map(|r| r.map_err(|e| PhpxError::Runtime(format!("redis scan failed: {e}"))))
            .collect::<Result<Vec<_>>>()?;
        if !keys.is_empty() {
            let _: usize = conn
                .del(keys)
                .map_err(|e| PhpxError::Runtime(format!("redis flush failed: {e}")))?;
        }
        Ok(())
    }
}

impl CacheDriver for RedisCache {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        use redis::Commands;
        check_key(key)?;
        let mut conn = self.locked()?;
        let raw: Option<Vec<u8>> = conn
            .get(self.entry_key(key))
            .map_err(|e| PhpxError::Runtime(format!("redis get failed: {e}")))?;
        match raw {
            None => {
                if let Ok(mut c) = self.counters.lock() {
                    c.1 += 1;
                }
                Ok(None)
            }
            Some(bytes) => {
                let expired_or_short = bytes.len() < 8 || {
                    let exp = u64::from_be_bytes(bytes[0..8].try_into().expect("8 bytes"));
                    exp != 0 && exp <= now_unix()
                };
                if expired_or_short {
                    let entry = self.entry_key(key);
                    let _: usize = conn.del(&entry).unwrap_or(0);
                    let _: usize = conn.zrem(self.index_key(), key).unwrap_or(0);
                    let _: i64 = conn.decr(self.bytes_key(), bytes.len() as i64).unwrap_or(0);
                    if let Ok(mut c) = self.counters.lock() {
                        c.1 += 1;
                    }
                    Ok(None)
                } else {
                    if let Ok(mut c) = self.counters.lock() {
                        c.0 += 1;
                    }
                    Ok(Some(bytes[8..].to_vec()))
                }
            }
        }
    }

    fn set(&self, key: &str, value: &[u8], ttl_secs: u64) -> Result<()> {
        use redis::Commands;
        check_key(key)?;
        check_value(value)?;
        let exp = if ttl_secs == 0 {
            0
        } else {
            now_unix() + ttl_secs.min(86400 * 30)
        };
        let entry = self.entry_key(key);
        let mut conn = self.locked()?;
        let old_len: usize = conn
            .strlen(&entry)
            .map_err(|e| PhpxError::Runtime(format!("redis set stat failed: {e}")))?;
        let mut buf = Vec::with_capacity(8 + value.len());
        buf.extend_from_slice(&exp.to_be_bytes());
        buf.extend_from_slice(value);
        drop(conn);
        self.evict_if_needed(buf.len() as u64)?;
        let mut conn = self.locked()?;
        let _: () = conn
            .set(&entry, &buf)
            .map_err(|e| PhpxError::Runtime(format!("redis set failed: {e}")))?;
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as f64)
            .unwrap_or(0.0);
        let _: usize = conn
            .zadd(self.index_key(), key, micros)
            .map_err(|e| PhpxError::Runtime(format!("redis index update failed: {e}")))?;
        let delta = buf.len() as i64 - old_len as i64;
        let _: i64 = conn
            .incr(self.bytes_key(), delta)
            .map_err(|e| PhpxError::Runtime(format!("redis accounting failed: {e}")))?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool> {
        use redis::Commands;
        check_key(key)?;
        let mut conn = self.locked()?;
        let entry = self.entry_key(key);
        let len: usize = conn
            .strlen(&entry)
            .map_err(|e| PhpxError::Runtime(format!("redis delete stat failed: {e}")))?;
        let gone: usize = conn
            .del(&entry)
            .map_err(|e| PhpxError::Runtime(format!("redis delete failed: {e}")))?;
        // Drop stale index rows even when the entry itself is gone (drift).
        let _: usize = conn.zrem(self.index_key(), key).unwrap_or(0);
        if gone == 0 {
            return Ok(false);
        }
        let _: i64 = conn.decr(self.bytes_key(), len as i64).unwrap_or(0);
        Ok(true)
    }

    fn clear(&self) -> Result<usize> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let pattern = format!("{}*", self.prefix);
        let keys: Vec<String> = conn
            .scan_match(&pattern)
            .map_err(|e| PhpxError::Runtime(format!("redis scan failed: {e}")))?
            .map(|r| r.map_err(|e| PhpxError::Runtime(format!("redis scan failed: {e}"))))
            .collect::<Result<Vec<_>>>()?;
        let idx = self.index_key();
        let bytes = self.bytes_key();
        let mut n = 0usize;
        for k in &keys {
            if k == &idx || k == &bytes {
                continue;
            }
            n += 1;
        }
        if !keys.is_empty() {
            let _: usize = conn
                .del(keys)
                .map_err(|e| PhpxError::Runtime(format!("redis clear failed: {e}")))?;
        }
        Ok(n)
    }

    fn stats(&self) -> Result<CacheStats> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let entries: usize = conn
            .zcard(self.index_key())
            .map_err(|e| PhpxError::Runtime(format!("redis stats failed: {e}")))?;
        let (hits, misses) = self.counters.lock().map(|c| *c).unwrap_or((0, 0));
        Ok(CacheStats {
            driver: self.driver_name.clone(),
            entries,
            hits,
            misses,
            max_entries: self.max_entries,
            max_bytes: self.max_bytes,
        })
    }
}

/// Open a cache driver from config values (shared by CLI + future gateway use).
pub fn open_cache(
    driver: &str,
    dir: Option<&str>,
    max_entries: usize,
    max_bytes: u64,
    redis_url: Option<&str>,
) -> Result<Box<dyn CacheDriver>> {
    if driver.eq_ignore_ascii_case("memory") {
        Ok(Box::new(MemoryCache::new(max_entries, max_bytes)))
    } else if driver.eq_ignore_ascii_case("filesystem") {
        let d = dir.unwrap_or("").trim();
        if d.is_empty() {
            return Err(PhpxError::Config(
                "cache.driver=filesystem requires [cache] dir (e.g. ./data/cache)".to_string(),
            ));
        }
        Ok(Box::new(FilesystemCache::open(
            Path::new(d),
            max_entries,
            max_bytes,
        )?))
    } else if driver.eq_ignore_ascii_case("redis") {
        let url = redis_url
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                std::env::var("PHPX_REDIS_URL")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .ok_or_else(|| {
                PhpxError::Config(
                    "cache.driver=redis requires [cache] url (e.g. redis://127.0.0.1:6379) or PHPX_REDIS_URL env"
                        .to_string(),
                )
            })?;
        Ok(Box::new(RedisCache::open(
            &url,
            "default",
            max_entries,
            max_bytes,
        )?))
    } else {
        Err(PhpxError::Config(format!(
            "cache.driver must be memory, filesystem or redis, got {driver}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_keys_and_big_values() {
        let c = MemoryCache::new(10, 1024 * 1024);
        assert!(c.set("", b"x", 0).is_err());
        assert!(c.set("a/b", b"x", 0).is_err());
        assert!(c.get("a/b").is_err());
        let big = vec![0u8; MAX_VALUE_BYTES + 1];
        assert!(c.set("k", &big, 0).is_err());
    }

    #[test]
    fn memory_evicts_oldest_when_full() {
        let c = MemoryCache::new(2, 1024 * 1024);
        c.set("a", b"1", 0).unwrap();
        c.set("b", b"2", 0).unwrap();
        c.set("c", b"3", 0).unwrap();
        assert!(c.get("a").unwrap().is_none());
        assert_eq!(c.get("b").unwrap(), Some(b"2".to_vec()));
    }
}
