use phpx_runtime::PhpRequest;
use phpx_worker::PersistentPoolConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn fixture_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php/public")
        .to_string_lossy()
        .to_string()
}

fn req_for(id: &str, entry: &str) -> PhpRequest {
    let mut r = PhpRequest::minimal(id, &fixture_root(), entry);
    r.path = "/".to_string();
    r
}

fn adaptive_config(min_w: u32, max_w: u32) -> PersistentPoolConfig {
    PersistentPoolConfig {
        worker_count: max_w,
        queue_bound: 16,
        queue_wait_timeout: Duration::from_secs(30),
        execution_timeout: Duration::from_secs(15),
        startup_timeout: Duration::from_secs(15),
        drain_timeout: Duration::from_secs(10),
        max_requests_per_worker: 1000,
        restart_max_retries: 5,
        restart_backoff_base: Duration::from_millis(10),
        restart_backoff_max: Duration::from_millis(100),
        adaptive_enabled: true,
        adaptive_min_workers: min_w,
        adaptive_interval: Duration::from_secs(1),
        adaptive_cooldown: Duration::from_secs(2),
        adaptive_scale_up_queue_depth: 1,
        adaptive_scale_down_idle_intervals: 3,
        adaptive_memory_guard: true,
        ..Default::default()
    }
}

#[test]
fn adaptive_starts_at_floor_and_scales_with_demand() {
    // P2: min=1/max=3 starts lean; 3 concurrent 5s sleeps force two
    // scale-ups (one worker per interval, cooldown between them); when the
    // load drains, two scale-downs return the pool to the floor.
    let pool =
        Arc::new(phpx_worker::PersistentPool::new(adaptive_config(1, 3)).expect("pool must start"));
    assert!(pool.adaptive_enabled());
    assert_eq!(pool.spawned_count(), 1, "adaptive pool starts at the floor");
    let mut handles = vec![];
    for i in 0..3 {
        let p = pool.clone();
        handles.push(std::thread::spawn(move || {
            p.execute(&req_for(&format!("load-{i}"), "sleep.php"))
                .expect("sleep must serve")
        }));
    }
    let start = Instant::now();
    while pool.spawned_count() < 3 {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "pool never scaled to the ceiling (spawned={})",
            pool.spawned_count()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    for h in handles {
        let r = h.join().expect("worker thread");
        assert_eq!(r.status, 200);
    }
    let start = Instant::now();
    while pool.spawned_count() > 1 {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "pool never scaled back to the floor (spawned={})",
            pool.spawned_count()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    // Exactly two ups and two downs: one worker per interval, hard floor/ceiling.
    let text = pool.prometheus_text();
    assert!(text.contains("phpx_pool_adaptive_enabled 1"), "{text}");
    assert!(
        text.contains("phpx_pool_adaptive_scale_events_total{direction=\"up\"} 2"),
        "{text}"
    );
    assert!(
        text.contains("phpx_pool_adaptive_scale_events_total{direction=\"down\"} 2"),
        "{text}"
    );
    assert!(text.contains("phpx_pool_workers_current 1"), "{text}");
}

#[test]
fn adaptive_disabled_pool_keeps_fixed_size_under_load() {
    // P2 default (opt-in only): without adaptive_enabled the pool stays
    // exactly at worker_count no matter the pressure.
    let mut cfg = adaptive_config(2, 2);
    cfg.adaptive_enabled = false;
    let pool = Arc::new(phpx_worker::PersistentPool::new(cfg).expect("pool must start"));
    assert!(!pool.adaptive_enabled());
    assert_eq!(pool.spawned_count(), 2);
    let mut handles = vec![];
    for i in 0..4 {
        let p = pool.clone();
        handles.push(std::thread::spawn(move || {
            p.execute(&req_for(&format!("fixed-{i}"), "sleep.php"))
                .expect("sleep must serve")
        }));
    }
    for h in handles {
        let r = h.join().expect("worker thread");
        assert_eq!(r.status, 200);
    }
    assert_eq!(
        pool.spawned_count(),
        2,
        "fixed pool must not gain or lose workers"
    );
    for s in pool.worker_snapshots() {
        assert_eq!(
            s.state,
            phpx_worker::WorkerState::Ready,
            "fixed slots stay Ready, never Stopped spares: {s:?}"
        );
    }
    let text = pool.prometheus_text();
    assert!(text.contains("phpx_pool_adaptive_enabled 0"), "{text}");
    assert!(
        text.contains("phpx_pool_adaptive_scale_events_total{direction=\"up\"} 0"),
        "{text}"
    );
}
