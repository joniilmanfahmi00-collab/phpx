use std::process::Command;

fn bin() -> String {
    env!("CARGO_BIN_EXE_phpx").to_string()
}

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin()).args(args).output().expect("run phpx");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (code, stdout, stderr)
}

#[test]
fn version_reports_preview() {
    let (code, out, _) = run(&["version"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("developer preview"), "{out}");
}

#[test]
fn doctor_ok_and_json() {
    let (code, out, _) = run(&["doctor"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("M4 hardening"), "{out}");
    let (code, out, _) = run(&["--json", "doctor"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("doctor json");
    assert!(v.get("checks").is_some(), "{out}");
}

#[test]
fn runtime_and_workers_status() {
    let (code, out, _) = run(&["runtime", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("process"), "{out}");
    assert!(out.contains("execution_mode"), "{out}");
    let (code, out, _) = run(&["--json", "runtime", "list"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("runtime list json");
    assert_eq!(
        v.get("execution_mode").and_then(|m| m.as_str()),
        Some("one-shot"),
        "{out}"
    );
    let (code, out, _) = run(&["runtime", "inspect"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("mode:"), "{out}");
    assert!(out.contains("execution_mode"), "{out}");
    let (code, out, _) = run(&["--json", "runtime", "inspect"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("runtime inspect json");
    assert_eq!(
        v.get("execution_mode").and_then(|m| m.as_str()),
        Some("one-shot"),
        "{out}"
    );
    let (code, out, _) = run(&["workers", "status"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("bounded pool"), "{out}");
    assert!(out.contains("execution_mode=one-shot"), "{out}");
    let (code, out, _) = run(&["--json", "workers", "status"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("workers status json");
    assert_eq!(
        v.get("execution_mode").and_then(|m| m.as_str()),
        Some("one-shot"),
        "{out}"
    );
    assert_eq!(
        v.get("memory_soft_limit").and_then(|m| m.as_str()),
        Some("0"),
        "{out}"
    );
    assert_eq!(
        v.get("memory_hard_limit").and_then(|m| m.as_str()),
        Some("0"),
        "{out}"
    );
    assert_eq!(
        v.get("memory_sample_interval_seconds")
            .and_then(|m| m.as_u64()),
        Some(5),
        "{out}"
    );
    let (code, out, _) = run(&["workers", "reload"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("valid"), "{out}");
}

#[test]
fn queue_status_real_scheduler_runs() {
    // M5 phase 1+2: queue status is real (0), scheduler run is a real
    // single-pass dispatch (0; disabled by default in the example config).
    let (code, out, _) = run(&["queue", "status"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("driver=") || out.contains("driver"), "{out}");
    let (code, out, _) = run(&["scheduler", "run"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("scheduler"), "{out}");
    let (code, out, _) = run(&["--json", "scheduler", "run"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("scheduler json");
    assert!(v.get("dispatched").is_some(), "{out}");
}

#[test]
fn cache_and_storage_status_real() {
    // M5 phase 3: cache + storage are real (exit 0, JSON has entries/objects).
    let (code, out, _) = run(&["cache", "status"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("cache"), "{out}");
    let (code, out, _) = run(&["--json", "cache", "status"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("cache json");
    assert!(v.get("entries").is_some(), "{out}");
    let (code, out, _) = run(&["storage", "status"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("storage"), "{out}");
    let (code, out, _) = run(&["--json", "storage", "status"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("storage json");
    assert!(v.get("objects").is_some(), "{out}");
}

#[test]
fn metrics_mentions_healthz() {
    let (code, out, _) = run(&["metrics"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("healthz"), "{out}");
}
