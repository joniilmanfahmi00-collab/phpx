use phpx_config::PhpxConfig;
use std::time::Duration;

fn test_config(workers: u32, timeout_s: u64) -> PhpxConfig {
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
mode = "process"
php_version = "8.4"
[runtime.process]
workers = {workers}
max_requests = 1000
request_timeout_seconds = {timeout_s}
startup_timeout_seconds = 10
memory_limit = "256M"
"#
    );
    PhpxConfig::parse_str(&toml).expect("test config must parse")
}

#[tokio::test]
async fn serves_front_controller_and_static() {
    let cfg = test_config(4, 10);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let r = client.get(format!("{base}/")).send().await.expect("GET /");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("hello from phpx vanilla"), "{body}");

    let r = client
        .get(format!("{base}/style.css"))
        .send()
        .await
        .expect("GET css");
    assert_eq!(r.status().as_u16(), 200);
    let ct = r
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.contains("css"), "{ct}");
    let body = r.text().await.unwrap();
    assert!(body.contains("background"), "{body}");

    let r = client
        .get(format!("{base}/hello.php"))
        .send()
        .await
        .expect("GET hello.php");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("hello direct php"), "{body}");
    assert!(!body.contains("<?php"), "source leaked: {body}");

    handle.abort();
}

#[tokio::test]
async fn blocks_traversal_and_survives_crash() {
    let cfg = test_config(4, 10);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    for raw_path in [
        "/%2e%2e/%2e%2e/Cargo.toml",
        "/..%2fCargo.toml",
        "/%2e%2e/Cargo.toml",
    ] {
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("tcp");
        let head = format!("GET {raw_path} HTTP/1.1");
        let mut req_bytes = head.into_bytes();
        req_bytes.extend_from_slice(b"\r\nHost: x\r\nConnection: close\r\n\r\n");
        tokio::io::AsyncWriteExt::write_all(&mut stream, &req_bytes)
            .await
            .expect("write");
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut buf)
            .await
            .expect("read");
        let text = String::from_utf8_lossy(&buf);
        assert!(
            text.starts_with("HTTP/1.1 400"),
            "traversal {raw_path} must be 400, got: {}",
            text.lines().next().unwrap_or("")
        );
    }

    let r = client
        .get(format!("{base}/crash.php"))
        .send()
        .await
        .expect("crash");
    assert_eq!(r.status().as_u16(), 500);
    let r = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("after crash");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.contains("hello from phpx vanilla"), "{body}");

    handle.abort();
}

#[tokio::test]
async fn backpressure_returns_503_not_unbounded() {
    let cfg = test_config(1, 10);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let c = client.clone();
        let url = format!("{base}/sleep.php");
        tasks.push(tokio::spawn(async move {
            c.get(url).send().await.unwrap().status().as_u16()
        }));
    }
    let codes = tokio::time::timeout(Duration::from_secs(20), async {
        let mut out = Vec::new();
        for t in tasks {
            out.push(t.await.unwrap());
        }
        out
    })
    .await
    .expect("requests finish");
    let ok = codes.iter().filter(|c| **c == 200).count();
    let busy = codes.iter().filter(|c| **c == 503).count();
    assert!(
        busy >= 1,
        "expected at least one 503 backpressure, got {codes:?}"
    );
    assert!(ok >= 1, "expected at least one success, got {codes:?}");
    handle.abort();
}
