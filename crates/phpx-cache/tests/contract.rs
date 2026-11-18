use phpx_cache::{CacheDriver, FilesystemCache, MemoryCache};

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
