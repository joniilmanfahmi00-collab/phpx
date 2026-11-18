use phpx_runtime::PhpRequest;
use phpx_worker::{BoundedPool, PoolConfig};
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::time::Duration;

fn fixture_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php/public")
        .to_string_lossy()
        .to_string()
}

#[test]
fn bounded_pool_serves_concurrent_requests() {
    let cfg = PoolConfig {
        max_workers: 2,
        queue_bound: 8,
        queue_wait_timeout: Duration::from_secs(10),
        execution_timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let pool = Arc::new(BoundedPool::new(cfg).unwrap());
    let barrier = Arc::new(Barrier::new(5));
    let mut handles = Vec::new();
    for i in 0..4 {
        let p = pool.clone();
        let b = barrier.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            let mut req = PhpRequest::minimal(&format!("pool-{i}"), &fixture_root(), "index.php");
            req.path = "/".to_string();
            let resp = p.execute(&req).expect("pool execute must succeed");
            assert_eq!(resp.status, 200);
            let body = String::from_utf8_lossy(&resp.body).to_string();
            assert!(body.contains(&format!("pool-{i}")), "{body}");
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().unwrap();
    }
    let stats = pool.stats();
    assert_eq!(stats.completed, 4, "{stats:?}");
}

#[test]
fn overload_is_rejected_not_queued_unboundedly() {
    let cfg = PoolConfig {
        max_workers: 1,
        queue_bound: 1,
        queue_wait_timeout: Duration::from_millis(100),
        execution_timeout: Duration::from_secs(15),
        ..Default::default()
    };
    let pool = Arc::new(BoundedPool::new(cfg).unwrap());
    // Occupy the single worker with a 5s sleep.
    let p1 = pool.clone();
    let h = std::thread::spawn(move || {
        let mut req = PhpRequest::minimal("overload-holder", &fixture_root(), "sleep.php");
        req.path = "/".to_string();
        let _ = p1.execute(&req);
    });
    std::thread::sleep(Duration::from_millis(500));
    // Pool busy + queue wait tiny -> this must fail fast with backpressure,
    // proving we never grow an unbounded queue.
    let mut req = PhpRequest::minimal("overload-victim", &fixture_root(), "index.php");
    req.path = "/".to_string();
    let err = pool.execute(&req).unwrap_err();
    assert!(err.to_string().contains("overload"), "{err}");
    h.join().unwrap();
    // After drain, pool serves again.
    let mut req2 = PhpRequest::minimal("overload-after", &fixture_root(), "index.php");
    req2.path = "/".to_string();
    let resp = pool.execute(&req2).unwrap();
    assert_eq!(resp.status, 200);
}
