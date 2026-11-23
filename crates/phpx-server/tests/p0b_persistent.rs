use phpx_config::PhpxConfig;

fn cfg_for_persistent(fixture: &str, workers: u32) -> PhpxConfig {
    cfg_for_persistent_with_memory(fixture, workers, "0", "0")
}

fn cfg_for_persistent_with_memory(
    fixture: &str,
    workers: u32,
    soft: &str,
    hard: &str,
) -> PhpxConfig {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/{fixture}"))
        .to_string_lossy()
        .to_string();
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = 8080\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \"public\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.4\"\n[runtime.process]\nexecution_mode = \"persistent\"\nworkers = {workers}\nmin_workers = 1\nmax_requests = 100\nrequest_timeout_seconds = 10\nstartup_timeout_seconds = 15\nmemory_limit = \"256M\"\nmemory_soft_limit = \"{soft}\"\nmemory_hard_limit = \"{hard}\"\nmemory_sample_interval_seconds = 1\nqueue_wait_timeout_seconds = 5\nshutdown_drain_timeout_seconds = 5\nrestart_max_retries = 3\nrestart_backoff_ms = 50\nrestart_backoff_max_ms = 500\n"
    );
    PhpxConfig::parse_str(&toml).expect("persistent fixture config must parse")
}

async fn get(
    client: &reqwest::Client,
    base: &str,
    path: &str,
) -> (u16, String, reqwest::header::HeaderMap) {
    let r = client
        .get(format!("{base}{path}"))
        .send()
        .await
        .expect("GET");
    let status = r.status().as_u16();
    let headers = r.headers().clone();
    let body = r.text().await.unwrap_or_default();
    (status, body, headers)
}

#[tokio::test]
async fn persistent_gateway_serves_and_reuses_process() {
    // P0-B gateway proof: one worker serves sequential requests in the SAME
    // OS process (stable getmypid), the inverse of the P0-A one-shot proof.
    let cfg = cfg_for_persistent("vanilla-php", 1);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let (code, body, headers) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("hello from phpx vanilla"), "{body}");
    assert!(headers.contains_key("x-request-id"));

    let (_, p1, _) = get(&client, &base, "/pid.php").await;
    let (_, p2, _) = get(&client, &base, "/pid.php").await;
    assert!(p1.contains("pid="), "{p1}");
    assert_eq!(p1.trim(), p2.trim(), "persistent gateway must reuse PID");

    handle.abort();
}

#[tokio::test]
async fn persistent_crash_recovery_serves_next_request() {
    // crash.php calls exit(1): the supervised worker dies, the pool
    // restarts it, and the gateway keeps serving (no gateway crash).
    let cfg = cfg_for_persistent("vanilla-php", 1);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let (code, _, _) = get(&client, &base, "/crash.php").await;
    assert_eq!(code, 500, "crash must map to 500, not a dead gateway");
    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("hello from phpx vanilla"), "{body}");

    handle.abort();
}

#[tokio::test]
async fn persistent_metrics_expose_worker_series() {
    let cfg = cfg_for_persistent("vanilla-php", 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let _ = get(&client, &base, "/").await;
    let r = client
        .get(format!("{base}/metrics"))
        .send()
        .await
        .expect("GET metrics");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    for key in [
        "phpx_execution_mode_info{execution_mode=\"persistent\"",
        "phpx_pool_completed_total",
        "phpx_pool_max_workers",
        "phpx_worker_state{worker_id=\"0\"}",
        "phpx_worker_requests_total",
        "phpx_worker_restarts_total",
        "phpx_worker_uptime_seconds",
    ] {
        assert!(body.contains(key), "missing {key}:\n{body}");
    }
    handle.abort();
}

#[tokio::test]
async fn persistent_guardian_soft_limit_recycles_via_gateway() {
    // P0-C end-to-end: TOML memory_soft_limit -> build_state -> supervised
    // pool -> soft recycle visible in /metrics with a reason label.
    // 48MB soft: base stays below on any platform, the 64MB hog always crosses.
    let cfg = cfg_for_persistent_with_memory("vanilla-php", 1, "48M", "0");
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (code, body, _) = get(&client, &base, "/mem.php?mb=64").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("held=67108864"), "{body}");
    let r = client
        .get(format!("{base}/metrics"))
        .send()
        .await
        .expect("GET metrics");
    assert_eq!(r.status().as_u16(), 200);
    let mbody = r.text().await.unwrap();
    for key in [
        "phpx_execution_mode_info{execution_mode=\"persistent\"",
        "phpx_worker_rss_bytes{worker_id=\"0\"}",
        "phpx_worker_peak_rss_bytes{worker_id=\"0\"}",
        "phpx_worker_memory_limit_hits_total{worker_id=\"0\",threshold=\"soft\"} 1",
        "reason=\"soft-limit\"",
    ] {
        assert!(mbody.contains(key), "missing {key}:\n{mbody}");
    }
    // The replacement serves the next request.
    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    handle.abort();
}
