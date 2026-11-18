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
