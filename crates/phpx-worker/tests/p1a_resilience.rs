use phpx_runtime::PhpRequest;
use phpx_worker::{BoundedPool, PersistentPool, PersistentPoolConfig, PoolConfig, WorkerState};
use std::path::PathBuf;
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

// RC-track acceptance thresholds (single-node dev scope; generous for shared
// CI runners so they catch regressions, not machine noise). Tied to the
// pool config below: startup_timeout 15s, execution_timeout 10s.
const STARTUP_BUDGET_MS: u64 = 15_000;
// Detection -> restart initiation: kill issued promptly after EOF discovery.
const RECOVERY_DETECTION_TO_RESTART_MAX_MS: u64 = 5_000;
// Restart initiation -> replacement ready: bounded by the startup budget + slack.
const RECOVERY_RESTART_TO_READY_MAX_MS: u64 = STARTUP_BUDGET_MS + 2_000;
// End to end per incident: startup budget + detection/exchange slack.
const RECOVERY_TOTAL_MAX_MS: u64 = STARTUP_BUDGET_MS + 5_000;
// User-observable recovery: kill -> failing request -> first 200 on the
// replacement (total + one execution budget).
const TIME_TO_FIRST_SUCCESS_MAX_MS: u64 = RECOVERY_TOTAL_MAX_MS + 10_000;

fn persistent_config(workers: u32) -> PersistentPoolConfig {
    PersistentPoolConfig {
        worker_count: workers,
        queue_bound: 16,
        queue_wait_timeout: Duration::from_secs(10),
        execution_timeout: Duration::from_secs(10),
        startup_timeout: Duration::from_secs(15),
        drain_timeout: Duration::from_secs(10),
        max_requests_per_worker: 1000,
        restart_max_retries: 5,
        restart_backoff_base: Duration::from_millis(10),
        restart_backoff_max: Duration::from_millis(100),
        ..Default::default()
    }
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let status = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .expect("kill must exist on unix test hosts");
    assert!(status.success(), "kill -9 {pid} failed");
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let status = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("taskkill must exist on Windows test hosts");
    assert!(status.success(), "taskkill {pid} failed");
}

#[test]
fn crash_while_idle_restarts_on_next_request() {
    // P1-A: a worker killed while idle (no in-flight request) must be
    // detected on next use and replaced with a new incarnation. The failed
    // request maps to 500 (crash accounting); the pool keeps serving.
    let pool = PersistentPool::new(persistent_config(1)).expect("pool must start");
    let good = pool.execute(&req_for("idle-1", "index.php")).unwrap();
    assert_eq!(good.status, 200);
    let pid = pool.worker_snapshots()[0].pid.expect("worker has a pid");
    kill_pid(pid);
    // Give the OS a moment to reap; the exchange fails regardless (EOF).
    std::thread::sleep(Duration::from_millis(300));
    let bad = pool
        .execute(&req_for("idle-2", "index.php"))
        .expect("crash maps to Ok(500), not transport Err");
    assert_eq!(bad.status, 500);
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].state, WorkerState::Ready, "{snaps:?}");
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    assert_ne!(snaps[0].pid, Some(pid), "restart must spawn a new PID");
    assert!(
        snaps[0]
            .restarts_by_reason
            .iter()
            .any(|(c, _)| c == "crash"),
        "{snaps:?}"
    );
    let good2 = pool.execute(&req_for("idle-3", "index.php")).unwrap();
    assert_eq!(good2.status, 200);
}

#[test]
fn recovery_timeline_reports_detection_to_ready() {
    // P1-A: recovery time is reported per leg (detection -> restart start ->
    // replacement ready), not as one ambiguous number.
    let pool = PersistentPool::new(persistent_config(1)).expect("pool must start");
    let pid = pool.worker_snapshots()[0].pid.expect("worker has a pid");
    kill_pid(pid);
    std::thread::sleep(Duration::from_millis(300));
    let bad = pool.execute(&req_for("rec-1", "crash.php")).unwrap();
    assert_eq!(bad.status, 500);
    let snaps = pool.worker_snapshots();
    let rec = snaps[0].last_recovery.clone().expect("incident recorded");
    assert_eq!(rec.cause, "crash", "{rec:?}");
    let d2r = rec.detection_to_restart_ms.expect("restart started");
    let r2r = rec.restart_to_ready_ms.expect("replacement ready");
    let total = rec.total_ms.expect("total measured");
    assert_eq!(total, d2r + r2r, "legs must add to the total: {rec:?}");
    assert!(
        d2r <= RECOVERY_DETECTION_TO_RESTART_MAX_MS,
        "detection->restart {d2r}ms exceeds {RECOVERY_DETECTION_TO_RESTART_MAX_MS}ms: {rec:?}"
    );
    assert!(
        r2r <= RECOVERY_RESTART_TO_READY_MAX_MS,
        "restart->ready {r2r}ms exceeds {RECOVERY_RESTART_TO_READY_MAX_MS}ms: {rec:?}"
    );
    assert!(
        total <= RECOVERY_TOTAL_MAX_MS,
        "recovery total {total}ms exceeds {RECOVERY_TOTAL_MAX_MS}ms: {rec:?}"
    );
    // Next request succeeds: time-to-first-success follows readiness.
    let good = pool.execute(&req_for("rec-2", "index.php")).unwrap();
    assert_eq!(good.status, 200);
    let text = pool.prometheus_text();
    assert!(
        text.contains("phpx_worker_last_recovery_duration_seconds{worker_id=\"0\"}"),
        "{text}"
    );
}

#[test]
fn time_to_first_success_after_idle_crash_within_budget() {
    // User-observable recovery: kill the idle worker, then measure wall time
    // from the kill to the first 200 served by the replacement (the request
    // in between maps to 500 by design). Must fit total + one execution
    // budget; proves recovery is not just internally recorded but served.
    let pool = PersistentPool::new(persistent_config(1)).expect("pool must start");
    let good = pool.execute(&req_for("tts-0", "index.php")).unwrap();
    assert_eq!(good.status, 200);
    let pid = pool.worker_snapshots()[0].pid.expect("worker has a pid");
    let t0 = Instant::now();
    kill_pid(pid);
    std::thread::sleep(Duration::from_millis(300));
    let bad = pool
        .execute(&req_for("tts-1", "index.php"))
        .expect("crash maps to Ok(500), not transport Err");
    assert_eq!(bad.status, 500);
    let good = pool.execute(&req_for("tts-2", "index.php")).unwrap();
    assert_eq!(good.status, 200);
    let elapsed_ms = t0.elapsed().as_millis() as u64;
    assert!(
        elapsed_ms <= TIME_TO_FIRST_SUCCESS_MAX_MS,
        "kill->first-200 took {elapsed_ms}ms, exceeds {TIME_TO_FIRST_SUCCESS_MAX_MS}ms"
    );
}

#[test]
fn crash_loop_opens_circuit_then_admits_again() {
    // P1-A: repeated consecutive crashes open the crash-loop circuit
    // (fast 503 backpressure); after the backoff window the pool admits
    // again instead of erroring forever.
    let mut cfg = persistent_config(1);
    cfg.restart_max_retries = 3;
    cfg.restart_backoff_base = Duration::from_millis(1000);
    cfg.restart_backoff_max = Duration::from_millis(2000);
    cfg.queue_wait_timeout = Duration::from_secs(5);
    let pool = PersistentPool::new(cfg).expect("pool must start");
    // Three consecutive crashes stay admitted (500 each); the third trips
    // the budget and opens the crash-loop circuit.
    for i in 0..3 {
        let r = pool
            .execute(&req_for(&format!("loop-{i}"), "crash.php"))
            .unwrap();
        assert_eq!(r.status, 500, "crash {i} must map to 500");
    }
    // Circuit now open: immediate admission fails fast with backpressure.
    let t0 = Instant::now();
    let err = pool
        .execute(&req_for("loop-blocked", "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("circuit"), "{err}");
    assert!(
        t0.elapsed() < Duration::from_secs(4),
        "circuit must fail fast"
    );
    // After the backoff window the pool admits work again.
    std::thread::sleep(Duration::from_millis(1500));
    let r = pool.execute(&req_for("loop-after", "crash.php")).unwrap();
    assert_eq!(
        r.status, 500,
        "admitted again after backoff (crashes again)"
    );
    let text = pool.prometheus_text();
    assert!(text.contains("phpx_pool_circuit_breaks_total"), "{text}");
}

#[test]
fn drain_with_in_flight_persistent_completes_then_stops() {
    // P1-A: graceful shutdown drains in-flight work within the deadline:
    // the in-flight request finishes 200, new work is rejected fast, and
    // workers end Stopped.
    let mut cfg = persistent_config(1);
    cfg.execution_timeout = Duration::from_secs(15);
    let pool = std::sync::Arc::new(PersistentPool::new(cfg).expect("pool must start"));
    let p1 = pool.clone();
    let holder = std::thread::spawn(move || {
        p1.execute(&req_for("inflight", "sleep.php"))
            .expect("in-flight request must complete 200")
    });
    // Wait until the worker is actually busy (no sleeps-by-faith).
    let start = Instant::now();
    while pool.load().0 != 1 {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "holder never started"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let p2 = pool.clone();
    let drainer = std::thread::spawn(move || p2.drain(Duration::from_secs(12)));
    std::thread::sleep(Duration::from_millis(200));
    let t0 = Instant::now();
    let err = pool
        .execute(&req_for("drain-victim", "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("draining"), "{err}");
    assert!(
        t0.elapsed() < Duration::from_secs(4),
        "drain reject must be fast"
    );
    let held = holder.join().expect("holder thread");
    assert_eq!(held.status, 200);
    assert!(
        drainer.join().expect("drainer thread"),
        "drain must complete"
    );
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].state, WorkerState::Stopped, "{snaps:?}");
    assert_eq!(snaps[0].pid, None, "{snaps:?}");
}

#[test]
fn drain_with_in_flight_oneshot_completes_then_stops() {
    // Same graceful-shutdown contract for the one-shot pool.
    let cfg = PoolConfig {
        max_workers: 1,
        queue_bound: 4,
        queue_wait_timeout: Duration::from_secs(5),
        execution_timeout: Duration::from_secs(15),
        drain_timeout: Duration::from_secs(12),
        ..Default::default()
    };
    let pool = std::sync::Arc::new(BoundedPool::new(cfg).unwrap());
    let p1 = pool.clone();
    let holder = std::thread::spawn(move || {
        p1.execute(&req_for("inflight-1s", "sleep.php"))
            .expect("in-flight must complete")
    });
    let start = Instant::now();
    while pool.snapshot().active != 1 {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "holder never started"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let p2 = pool.clone();
    let drainer = std::thread::spawn(move || p2.drain(Duration::from_secs(12)));
    std::thread::sleep(Duration::from_millis(200));
    let err = pool
        .execute(&req_for("drain-victim-1s", "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("draining"), "{err}");
    let held = holder.join().expect("holder thread");
    assert_eq!(held.status, 200);
    assert!(drainer.join().expect("drainer thread"));
}

#[test]
fn app_exit_kills_worker_and_pool_resyncs() {
    // P1-B manual-QA finding: apps that terminate the process (exit/die,
    // e.g. WordPress wp_redirect/REST paths) kill a persistent worker and
    // flush raw bytes outside any frame. The supervisor must turn that into
    // a 500 + restart (never a hang, never a poisoned stream): the next
    // request is served by the replacement worker.
    let pool = PersistentPool::new(persistent_config(1)).expect("pool must start");
    let bad = pool
        .execute(&req_for("bye-1", "byeframe.php"))
        .expect("exit maps to Ok(500), not transport Err");
    assert_eq!(bad.status, 500);
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].incarnation, 2, "{snaps:?}");
    assert_eq!(snaps[0].state, WorkerState::Ready, "{snaps:?}");
    assert!(
        snaps[0]
            .restarts_by_reason
            .iter()
            .any(|(c, _)| c == "crash"),
        "exit-without-frame must count as a crash: {snaps:?}"
    );
    // Stream resynced: the replacement serves framed responses again.
    for i in 0..3 {
        let good = pool
            .execute(&req_for(&format!("bye-after-{i}"), "index.php"))
            .unwrap();
        assert_eq!(good.status, 200);
    }
    let snaps = pool.worker_snapshots();
    assert_eq!(snaps[0].incarnation, 2, "no further restarts: {snaps:?}");
}

#[test]
fn reload_is_drain_then_rebuild() {
    // P1-A: PHPX has no live reload (documented: restart the gateway).
    // The supported reload story is drain + rebuild, pinned here: the old
    // pool stops cleanly and a fresh pool serves immediately.
    let pool = PersistentPool::new(persistent_config(1)).expect("pool must start");
    let r1 = pool.execute(&req_for("rel-1", "index.php")).unwrap();
    assert_eq!(r1.status, 200);
    assert!(pool.drain(Duration::from_secs(5)));
    let err = pool
        .execute(&req_for("rel-2", "index.php"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("draining"), "{err}");
    let pool2 = PersistentPool::new(persistent_config(1)).expect("rebuild must start");
    let r3 = pool2.execute(&req_for("rel-3", "index.php")).unwrap();
    assert_eq!(r3.status, 200);
}

#[test]
fn persistent_worker_state_leaks_are_visible_and_oneshot_is_clean() {
    // P1-A: documents the persistent state-reset limit — $GLOBALS set by app
    // code persists across requests on a reused worker, while one-shot
    // resets every time. If this test ever shows global=1 twice on
    // persistent, the reset contract got stronger (update the contract doc).
    let pool = PersistentPool::new(persistent_config(1)).expect("pool must start");
    let body = |id: &str| {
        String::from_utf8_lossy(
            &pool
                .execute(&req_for(id, "leak.php"))
                .expect("leak probe")
                .body,
        )
        .to_string()
    };
    assert!(
        body("leak-p1").contains("global=1"),
        "first hit starts clean"
    );
    let second = body("leak-p2");
    assert!(
        second.contains("global=2"),
        "persistent reuse must show the documented leak: {second}"
    );

    let cfg = PoolConfig {
        max_workers: 1,
        queue_bound: 4,
        queue_wait_timeout: Duration::from_secs(5),
        execution_timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let one = BoundedPool::new(cfg).unwrap();
    for id in ["leak-o1", "leak-o2"] {
        let b = String::from_utf8_lossy(&one.execute(&req_for(id, "leak.php")).unwrap().body)
            .to_string();
        assert!(b.contains("global=1"), "one-shot must isolate: {b}");
    }
}

#[test]
fn short_soak_mixed_workload_stays_healthy() {
    // P1-A short soak: mixed workload (fast + crash every 10th) over ~100+
    // requests. Asserts conservation (every request accounted), restart
    // accounting matches the injected crashes, and no worker ends Failed.
    // Long soak (1h/6h/24h RSS trend) stays in scripts/soak-long.sh with
    // archived results; this is the repeatable CI gate.
    let pool = PersistentPool::new(persistent_config(2)).expect("pool must start");
    let total = 120;
    let mut ok = 0;
    let mut crashed = 0;
    for i in 0..total {
        let entry = if i % 10 == 5 {
            "crash.php"
        } else {
            "index.php"
        };
        let resp = pool.execute(&req_for(&format!("soak-{i}"), entry)).unwrap();
        match resp.status {
            200 => ok += 1,
            500 => crashed += 1,
            s => panic!("unexpected status {s} at soak-{i}"),
        }
    }
    assert_eq!(ok + crashed, total);
    assert_eq!(crashed, 12, "one crash per 10 requests");
    let active_workers = pool
        .worker_snapshots()
        .into_iter()
        .filter(|s| s.state == WorkerState::Ready)
        .count();
    assert_eq!(active_workers, 2, "all workers Ready after soak");
    let restarts: u64 = pool.worker_snapshots().iter().map(|s| s.restarts).sum();
    assert_eq!(restarts, 12, "one restart per injected crash");
    let text = pool.prometheus_text();
    assert!(text.contains("phpx_pool_completed_total 108"), "{text}");
    assert!(text.contains("phpx_pool_failed_total 12"), "{text}");
}
