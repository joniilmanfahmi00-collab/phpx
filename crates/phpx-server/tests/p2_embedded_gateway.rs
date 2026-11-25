//! P2 embedded serve path: pooling + routing + explicit timeout through the
//! gateway. Mode-aware: linked builds serve vanilla live; stub builds fail
//! closed with the actionable message (gateway 500, never hang/panic).

use phpx_config::PhpxConfig;
use std::time::Duration;

fn embedded_config(timeout_s: u64) -> PhpxConfig {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php")
        .to_string_lossy()
        .to_string();
    let toml = format!(
        r#"
schema_version = 1
[server]
host = "127.0.0.1"
port = 8080
tls = false
request_body_limit = "1MB"
[application]
root = '{root}'
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "embedded"
php_version = "8.4"
[runtime.embedded]
enabled = true
request_timeout_seconds = {timeout_s}
queue_wait_timeout_seconds = 2
shutdown_drain_timeout_seconds = 5
"#
    );
    PhpxConfig::parse_str(&toml).expect("embedded test config must parse")
}

#[test]
fn build_state_selects_embedded_pool_with_explicit_timeouts() {
    let cfg = embedded_config(7);
    cfg.validate().expect("embedded config must validate");
    let state = phpx_server::build_state(&cfg).expect("build_state");
    assert_eq!(state.pool.execution_mode(), "embedded");
    let snap = state.pool.load_snapshot();
    assert_eq!(snap.active, 0);
    assert_eq!(snap.waiting, 0);
    assert_eq!(state.drain_timeout, Duration::from_secs(5));
    let text = state.pool.prometheus_text();
    assert!(text.contains("execution_mode=\"embedded\""), "{text}");
}

#[test]
fn rejects_embedded_without_enabled() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php")
        .to_string_lossy()
        .to_string();
    let toml = format!(
        r#"
schema_version = 1
[server]
host = "127.0.0.1"
port = 8080
tls = false
[application]
root = '{root}'
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "embedded"
php_version = "8.4"
"#
    );
    let e = PhpxConfig::parse_str(&toml).expect_err("must reject embedded without enabled");
    assert!(e.to_string().contains("enabled=true"), "{e}");
}

#[test]
fn rejects_bad_embedded_timeouts() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php")
        .to_string_lossy()
        .to_string();
    let toml = format!(
        r#"
schema_version = 1
[server]
host = "127.0.0.1"
port = 8080
tls = false
[application]
root = '{root}'
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "embedded"
php_version = "8.4"
[runtime.embedded]
enabled = true
request_timeout_seconds = 0
"#
    );
    let e = PhpxConfig::parse_str(&toml).expect_err("must reject timeout 0");
    assert!(e.to_string().contains("request_timeout_seconds"), "{e}");
}

#[tokio::test]
async fn gateway_serves_or_fails_closed_by_build() {
    let cfg = embedded_config(10);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let r = client.get(format!("{base}/")).send().await.expect("GET /");
    let status = r.status().as_u16();
    let body = r.text().await.unwrap_or_default();
    if phpx_runtime_embedded::is_build_linked() {
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("hello from phpx vanilla"), "{body}");
    } else {
        // Stub: fail-closed 500 with the actionable hint, bounded (no hang).
        assert_eq!(status, 500, "{body}");
        assert!(
            body.contains("live engine unavailable") || body.contains("php dispatch failed"),
            "{body}"
        );
    }
    handle.abort();
}

#[tokio::test]
async fn gateway_routing_static_and_direct_php() {
    let cfg = embedded_config(10);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    if phpx_runtime_embedded::is_build_linked() {
        let r = client
            .get(format!("{base}/style.css"))
            .send()
            .await
            .expect("GET css");
        assert_eq!(r.status().as_u16(), 200);
        let r = client
            .get(format!("{base}/hello.php"))
            .send()
            .await
            .expect("GET hello.php");
        assert_eq!(r.status().as_u16(), 200);
        let body = r.text().await.unwrap();
        assert!(body.contains("hello direct php"), "{body}");
    } else {
        // Stub still serves static files (no engine needed); only PHP fails.
        let r = client
            .get(format!("{base}/style.css"))
            .send()
            .await
            .expect("GET css");
        assert_eq!(r.status().as_u16(), 200);
    }
    handle.abort();
}
