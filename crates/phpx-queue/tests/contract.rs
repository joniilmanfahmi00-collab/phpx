use phpx_queue::{MemoryQueue, QueueDriver, RedisQueue, SqliteQueue};
use std::path::PathBuf;

fn redis_url() -> Option<String> {
    std::env::var("PHPX_REDIS_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn redis_queue() -> Option<RedisQueue> {
    let url = redis_url()?;
    let ns = format!("test-{}", uuid::Uuid::new_v4());
    let q = RedisQueue::open(&url, &ns, 100).expect("open redis queue");
    q.flush_namespace().expect("flush redis namespace");
    Some(q)
}

fn memory() -> MemoryQueue {
    MemoryQueue::new(100)
}

fn sqlite() -> (SqliteQueue, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tmp");
    let path: PathBuf = dir.path().join("queue.db");
    let q = SqliteQueue::open(&path, 100).expect("open sqlite");
    (q, dir)
}

fn contract_enqueue_reserve_ack<Q: QueueDriver>(q: &Q) {
    let job = q.enqueue("{\"hello\":\"world\"}", 3, 0).expect("enqueue");
    assert!(!job.id.is_empty());
    let st = q.status().expect("status");
    assert_eq!(st.depth, 1);
    let got = q.reserve(60).expect("reserve").expect("job ready");
    assert_eq!(got.id, job.id);
    let st2 = q.status().expect("status2");
    assert_eq!(st2.running, 1);
    assert!(q.ack(&job.id).expect("ack"));
    let st3 = q.status().expect("status3");
    assert_eq!(st3.depth, 0);
    assert_eq!(st3.dead_letter, 0);
}

#[test]
fn memory_enqueue_reserve_ack() {
    contract_enqueue_reserve_ack(&memory());
}

#[test]
fn sqlite_enqueue_reserve_ack() {
    let (q, _dir) = sqlite();
    contract_enqueue_reserve_ack(&q);
}

fn contract_retry_then_dead<Q: QueueDriver>(q: &Q) {
    let job = q.enqueue("work", 1, 0).expect("enqueue");
    let r1 = q.reserve(60).expect("r1").expect("job");
    assert_eq!(r1.id, job.id);
    let o1 = q.fail(&job.id).expect("fail1");
    match o1 {
        phpx_queue::FailOutcome::Retrying { attempts, .. } => assert_eq!(attempts, 1),
        other => panic!("expected retry, got {:?}", other),
    }
    // wait for backoff? backoff is ~2s + jitter, so reserve now should be None (delayed)
    let none = q.reserve(60).expect("reserve delayed");
    assert!(none.is_none(), "job should be delayed after fail");
    // ack unknown should be false, fail unknown should error
    assert!(!q.ack("nope").expect("ack unknown"));
}

#[test]
fn memory_retry_delayed() {
    contract_retry_then_dead(&memory());
}

#[test]
fn sqlite_retry_delayed() {
    let (q, _dir) = sqlite();
    contract_retry_then_dead(&q);
}

#[test]
fn sqlite_survives_restart() {
    let dir = tempfile::tempdir().expect("tmp");
    let path: PathBuf = dir.path().join("q.db");
    let job_id = {
        let q = SqliteQueue::open(&path, 100).expect("open1");
        let j = q.enqueue("durable", 3, 0).expect("enq");
        // simulate crash: drop without ack, running job should be recovered
        let r = q.reserve(60).expect("res").expect("job");
        assert_eq!(r.id, j.id);
        j.id.clone()
    };
    // reopen = forced termination recovery
    let q2 = SqliteQueue::open(&path, 100).expect("open2");
    let st = q2.status().expect("status");
    assert_eq!(st.depth, 1, "job must survive restart, got {:?}", st);
    let r2 = q2.reserve(60).expect("res2").expect("job after restart");
    assert_eq!(r2.id, job_id);
    assert!(q2.ack(&job_id).expect("ack"));
    let st2 = q2.status().expect("status2");
    assert_eq!(st2.depth, 0);
}

#[test]
fn dead_letter_after_max_retries_memory() {
    let q = memory();
    // max_retries=0 => first fail goes dead
    let j = q.enqueue("boom", 0, 0).expect("enq");
    let r = q.reserve(60).expect("res").expect("job");
    assert_eq!(r.id, j.id);
    let o = q.fail(&j.id).expect("fail");
    assert!(matches!(o, phpx_queue::FailOutcome::Dead { .. }));
    let st = q.status().expect("status");
    assert_eq!(st.dead_letter, 1);
    assert_eq!(st.depth, 0);
}

fn require_redis() -> Option<RedisQueue> {
    match redis_queue() {
        Some(q) => Some(q),
        None => {
            eprintln!("SKIP redis contract: set PHPX_REDIS_URL (e.g. redis://127.0.0.1:6379)");
            None
        }
    }
}

#[test]
fn redis_enqueue_reserve_ack() {
    let Some(q) = require_redis() else { return };
    assert_eq!(q.status().expect("status").driver, "redis");
    contract_enqueue_reserve_ack(&q);
    q.flush_namespace().expect("flush");
}

#[test]
fn redis_retry_delayed() {
    let Some(q) = require_redis() else { return };
    contract_retry_then_dead(&q);
    q.flush_namespace().expect("flush");
}

#[test]
fn redis_dead_letter_after_max_retries() {
    let Some(q) = require_redis() else { return };
    let j = q.enqueue("boom", 0, 0).expect("enq");
    let r = q.reserve(60).expect("res").expect("job");
    assert_eq!(r.id, j.id);
    let o = q.fail(&j.id).expect("fail");
    assert!(matches!(o, phpx_queue::FailOutcome::Dead { .. }));
    let st = q.status().expect("status");
    assert_eq!(st.dead_letter, 1);
    assert_eq!(st.depth, 0);
    q.flush_namespace().expect("flush");
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
    let a =
        RedisQueue::open(&url, &format!("iso-a-{}", uuid::Uuid::new_v4()), 100).expect("open a");
    let b =
        RedisQueue::open(&url, &format!("iso-b-{}", uuid::Uuid::new_v4()), 100).expect("open b");
    a.flush_namespace().expect("flush a");
    b.flush_namespace().expect("flush b");
    let j = a.enqueue("private", 3, 0).expect("enq a");
    assert!(
        b.reserve(60).expect("res b").is_none(),
        "B must not see A's job"
    );
    let got = a.reserve(60).expect("res a").expect("A sees its job");
    assert_eq!(got.id, j.id);
    a.flush_namespace().expect("flush a");
    b.flush_namespace().expect("flush b");
}
