use phpx_config::PhpxConfig;

fn cfg_for(fixture: &str, workers: u32) -> PhpxConfig {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/{fixture}"))
        .to_string_lossy()
        .to_string();
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = 8080\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \"public\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.4\"\n[runtime.process]\nworkers = {workers}\nmin_workers = 1\nmax_requests = 100\nrequest_timeout_seconds = 10\nstartup_timeout_seconds = 10\nmemory_limit = \"256M\"\nqueue_wait_timeout_seconds = 2\nshutdown_drain_timeout_seconds = 5\nrestart_max_retries = 3\nrestart_backoff_ms = 50\nrestart_backoff_max_ms = 500\n"
    );
    PhpxConfig::parse_str(&toml).expect("m4 fixture config must parse")
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
async fn vanilla_no_cross_contam_and_crash_recovery() {
    let cfg = cfg_for("vanilla-php", 4);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Isolation: two sequential hits must both be clean (one-shot, no $GLOBALS leak).
    for _ in 0..2 {
        let (code, body, headers) = get(&client, &base, "/isolate.php").await;
        assert_eq!(code, 200, "{body}");
        assert!(body.contains("clean"), "{body}");
        assert!(!body.contains("LEAK"), "{body}");
        assert!(headers.contains_key("x-request-id"));
    }
    // RIDs must differ across requests (freshness proof).
    let (_, b1, _) = get(&client, &base, "/isolate.php").await;
    let (_, b2, _) = get(&client, &base, "/isolate.php").await;
    assert_ne!(b1, b2, "request ids should differ: {b1} vs {b2}");

    // Crash then recovery.
    let (code, _, _) = get(&client, &base, "/crash.php").await;
    assert_eq!(code, 500);
    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("hello from phpx vanilla"), "{body}");

    // No source leak anywhere.
    let (code, body, _) = get(&client, &base, "/hello.php").await;
    assert_eq!(code, 200, "{body}");
    assert!(!body.contains("<?php"), "{body}");

    handle.abort();
}

#[tokio::test]
async fn composer_autoload_and_sapi_superglobals() {
    let cfg = cfg_for("composer-autoload", 4);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let (code, body, headers) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("composer-autoload ok"), "{body}");
    assert!(!body.contains("<?php"), "{body}");
    assert!(headers.contains_key("x-request-id"));

    // $_GET via bootstrap (not just getenv).
    let r = client
        .get(format!("{base}/?name=ara"))
        .send()
        .await
        .expect("GET query");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("query_name=ara"), "{body}");
    assert!(body.contains("greeter=hello ara"), "{body}");

    // Headers + cookies + body round-trip.
    let r = client
        .post(format!("{base}/submit"))
        .header("user-agent", "phpx-m4-test")
        .header("cookie", "phpx_test=cookie123")
        .body("hello-body-m4")
        .send()
        .await
        .expect("POST");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("cookie=cookie123"), "{body}");
    assert!(body.contains("ua=phpx-m4-test"), "{body}");
    assert!(body.contains("hello-body-m4"), "{body}");

    // Urlencoded form body populates $_POST (web-SAPI parity for classic forms).
    let r = client
        .post(format!("{base}/submit"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("pname=ara-form&other=1")
        .send()
        .await
        .expect("POST form");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("post_pname=ara-form"), "{body}");
    assert!(body.contains("method=POST"), "{body}");

    // Static asset.
    let (code, body, _) = get(&client, &base, "/style.css").await;
    assert_eq!(code, 200, "{body}");

    handle.abort();
}

#[tokio::test]
async fn laravel_minimal_routing_and_json() {
    let cfg = cfg_for("laravel-minimal", 4);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("laravel-minimal ok"), "{body}");

    let r = client
        .get(format!("{base}/api/hello"))
        .header("accept", "application/json")
        .send()
        .await
        .expect("GET api");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("laravel-sim"), "{body}");

    let r = client
        .post(format!("{base}/submit"))
        .header("cookie", "phpx_test=laravel-cookie")
        .body("laravel-body")
        .send()
        .await
        .expect("POST");
    let body = r.text().await.unwrap();
    assert!(body.contains("laravel-body"), "{body}");
    assert!(body.contains("laravel-cookie"), "{body}");

    handle.abort();
}

#[tokio::test]
async fn wordpress_and_woocommerce_smoke() {
    let cfg = cfg_for("wordpress-minimal", 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("wordpress-minimal ok"), "{body}");
    assert!(body.contains("Hello WP"), "{body}");
    let (code, body, _) = get(&client, &base, "/?p=2").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("single p=2"), "{body}");
    let (code, body, _) = get(&client, &base, "/?s=hello").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("search s=hello"), "{body}");
    handle.abort();

    let cfg = cfg_for("woocommerce-minimal", 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("woocommerce-minimal ok"), "{body}");
    let (code, body, _) = get(&client, &base, "/?add_to_cart=101").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("cart=101"), "{body}");
    assert!(body.contains("total_cents=1500"), "{body}");
    let r = client
        .get(format!("{base}/?add_to_cart=202&checkout=1"))
        .send()
        .await
        .expect("checkout");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("checkout"), "{body}");
    handle.abort();
}

#[tokio::test]
async fn one_shot_gateway_spawns_fresh_process_per_request() {
    // P0-A gateway-level proof: PIDs via /pid.php must differ across
    // sequential requests (request-scoped lifetime, not persistent reuse).
    let cfg = cfg_for("vanilla-php", 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (_, b1, _) = get(&client, &base, "/pid.php").await;
    let (_, b2, _) = get(&client, &base, "/pid.php").await;
    assert!(b1.contains("pid="), "{b1}");
    assert!(b2.contains("pid="), "{b2}");
    assert_ne!(b1.trim(), b2.trim(), "one-shot gateway must use fresh PIDs");
    handle.abort();
}

#[tokio::test]
async fn metrics_endpoint_exposes_pool_counters() {
    let cfg = cfg_for("vanilla-php", 2);
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
    assert!(r.headers().contains_key("x-request-id"));
    let body = r.text().await.unwrap();
    for key in [
        "phpx_execution_mode_info",
        "phpx_pool_completed_total",
        "phpx_pool_failed_total",
        "phpx_pool_timeouts_total",
        "phpx_pool_rejected_overload_total",
        "phpx_pool_recycled_total",
        "phpx_pool_restarts_total",
        "phpx_pool_circuit_breaks_total",
        "phpx_pool_active",
        "phpx_pool_waiting",
        "phpx_pool_max_workers",
    ] {
        assert!(body.contains(key), "missing {key} in:\n{body}");
    }
    assert!(
        body.contains("execution_mode=\"one-shot\""),
        "metrics must distinguish execution mode:\n{body}"
    );
    handle.abort();
}
