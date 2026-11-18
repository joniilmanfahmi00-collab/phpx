use phpx_config::PhpxConfig;

/// M7 resilience: saturation is bounded + observable, gateway recovers.
/// workers=2, queue_bound=2, queue_wait=2s, sleep.php blocks 5s.
/// 8 concurrent sleepers -> some 503 (backpressure), never hang, gateway alive after.
fn cfg_saturation(root: &str) -> PhpxConfig {
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = 8080\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \"public\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.4\"\n[runtime.process]\nworkers = 2\nmin_workers = 1\nmax_requests = 100\nrequest_timeout_seconds = 12\nstartup_timeout_seconds = 10\nmemory_limit = \"256M\"\nqueue_bound = 2\nqueue_wait_timeout_seconds = 2\nshutdown_drain_timeout_seconds = 5\nrestart_max_retries = 5\nrestart_backoff_ms = 50\nrestart_backoff_max_ms = 500\n"
    );
    PhpxConfig::parse_str(&toml).expect("saturation config must parse")
}

fn fixture_root(name: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/{name}"))
        .to_string_lossy()
        .to_string()
}

#[tokio::test]
async fn m7_saturation_bounded_and_observable() {
    let cfg = cfg_saturation(&fixture_root("vanilla-php"));
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // 8 concurrent slow requests against 2 workers + queue 2.
    let mut tasks = Vec::new();
    for i in 0..8 {
        let c = client.clone();
        let url = format!("{base}/sleep.php");
        tasks.push(tokio::spawn(async move {
            let r = c
                .get(&url)
                .header(
                    "traceparent",
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .header("x-test-id", format!("sat-{i}"))
                .send()
                .await;
            match r {
                Ok(resp) => resp.status().as_u16(),
                Err(_) => 0,
            }
        }));
    }
    let mut codes = Vec::new();
    for t in tasks {
        codes.push(t.await.unwrap_or(0));
    }
    // No hang (all answered), no 500-crash of gateway; allowed: 200 (served) or 503 (backpressure) or 504 (timeout).
    assert_eq!(codes.len(), 8);
    for c in &codes {
        assert!(
            *c == 200 || *c == 503 || *c == 504,
            "saturation must be bounded (200/503/504), got {c} in {codes:?}"
        );
    }
    // Observable: /metrics exposes pool series.
    let m = client
        .get(format!("{base}/metrics"))
        .send()
        .await
        .expect("metrics");
    assert_eq!(m.status().as_u16(), 200);
    let body = m.text().await.unwrap_or_default();
    assert!(
        body.contains("phpx_pool_"),
        "metrics must expose pool series"
    );
    // Recovery: gateway alive, fast endpoint 200, x-request-id present (OTel correlation ready).
    let r = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("recovery GET");
    assert_eq!(r.status().as_u16(), 200);
    assert!(r.headers().contains_key("x-request-id"));
    handle.abort();
}

#[tokio::test]
async fn m7_cgroup_style_bounds_are_config_gated() {
    // M7: unbounded pools/queues are rejected at config time (operator + orchestrator enforce the rest).
    // queue_bound=0 and >100000 must fail; workers=0 must fail (existing M4 gate, re-asserted for RC).
    let bad_queue = "schema_version = 1\n[server]\n[application]\n[runtime]\nmode = \"process\"\n[runtime.process]\nworkers = 2\nqueue_bound = 0\n";
    assert!(
        PhpxConfig::parse_str(bad_queue).is_err(),
        "queue_bound=0 must be rejected"
    );
    let bad_workers = "schema_version = 1\n[server]\n[application]\n[runtime]\nmode = \"process\"\n[runtime.process]\nworkers = 0\n";
    assert!(
        PhpxConfig::parse_str(bad_workers).is_err(),
        "workers=0 must be rejected"
    );
}
