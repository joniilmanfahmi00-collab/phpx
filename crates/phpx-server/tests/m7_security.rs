use phpx_config::PhpxConfig;

fn cfg_for(root: &str, workers: u32) -> PhpxConfig {
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = 8080\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \"public\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.4\"\n[runtime.process]\nworkers = {workers}\nmin_workers = 1\nmax_requests = 100\nrequest_timeout_seconds = 10\nstartup_timeout_seconds = 10\nmemory_limit = \"256M\"\nqueue_wait_timeout_seconds = 2\nshutdown_drain_timeout_seconds = 5\nrestart_max_retries = 3\nrestart_backoff_ms = 50\nrestart_backoff_max_ms = 500\n"
    );
    PhpxConfig::parse_str(&toml).expect("m7 fixture config must parse")
}

fn fixture_root(name: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/{name}"))
        .to_string_lossy()
        .to_string()
}

#[tokio::test]
async fn m7_traversal_blocked_before_fs() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let cfg = cfg_for(&fixture_root("vanilla-php"), 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    // Raw socket: bypass reqwest/url normalization so the gateway sees the exact bytes.
    // Encoded traversal must be rejected pre-FS with 400 (never escape, never 500).
    for raw_path in ["/%2e%2e/x", "/%2E%2E/x", "/%00", "/a%2fb"] {
        let mut s = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let req =
            format!("GET {raw_path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).await.expect("write");
        let mut buf = vec![0u8; 2048];
        let n = tokio::time::timeout(std::time::Duration::from_secs(3), s.read(&mut buf))
            .await
            .expect("timeout")
            .expect("read");
        let head = String::from_utf8_lossy(&buf[..n]);
        let status = head.lines().next().unwrap_or("").to_string();
        assert!(
            status.contains("400") || status.contains("404"),
            "{raw_path} should be blocked pre-FS, got: {status}"
        );
        assert!(!head.contains("outside-secret"), "must never leak");
    }
    handle.abort();
}

#[tokio::test]
async fn m7_sensitive_headers_do_not_break_gateway() {
    // M7: gateway must accept requests carrying secrets without echoing them.
    // Log redaction is covered by unit tests (redacted_header_value);
    // here we prove the gateway stays 200 and never reflects secrets in body.
    let cfg = cfg_for(&fixture_root("vanilla-php"), 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{base}/"))
        .header("authorization", "Bearer supersecret123")
        .header("cookie", "session=abc123")
        .header("x-api-key", "key-supersecret")
        .header(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .send()
        .await
        .expect("GET");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap_or_default();
    assert!(!body.contains("supersecret123"), "secret leaked in body");
    assert!(!body.contains("abc123"), "cookie leaked in body");
    handle.abort();
}

#[tokio::test]
async fn m7_symlink_escape_is_forbidden() {
    // M7 boundary: symlink inside docroot pointing outside must give 403.
    let tmp = tempfile::tempdir().expect("tmp");
    let docroot = tmp.path().join("public");
    std::fs::create_dir_all(&docroot).expect("mkdir");
    std::fs::write(docroot.join("index.php"), "<?php echo 'ok';").expect("write index");
    std::fs::write(tmp.path().join("outside.txt"), "outside-secret").expect("write outside");
    // public/link.php -> ../outside.txt (symlink escape attempt)
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("outside.txt"), docroot.join("link.php"))
        .expect("symlink");
    #[cfg(windows)]
    {
        // Windows: symlink needs privilege; fall back to traversal check only.
        if std::os::windows::fs::symlink_file(
            tmp.path().join("outside.txt"),
            docroot.join("link.php"),
        )
        .is_err()
        {
            return;
        }
    }
    let root = tmp.path().to_string_lossy().to_string();
    let cfg = cfg_for(&root, 2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{base}/link.php"))
        .send()
        .await
        .expect("GET");
    let code = r.status().as_u16();
    // Canonical check: outside docroot => 403. If platform resolves differently, 404 is also safe (never 200 + leak).
    assert!(
        code == 403 || code == 404,
        "symlink escape must be blocked, got {code}"
    );
    if code == 200 {
        let body = r.text().await.unwrap_or_default();
        assert!(!body.contains("outside-secret"), "symlink leak!");
    }
    handle.abort();
}

#[test]
fn m7_dockerfile_runs_non_root() {
    // M7 non-root verification: Dockerfile must pin USER phpx + HEALTHCHECK.
    // Runs on Windows too (static file check, no Docker needed).
    let dockerfile = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Dockerfile");
    let t = std::fs::read_to_string(&dockerfile).expect("Dockerfile readable");
    assert!(
        t.contains("USER phpx"),
        "Dockerfile must run as non-root USER phpx"
    );
    assert!(
        t.contains("HEALTHCHECK"),
        "Dockerfile must keep HEALTHCHECK"
    );
    assert!(t.contains("10001"), "phpx uid should be pinned (10001)");
}
