use phpx_storage::{LocalStorage, StorageDriver};

fn local() -> (LocalStorage, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tmp");
    let s = LocalStorage::open(&dir.path().join("objects")).expect("open storage");
    (s, dir)
}

fn contract_put_get_list<Q: StorageDriver>(s: &Q) {
    assert!(s.get("hello.txt").expect("get").is_none());
    s.put("hello.txt", b"hi").expect("put");
    s.put("logs/a.txt", b"a").expect("put2");
    assert_eq!(s.get("hello.txt").expect("get2"), Some(b"hi".to_vec()));
    let all = s.list("").expect("list");
    assert!(all.contains(&"hello.txt".to_string()));
    assert!(all.contains(&"logs/a.txt".to_string()));
    let sub = s.list("logs/").expect("prefix");
    assert_eq!(sub, vec!["logs/a.txt".to_string()]);
    let st = s.status().expect("status");
    assert_eq!(st.objects, 2);
    assert!(s.delete("hello.txt").expect("del"));
    assert!(!s.delete("hello.txt").expect("del2"));
}

#[test]
fn local_put_get_list() {
    let (s, _dir) = local();
    contract_put_get_list(&s);
}

#[test]
fn local_survives_reopen() {
    let dir = tempfile::tempdir().expect("tmp");
    let root = dir.path().join("objects");
    {
        let s = LocalStorage::open(&root).unwrap();
        s.put("k.txt", b"v").unwrap();
    }
    let s2 = LocalStorage::open(&root).unwrap();
    assert_eq!(s2.get("k.txt").unwrap(), Some(b"v".to_vec()));
}

#[test]
fn rejects_traversal() {
    let (s, _dir) = local();
    assert!(s.put("../x", b"hi").is_err());
    assert!(s.put("/abs", b"hi").is_err());
    assert!(s.get("../x").is_err());
}
