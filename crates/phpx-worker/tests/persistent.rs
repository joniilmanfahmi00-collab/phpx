use phpx_runtime::PhpRequest;
use phpx_worker::{PersistentPool, PersistentPoolConfig, WorkerState};
use std::path::PathBuf;
use std::time::Duration;

fn fixture_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php/public")
        .to_string_lossy()
        .to_string()
}

fn composer_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/composer-autoload/public")
        .to_string_lossy()
        .to_string()
}

fn req_for(id: &str, root: &str, entry: &str) -> PhpRequest {
    let mut r = PhpRequest::minimal(id, root, entry);
    r.path = "/".to_string();
    r
}

fn pool_config(workers: u32) -> PersistentPoolConfig {
    PersistentPoolConfig {
        worker_count: workers,
        queue_bound: 16,
        queue_wait_timeout: Duration::from_secs(10),
        execution_timeout: Duration::from_secs(10),
        startup_timeout: Duration::from_secs(15),
        drain_timeout: Duration::from_secs(5),
        max_requests_per_worker: 1000,
        restart_max_retries: 5,
        restart_backoff_base: Duration::from_millis(10),
        restart_backoff_max: Duration::from_millis(100),
        ..Default::default()
    }
}

fn guardian_config(workers: u32, soft_mb: u64, hard_mb: u64) -> PersistentPoolConfig {
    let mut cfg = pool_config(workers);
    cfg.memory_soft_bytes = soft_mb * 1024 * 1024;
    cfg.memory_hard_bytes = hard_mb * 1024 * 1024;
    cfg.memory_sample_interval = Duration::from_secs(1);
    cfg
}

fn mem_req(id: &str, mb: u64) -> PhpRequest {
    let mut r = req_for(id, &fixture_root(), "mem.php");
    r.path = "/mem.php".to_string();
    r.query = format!("mb={mb}");
    r
}

/// Measure one fresh worker's base RSS so thresholds can be set relative to
/// it (base varies by platform/PHP build; absolute thresholds would be flaky).
fn measure_base_rss_bytes() -> u64 {
    // Huge soft limit: sampling runs (guardian enabled) but never fires.
    let pool = PersistentPool::new(guardian_config(1, 4096, 0)).expect("probe pool");
    let resp = pool
        .execute(&req_for("probe", &fixture_root(), "index.php"))
        .expect("probe request");
    assert_eq!(resp.status, 200);
    let rss = pool.worker_snapshots()[0].rss_bytes;
    assert!(rss > 0, "probe must sample base RSS");
    rss
}

const MB: u64 = 1024 * 1024;

// Guardian recycle latency budget: the hog request itself may take up to the
// execution timeout (10s) and the replacement spawn up to the startup timeout
// (15s); 5s slack for loaded CI. Enforcement is synchronous at request
// completion, so this is normally far below the cap.
const GUARDIAN_RECYCLE_MAX_MS: u64 = 30_000;

#[test]
fn soft_limit_recycles_gracefully_with_controlled_workload() {
    // P0-C acceptance: soft-threshold recycling with a controlled memory
    // workload. Threshold sits above base RSS; the 64MB hog crosses it.
    let base = measure_base_rss_bytes();
    let mut cfg = guardian_config(1, 0, 0);
    cfg.memory_soft_bytes = base + 16 * MB;
    let pool = PersistentPool::new(cfg).expect("pool must start");
    let t0 = std::time::Instant::now();
    let resp = pool.execute(&mem_req("soft-1", 64)).expect("execute");
    assert_eq!(
        resp.status,
        200,
        "body: {}",
        String::from_utf8_lossy(&resp.body)
    );
    let body = String::from_utf8_lossy(&resp.body).to_string();
    assert!(body.contains("held=67108864"), "{body}");
    // Enforcement is synchronous at request completion: the worker that
    // served is already replaced (new incarnation, fresh RSS baseline).
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    assert_eq!(snaps[0].soft_hits, 1, "{snaps:?}");
    assert_eq!(snaps[0].hard_hits, 0, "{snaps:?}");
    assert_eq!(snaps[0].state, WorkerState::Ready, "{snaps:?}");
    assert!(
        snaps[0]
            .restarts_by_reason
            .iter()
            .any(|(c, n)| c == "soft-limit" && *n == 1),
        "{snaps:?}"
    );
    // Replacement serves: fresh $GLOBALS, so held=0 for a plain hit.
    let plain = pool
        .execute(&req_for("soft-after", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(plain.status, 200);
    let elapsed_ms = t0.elapsed().as_millis() as u64;
    assert!(
        elapsed_ms <= GUARDIAN_RECYCLE_MAX_MS,
        "hog->recycle->serve took {elapsed_ms}ms, exceeds {GUARDIAN_RECYCLE_MAX_MS}ms"
    );
    // Metrics expose the trigger with its threshold label.
    let text = pool.prometheus_text();
    assert!(
        text.contains("phpx_worker_memory_limit_hits_total{worker_id=\"0\",threshold=\"soft\"} 1"),
        "{text}"
    );
    assert!(
        text.contains("phpx_worker_rss_bytes{worker_id=\"0\"}"),
        "{text}"
    );
    assert!(
        text.contains("phpx_worker_peak_rss_bytes{worker_id=\"0\"}"),
        "{text}"
    );
    assert!(
        text.contains("phpx_worker_startup_duration_seconds{worker_id=\"0\"}"),
        "{text}"
    );
    assert!(
        text.contains("phpx_worker_healthy{worker_id=\"0\"} 1"),
        "{text}"
    );
}

#[test]
fn hard_limit_terminates_protectively_with_controlled_workload() {
    // P0-C acceptance: hard-threshold action in an isolated worker.
    // The 64MB hog crosses base+48MB; the fresh replacement stays below.
    let base = measure_base_rss_bytes();
    let mut cfg = guardian_config(1, 0, 0);
    cfg.memory_hard_bytes = base + 48 * MB;
    let pool = PersistentPool::new(cfg).expect("pool must start");
    let pid_before = pool.worker_snapshots()[0].pid;
    let resp = pool.execute(&mem_req("hard-1", 64)).expect("execute");
    assert_eq!(
        resp.status,
        200,
        "body: {}",
        String::from_utf8_lossy(&resp.body)
    );
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    assert_eq!(snaps[0].hard_hits, 1, "{snaps:?}");
    assert_eq!(snaps[0].soft_hits, 0, "{snaps:?}");
    assert_ne!(snaps[0].pid, pid_before, "hard limit must spawn a new PID");
    assert_eq!(snaps[0].state, WorkerState::Ready, "{snaps:?}");
    assert!(
        snaps[0]
            .restarts_by_reason
            .iter()
            .any(|(c, n)| c == "hard-limit" && *n == 1),
        "{snaps:?}"
    );
    // Pool keeps serving on the replacement.
    let good = pool
        .execute(&req_for("hard-after", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(good.status, 200);
    let text = pool.prometheus_text();
    assert!(
        text.contains("phpx_worker_memory_limit_hits_total{worker_id=\"0\",threshold=\"hard\"} 1"),
        "{text}"
    );
}

#[test]
fn sampler_thread_refreshes_idle_workers() {
    // The background sampler keeps sampling idle workers: after a request
    // (age ~0), sleeping 3x the 1s interval must show a fresh sample
    // (last_ok age stays small instead of growing to the sleep duration).
    // A huge soft limit means sampling runs but never fires.
    let mut cfg = guardian_config(1, 4096, 0);
    cfg.memory_sample_interval = Duration::from_secs(1);
    let pool = PersistentPool::new(cfg).expect("pool must start");
    let resp = pool
        .execute(&req_for("idle-1", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(resp.status, 200);
    std::thread::sleep(Duration::from_secs(3));
    let snaps = pool.worker_snapshots();
    let age = snaps[0]
        .last_ok_age_secs
        .expect("sampler must refresh last_ok");
    assert!(age <= 2, "idle worker not resampled by sampler: {snaps:?}");
    assert!(snaps[0].rss_bytes > 0, "sampler must record RSS: {snaps:?}");
    assert!(snaps[0].healthy, "{snaps:?}");
    // Pool still serves after the sampler has been sweeping.
    let good = pool
        .execute(&req_for("idle-2", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(good.status, 200);
}

#[test]
fn fixed_pool_serves_requests_with_identity() {
    let pool = PersistentPool::new(pool_config(2)).expect("pool must start");
    assert_eq!(pool.ready_count(), 2);
    for (i, snap) in pool.worker_snapshots().iter().enumerate() {
        assert_eq!(snap.state, WorkerState::Ready, "{snap:?}");
        assert_eq!(snap.id, i as u64);
        assert_eq!(snap.incarnation, 1);
        assert!(snap.pid.is_some(), "{snap:?}");
    }
    for i in 0..4 {
        let resp = pool
            .execute(&req_for(&format!("p0b-{i}"), &fixture_root(), "index.php"))
            .expect("execute must succeed");
        assert_eq!(resp.status, 200);
        assert!(String::from_utf8_lossy(&resp.body).contains(&format!("p0b-{i}")));
    }
    let total: u64 = pool.worker_snapshots().iter().map(|s| s.completed).sum();
    assert_eq!(total, 4);
}

#[test]
fn worker_reused_across_requests_same_pid() {
    // P0-B core proof (inverse of P0-A): one worker serves sequential
    // requests in the SAME OS process, so getmypid() must be stable.
    let pool = PersistentPool::new(pool_config(1)).expect("pool must start");
    let parse_pid = |body: &str| -> String {
        body.trim()
            .strip_prefix("pid=")
            .unwrap_or_else(|| panic!("expected pid=<n>, got: {body}"))
            .trim()
            .to_string()
    };
    let r1 = pool
        .execute(&req_for("reuse-1", &fixture_root(), "pid.php"))
        .unwrap();
    let r2 = pool
        .execute(&req_for("reuse-2", &fixture_root(), "pid.php"))
        .unwrap();
    assert_eq!(r1.status, 200);
    assert_eq!(r2.status, 200);
    let pid1 = parse_pid(&String::from_utf8_lossy(&r1.body));
    let pid2 = parse_pid(&String::from_utf8_lossy(&r2.body));
    assert_eq!(pid1, pid2, "persistent worker must reuse its process");
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].completed, 2);
    assert_eq!(snaps[0].incarnation, 1);
    assert_eq!(snaps[0].restarts, 0);
}

#[test]
fn crash_restarts_worker_with_new_incarnation() {
    // crash.php calls exit(1): the worker process dies and the supervisor
    // must restart it (new incarnation) without killing the gateway.
    let pool = PersistentPool::new(pool_config(1)).expect("pool must start");
    let pid_before = pool.worker_snapshots()[0].pid;
    let bad = pool
        .execute(&req_for("crash-1", &fixture_root(), "crash.php"))
        .expect("crash maps to Ok(500), not transport Err");
    assert_eq!(bad.status, 500);
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].state, WorkerState::Ready, "{snaps:?}");
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    assert_eq!(snaps[0].restarts, 1, "{snaps:?}");
    assert_ne!(snaps[0].pid, pid_before, "restart must spawn a new PID");
    // Pool serves again on the replacement.
    let good = pool
        .execute(&req_for("after-crash", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(good.status, 200);
    assert!(String::from_utf8_lossy(&good.body).contains("after-crash"));
}

#[test]
fn timeout_kills_and_restarts_hung_worker() {
    let mut cfg = pool_config(1);
    cfg.execution_timeout = Duration::from_secs(2);
    let pool = PersistentPool::new(cfg).expect("pool must start");
    let resp = pool
        .execute(&req_for("hang-1", &fixture_root(), "sleep.php"))
        .expect("timeout maps to Ok(504)");
    assert_eq!(resp.status, 504);
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].state, WorkerState::Ready, "{snaps:?}");
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    // Replacement is healthy.
    let good = pool
        .execute(&req_for("after-hang", &fixture_root(), "pid.php"))
        .unwrap();
    assert_eq!(good.status, 200);
}

#[test]
fn startup_failure_reports_diagnostic() {
    // Unspawnable php: pool construction fails fast with reasons (P0-B
    // startup gate), instead of serving half-dead capacity.
    let cfg = PersistentPoolConfig {
        php_bin: "/nonexistent-php-binary-xyz".to_string(),
        worker_count: 2,
        startup_timeout: Duration::from_secs(5),
        restart_max_retries: 0,
        ..Default::default()
    };
    let err = match PersistentPool::new(cfg) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("pool with bad php must fail to start"),
    };
    assert!(err.contains("0/2 workers ready"), "{err}");
}

#[test]
fn restart_budget_exhaustion_marks_failed_with_reason() {
    // Temp worker script deleted mid-test: the first restart attempt fails
    // and (budget 0) the worker goes Failed with a diagnostic; executes
    // surface the Failed reason instead of hanging or looping forever.
    let script_src = include_str!("../src/worker-persistent.php");
    let tmp = std::env::temp_dir().join(format!("phpx-p0b-budget-{}.php", std::process::id()));
    std::fs::write(&tmp, script_src).expect("stage temp worker script");
    let cfg = PersistentPoolConfig {
        worker_count: 1,
        script_path: Some(tmp.clone()),
        startup_timeout: Duration::from_secs(15),
        execution_timeout: Duration::from_secs(10),
        restart_max_retries: 0,
        restart_backoff_base: Duration::from_millis(1),
        restart_backoff_max: Duration::from_millis(5),
        ..Default::default()
    };
    let pool = PersistentPool::new(cfg).expect("pool must start");
    assert_eq!(pool.ready_count(), 1);
    std::fs::remove_file(&tmp).expect("delete worker script");
    // Crash the worker so the supervisor must respawn from the deleted script.
    let bad = pool
        .execute(&req_for("crash-budget", &fixture_root(), "crash.php"))
        .unwrap();
    assert_eq!(bad.status, 500);
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].state, WorkerState::Failed, "{snaps:?}");
    assert!(
        snaps[0]
            .exit_reason
            .as_deref()
            .unwrap_or("")
            .contains("restart failed"),
        "{snaps:?}"
    );
    // Let the crash-loop circuit window (a few ms at this budget) expire so
    // the next execute reaches the terminal Failed diagnostic deterministically.
    std::thread::sleep(Duration::from_millis(100));
    let err = pool
        .execute(&req_for("after-failed", &fixture_root(), "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Failed"), "{err}");
    let text = pool.prometheus_text();
    assert!(
        text.contains("phpx_worker_state{worker_id=\"0\"} 7"),
        "{text}"
    );
}

#[test]
fn queue_bound_is_enforced() {
    let mut cfg = pool_config(1);
    cfg.queue_bound = 1;
    cfg.queue_wait_timeout = Duration::from_millis(200);
    cfg.execution_timeout = Duration::from_secs(15);
    let pool = std::sync::Arc::new(PersistentPool::new(cfg).expect("pool must start"));
    let p1 = pool.clone();
    let h = std::thread::spawn(move || {
        let _ = p1.execute(&req_for("holder", &fixture_root(), "sleep.php"));
    });
    // Wait until the holder actually occupies the single worker (no sleeps).
    let start = std::time::Instant::now();
    while pool.load().0 != 1 {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "holder never occupied the worker"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let err = pool
        .execute(&req_for("victim", &fixture_root(), "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("overload"), "{err}");
    h.join().unwrap();
    let resp = pool
        .execute(&req_for("after-queue", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(resp.status, 200);
}

#[test]
fn recycle_replaces_worker_after_budget() {
    let mut cfg = pool_config(1);
    cfg.max_requests_per_worker = 2;
    let pool = PersistentPool::new(cfg).expect("pool must start");
    let pid_before = pool.worker_snapshots()[0].pid;
    for i in 0..2 {
        let resp = pool
            .execute(&req_for(&format!("rec-{i}"), &fixture_root(), "index.php"))
            .unwrap();
        assert_eq!(resp.status, 200);
    }
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    assert_ne!(snaps[0].pid, pid_before, "recycle must spawn a new PID");
    assert_eq!(
        snaps[0].completed, 0,
        "counter resets on replacement: {snaps:?}"
    );
    // Replacement serves.
    let resp = pool
        .execute(&req_for("rec-after", &fixture_root(), "index.php"))
        .unwrap();
    assert_eq!(resp.status, 200);
}

#[test]
fn sapi_state_reset_per_request_on_reuse() {
    // Same worker serves different query/cookie/body values: per-request
    // SAPI reset must hold despite process reuse.
    let pool = PersistentPool::new(pool_config(1)).expect("pool must start");
    for (name, cookie, body) in [("ara", "c1", "b1"), ("budi", "c2", "b2")] {
        let mut req = req_for(&format!("sapi-{name}"), &composer_root(), "index.php");
        req.query = format!("name={name}");
        req.headers
            .insert("cookie".to_string(), format!("phpx_test={cookie}"));
        req.headers
            .insert("user-agent".to_string(), "phpx-p0b-test".to_string());
        req.body = body.as_bytes().to_vec();
        let resp = pool.execute(&req).unwrap();
        assert_eq!(
            resp.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&resp.body)
        );
        let text = String::from_utf8_lossy(&resp.body).to_string();
        assert!(text.contains(&format!("query_name={name}")), "{text}");
        assert!(text.contains(&format!("cookie={cookie}")), "{text}");
        assert!(text.contains(&format!("body={body}")), "{text}");
        assert!(text.contains("ua=phpx-p0b-test"), "{text}");
    }
}

#[test]
fn drain_rejects_new_work_and_stops_workers() {
    let pool = PersistentPool::new(pool_config(1)).expect("pool must start");
    pool.begin_drain();
    let err = pool
        .execute(&req_for("drain-victim", &fixture_root(), "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("draining"), "{err}");
    assert!(pool.drain(Duration::from_secs(5)));
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].state, WorkerState::Stopped, "{snaps:?}");
    assert_eq!(snaps[0].pid, None, "{snaps:?}");
}

#[test]
fn metrics_distinguish_persistent_mode() {
    let pool = PersistentPool::new(pool_config(2)).expect("pool must start");
    let _ = pool.execute(&req_for("m-1", &fixture_root(), "index.php"));
    let text = pool.prometheus_text();
    assert!(
        text.contains("phpx_execution_mode_info{execution_mode=\"persistent\""),
        "{text}"
    );
    for key in [
        "phpx_pool_completed_total",
        "phpx_pool_max_workers",
        "phpx_worker_state",
        "phpx_worker_requests_total",
        "phpx_worker_restarts_total",
        "phpx_worker_uptime_seconds",
    ] {
        assert!(text.contains(key), "missing {key}:\n{text}");
    }
}
