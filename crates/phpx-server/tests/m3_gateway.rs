use phpx_config::PhpxConfig;

fn test_config() -> PhpxConfig {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php")
        .to_string_lossy()
        .to_string();
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = 8080\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \"public\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.4\"\n[runtime.process]\nworkers = 2\nmax_requests = 1000\nrequest_timeout_seconds = 10\nstartup_timeout_seconds = 10\nmemory_limit = \"256M\"\n"
    );
    PhpxConfig::parse_str(&toml).expect("test config must parse")
}

#[tokio::test]
async fn healthz_and_readyz_do_not_touch_php() {
    let cfg = test_config();
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{base}/healthz"))
        .send()
        .await
        .expect("GET healthz");
    assert_eq!(r.status().as_u16(), 200);
    assert!(r.headers().contains_key("x-request-id"));
    let body = r.text().await.unwrap();
    assert!(body.contains("ok"), "{body}");
    let r = client
        .get(format!("{base}/readyz"))
        .send()
        .await
        .expect("GET readyz");
    assert_eq!(r.status().as_u16(), 200);
    assert!(r.headers().contains_key("x-request-id"));
    let body = r.text().await.unwrap();
    assert!(body.contains("ready"), "{body}");
    handle.abort();
}
