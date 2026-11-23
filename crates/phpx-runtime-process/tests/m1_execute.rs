use phpx_runtime::PhpRequest;
use phpx_runtime_process::{execute_one_shot, ExecuteOptions, PhpDiscovery};
use std::path::PathBuf;
use std::time::Duration;

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

#[test]
fn php_heap_limit_is_enforced_via_d_flag() {
    // P0-C: memory_limit is a real PHP-level limit (Zend heap), enforced by
    // passing `-d memory_limit=...` to the child. 8M heap cannot hold 32M.
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let mut req = req_for("heap-1", "mem.php");
    req.query = "mb=32".to_string();
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(10),
        memory_limit: Some("8M".to_string()),
        ..Default::default()
    };
    let resp = execute_one_shot(&info.bin, &req, &opts).unwrap();
    // PHP fatals with "Allowed memory size exhausted" -> non-zero exit -> 500.
    assert_eq!(
        resp.status,
        500,
        "body: {}",
        String::from_utf8_lossy(&resp.body)
    );
    // And the surviving default path reports the default ini (no -d).
    let plain = execute_one_shot(
        &info.bin,
        &req_for("heap-2", "mem.php"),
        &ExecuteOptions {
            timeout: Duration::from_secs(10),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(plain.status, 200);
    let body = String::from_utf8_lossy(&plain.body).to_string();
    assert!(body.contains("heap_limit="), "{body}");
}

#[test]
fn discovers_php_and_extensions() {
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().expect("php must be installed");
    assert!(
        info.version_line.contains("PHP"),
        "unexpected: {}",
        info.version_line
    );
    for want in ["json", "mbstring"] {
        assert!(
            info.extensions.iter().any(|e| e == want),
            "missing ext {want}"
        );
    }
}

#[test]
fn executes_vanilla_fixture() {
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let req = req_for("m1-vanilla-1", "index.php");
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let resp = execute_one_shot(&info.bin, &req, &opts).unwrap();
    assert_eq!(
        resp.status,
        200,
        "body: {}",
        String::from_utf8_lossy(&resp.body)
    );
    let body = String::from_utf8_lossy(&resp.body);
    assert!(body.contains("hello from phpx vanilla"), "{body}");
    assert!(body.contains("m1-vanilla-1"), "{body}");
}

#[test]
fn sequential_requests_do_not_cross_contaminate() {
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    for id in ["seq-A", "seq-B", "seq-C"] {
        let resp = execute_one_shot(&info.bin, &req_for(id, "isolate.php"), &opts).unwrap();
        assert_eq!(resp.status, 200);
        let body = String::from_utf8_lossy(&resp.body).to_string();
        assert!(body.contains("clean"), "{body}");
        assert!(body.contains(id), "{body}");
        assert!(!body.contains("LEAK"), "{body}");
    }
}

#[test]
fn large_response_does_not_deadlock_supervisor() {
    // P0-D regression: 256KB stdout (> 64KB OS pipe buffer) must stream
    // through concurrent drain threads. Draining only after child exit
    // deadlocked real apps (Laravel welcome ~= 80KB -> 504).
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    let resp = execute_one_shot(&info.bin, &req_for("big-1", "big.php"), &opts).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.len(), 256 * 1024, "truncated body?");
    assert!(resp.body.iter().all(|&b| b == b'x'));
}

#[test]
fn one_shot_process_lifetime_is_request_scoped() {
    // P0-A: each admitted request spawns a fresh OS process, so getmypid()
    // must differ across sequential requests. A persistent pool (P0-B) would
    // reuse the PID; this test pins the one-shot lifetime.
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let parse_pid = |body: &str| -> String {
        body.trim()
            .strip_prefix("pid=")
            .unwrap_or_else(|| panic!("expected pid=<n>, got: {body}"))
            .trim()
            .to_string()
    };
    let r1 = execute_one_shot(&info.bin, &req_for("pid-1", "pid.php"), &opts).unwrap();
    assert_eq!(
        r1.status,
        200,
        "body: {}",
        String::from_utf8_lossy(&r1.body)
    );
    let r2 = execute_one_shot(&info.bin, &req_for("pid-2", "pid.php"), &opts).unwrap();
    assert_eq!(
        r2.status,
        200,
        "body: {}",
        String::from_utf8_lossy(&r2.body)
    );
    let pid1 = parse_pid(&String::from_utf8_lossy(&r1.body));
    let pid2 = parse_pid(&String::from_utf8_lossy(&r2.body));
    assert_ne!(
        pid1, pid2,
        "one-shot must spawn a fresh process per request"
    );
}

#[test]
fn crash_does_not_take_down_supervisor() {
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let bad = execute_one_shot(&info.bin, &req_for("m1-crash-1", "crash.php"), &opts).unwrap();
    assert_eq!(bad.status, 500);
    // Supervisor still alive: next request succeeds.
    let good = execute_one_shot(&info.bin, &req_for("m1-after-crash", "index.php"), &opts).unwrap();
    assert_eq!(good.status, 200);
    assert!(String::from_utf8_lossy(&good.body).contains("m1-after-crash"));
}

#[test]
fn execution_timeout_kills_slow_worker() {
    let d = PhpDiscovery::from_env_or_default();
    let info = d.inspect().unwrap();
    let opts = ExecuteOptions {
        timeout: Duration::from_secs(1),
        ..Default::default()
    };
    let resp = execute_one_shot(&info.bin, &req_for("m1-timeout-1", "sleep.php"), &opts).unwrap();
    assert_eq!(resp.status, 504);
    // Pool/supervisor usable afterwards.
    let opts2 = ExecuteOptions {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let good =
        execute_one_shot(&info.bin, &req_for("m1-after-timeout", "index.php"), &opts2).unwrap();
    assert_eq!(good.status, 200);
}
