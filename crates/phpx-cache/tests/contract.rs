use phpx_cache::{CacheDriver, FilesystemCache, MemoryCache, RedisCache};

fn redis_url() -> Option<String> {
    std::env::var("PHPX_REDIS_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn redis_cache() -> Option<RedisCache> {
    let url = redis_url()?;
    let ns = format!("test-{}-{}", std::process::id(), ns_next());
    let c = RedisCache::open(&url, &ns, 100, 1024 * 1024).expect("open redis cache");
    c.flush_namespace().expect("flush redis namespace");
    Some(c)
}

fn ns_next() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn require_redis() -> Option<RedisCache> {
    match redis_cache() {
        Some(c) => Some(c),
        None => {
            eprintln!("SKIP redis contract: set PHPX_REDIS_URL (e.g. redis://127.0.0.1:6379)");
            None
        }
    }
}

fn memory() -> MemoryCache {
    MemoryCache::new(100, 1024 * 1024)
}

fn filesystem() -> (FilesystemCache, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tmp");
    let c = FilesystemCache::open(dir.path(), 100, 1024 * 1024).expect("open fs cache");
    (c, dir)
}

fn contract_set_get<Q: CacheDriver>(q: &Q) {
    assert!(q.get("hello").expect("get").is_none());
    q.set("hello", b"world", 0).expect("set");
    assert_eq!(q.get("hello").expect("get2"), Some(b"world".to_vec()));
    let st = q.stats().expect("stats");
    assert_eq!(st.entries, 1);
    assert!(st.hits >= 1 && st.misses >= 1);
}

#[test]
fn memory_set_get() {
    contract_set_get(&memory());
}

#[test]
fn filesystem_set_get() {
    let (c, _dir) = filesystem();
    contract_set_get(&c);
}

fn contract_ttl_delete_clear<Q: CacheDriver>(q: &Q) {
    q.set("temp", b"v", 1).expect("set ttl");
    assert_eq!(q.get("temp").expect("get"), Some(b"v".to_vec()));
    assert!(q.delete("temp").expect("del"));
    assert!(q.get("temp").expect("get2").is_none());
    assert!(!q.delete("temp").expect("del2"));
    q.set("a", b"1", 0).unwrap();
    q.set("b", b"2", 0).unwrap();
    assert_eq!(q.clear().expect("clear"), 2);
    assert_eq!(q.stats().expect("stats").entries, 0);
}

#[test]
fn memory_ttl_delete_clear() {
    contract_ttl_delete_clear(&memory());
}

#[test]
fn filesystem_ttl_delete_clear() {
    let (c, _dir) = filesystem();
    contract_ttl_delete_clear(&c);
}

#[test]
fn filesystem_survives_reopen() {
    let dir = tempfile::tempdir().expect("tmp");
    {
        let c = FilesystemCache::open(dir.path(), 100, 1024 * 1024).unwrap();
        c.set("k", b"v", 0).unwrap();
    }
    let c2 = FilesystemCache::open(dir.path(), 100, 1024 * 1024).unwrap();
    assert_eq!(c2.get("k").unwrap(), Some(b"v".to_vec()));
}

#[test]
fn rejects_bad_keys() {
    let c = memory();
    assert!(c.set("", b"x", 0).is_err());
    assert!(c.set("a/b", b"x", 0).is_err());
}

#[test]
fn redis_set_get() {
    let Some(c) = require_redis() else { return };
    assert_eq!(c.stats().expect("stats").driver, "redis");
    contract_set_get(&c);
    c.flush_namespace().expect("flush");
}

#[test]
fn redis_ttl_delete_clear() {
    let Some(c) = require_redis() else { return };
    contract_ttl_delete_clear(&c);
    c.flush_namespace().expect("flush");
}

#[test]
fn redis_ttl_entry_expires() {
    let Some(c) = require_redis() else { return };
    c.set("short", b"v", 1).expect("set ttl");
    assert_eq!(c.get("short").expect("get"), Some(b"v".to_vec()));
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(
        c.get("short").expect("get2"),
        None,
        "TTL must expire the entry"
    );
    c.flush_namespace().expect("flush");
}

#[test]
fn redis_namespaces_are_isolated() {
    let url = match redis_url() {
        Some(u) => u,
        None => {
            eprintln!("SKIP redis contract: set PHPX_REDIS_URL (e.g. redis://127.0.0.1:6379)");
            return;
        }
    };
    let a = RedisCache::open(
        &url,
        &format!("iso-a-{}-{}", std::process::id(), ns_next()),
        100,
        1024 * 1024,
    )
    .expect("open a");
    let b = RedisCache::open(
        &url,
        &format!("iso-b-{}-{}", std::process::id(), ns_next()),
        100,
        1024 * 1024,
    )
    .expect("open b");
    a.flush_namespace().expect("flush a");
    b.flush_namespace().expect("flush b");
    a.set("k", b"av", 0).expect("set a");
    assert_eq!(b.get("k").expect("get b"), None, "B must not see A's key");
    assert_eq!(a.get("k").expect("get a"), Some(b"av".to_vec()));
    a.flush_namespace().expect("flush a");
    b.flush_namespace().expect("flush b");
}
