use phpx_config::PhpxConfig;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// P0-D tahap 1: REAL upstream Laravel (composer create-project, no source
// changes to the app) served by PHPX's own gateway (`phpx serve` equivalent:
// `spawn_test_server`). Apache/nginx/php-fpm are never the server under
// test; at most a differential-test reference (not implemented).
//
// Provisioning: `scripts/setup-laravel-smoke.sh` (.ps1 on Windows), or
// automatically below when missing (needs `composer` + network). Override
// the location with `PHPX_LARAVEL_DIR`.

fn laravel_dir() -> PathBuf {
    if let Ok(d) = std::env::var("PHPX_LARAVEL_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/smoke/laravel")
}

fn run(cmd: &str, args: &[&str], dir: &std::path::Path) -> Result<String, String> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COMPOSER_ALLOW_SUPERUSER", "1")
        .output()
        .map_err(|e| format!("spawn {cmd} failed: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!(
            "{cmd} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(500)
                .collect::<String>()
        ))
    }
}

/// Provision once per test binary (idempotent; skips when present).
fn ensure_laravel_app() -> PathBuf {
    static ONCE: OnceLock<PathBuf> = OnceLock::new();
    ONCE.get_or_init(|| {
        let dir = laravel_dir();
        let ready = dir.join("public/index.php").is_file()
            && dir.join("vendor/autoload.php").is_file()
            && dir.join(".env").is_file();
        if ready {
            return dir;
        }
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        run(
            "composer",
            &[
                "create-project",
                "laravel/laravel:^12.0",
                &dir.to_string_lossy(),
                "--prefer-dist",
                "--no-progress",
                "--no-interaction",
            ],
            &repo,
        )
        .unwrap_or_else(|e| {
            panic!(
                "real-Laravel provisioning failed ({e}). Pre-provision with \
                 scripts/setup-laravel-smoke.sh (or .ps1 on Windows), or set \
                 PHPX_LARAVEL_DIR to a composer-installed laravel/laravel dir."
            )
        });
        dir
    })
    .clone()
}

fn cfg_for_laravel(workers: u32) -> PhpxConfig {
    let root = ensure_laravel_app().to_string_lossy().to_string();
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = 8080\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \"public\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.3\"\n[runtime.process]\nexecution_mode = \"one-shot\"\nworkers = {workers}\nmin_workers = 1\nmax_requests = 100\nrequest_timeout_seconds = 60\nstartup_timeout_seconds = 15\nmemory_limit = \"256M\"\nqueue_wait_timeout_seconds = 5\nshutdown_drain_timeout_seconds = 5\nrestart_max_retries = 3\nrestart_backoff_ms = 50\nrestart_backoff_max_ms = 500\n"
    );
    PhpxConfig::parse_str(&toml).expect("laravel fixture config must parse")
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
async fn laravel_boots_and_serves_welcome() {
    let cfg = cfg_for_laravel(2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let t0 = Instant::now();
    let (code, body, headers) = get(&client, &base, "/").await;
    let elapsed = t0.elapsed();
    assert_eq!(
        code,
        200,
        "welcome must be 200, body head: {}",
        body.chars().take(300).collect::<String>()
    );
    assert!(
        body.len() > 50_000,
        "welcome page suspiciously small: {}",
        body.len()
    );
    assert!(
        body.contains("<title>Laravel</title>"),
        "not the Laravel welcome page"
    );
    assert!(headers.contains_key("x-request-id"));
    // No boot failure markers: a broken boot (missing key, bad env, fatal)
    // still renders *some* HTML, so assert the absence of failure text.
    for marker in [
        "Exception",
        "APP_KEY",
        "Allowed memory size",
        "Whoops",
        "Server Error",
    ] {
        assert!(
            !body.contains(marker),
            "boot failure marker {marker} in body"
        );
    }
    assert!(
        elapsed < Duration::from_secs(60),
        "one-shot boot too slow: {elapsed:?}"
    );
    // Repeatability: a second request boots fresh and serves again.
    let (code, body, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200);
    assert!(
        body.contains("<title>Laravel</title>"),
        "second boot differs"
    );
    handle.abort();
}

#[tokio::test]
async fn laravel_unknown_route_reports_not_found_body() {
    // CLI-SAPI limit (see sapi-audit.md): PHP-set status/headers are not
    // propagated by one-shot workers, so the framework's 404 page arrives
    // as HTTP 200 with the "Not Found" body. The test pins this contract:
    // body fidelity is guaranteed, status fidelity is a documented gap.
    let cfg = cfg_for_laravel(2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (code, body, _) = get(&client, &base, "/no-such-route-phpx-404").await;
    assert!(
        body.contains("Not Found"),
        "expected Laravel 404 body, got: {}",
        body.chars().take(300).collect::<String>()
    );
    assert_eq!(
        code, 200,
        "documents current CLI-SAPI status limit (framework 404 -> gateway 200)"
    );
    handle.abort();
}

/// Count rows in the app's sqlite `sessions` table via the php CLI
/// (proves session writes happened server-side; Set-Cookie itself is
/// swallowed by CLI SAPI and cannot be observed on the wire).
/// Retries briefly: under parallel test load a concurrent writer can hold
/// the sqlite lock (SQLITE_BUSY on the count query).
fn sqlite_session_count(dir: &std::path::Path) -> u64 {
    let db = dir
        .join("database/database.sqlite")
        .to_string_lossy()
        .to_string();
    let code = format!(
        "$db = new PDO(\"sqlite:{db}\"); echo $db->query(\"select count(*) from sessions\")->fetchColumn();"
    );
    for _ in 0..50 {
        let out = std::process::Command::new("php")
            .args(["-r", &code])
            .output()
            .expect("php must exist for session counting");
        if out.status.success() {
            if let Ok(n) = String::from_utf8_lossy(&out.stdout).trim().parse::<u64>() {
                return n;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("session count query kept failing (locked sqlite?)");
}

#[tokio::test]
async fn laravel_sessions_persist_server_side() {
    // P1-B integration: the session middleware runs on every web request
    // (database driver, migrated). Each GET persists a session row even
    // though Set-Cookie never reaches the client (CLI-SAPI limit).
    let dir = ensure_laravel_app();
    let before = sqlite_session_count(&dir);
    let cfg = cfg_for_laravel(2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let (code, _, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200);
    let (code, _, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200);
    let after = sqlite_session_count(&dir);
    assert!(
        after >= before + 2,
        "two page loads must persist two sessions (before={before}, after={after})"
    );
    handle.abort();
}

#[tokio::test]
async fn laravel_json_error_for_wrong_method() {
    // P1-B integration: JSON error contract. An API client sending POST to
    // a GET-only route gets the framework's JSON error body (405 inside),
    // while the gateway status stays 200 (documented CLI-SAPI limit).
    let cfg = cfg_for_laravel(2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{base}/"))
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .body("x=1")
        .send()
        .await
        .expect("POST");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap_or_default();
    assert!(
        body.contains("\"message\""),
        "expected JSON error body, got: {}",
        body.chars().take(200).collect::<String>()
    );
    assert!(body.contains("not supported"), "{body}");
    handle.abort();
}

#[tokio::test]
async fn laravel_multipart_is_rejected_gracefully() {
    // P1-B integration: multipart bodies are parsed by the SAPI shim
    // ($_POST/$_FILES populated, like the upload fixture pins), but this
    // route is GET-only so routing still answers 405 with the framework page
    // and the worker must not crash.
    let cfg = cfg_for_laravel(2);
    let (addr, handle) = phpx_server::spawn_test_server(cfg).await.expect("spawn");
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let boundary = "phpx-boundary-123";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nphpx\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"doc\"; filename=\"up.txt\"\r\nContent-Type: text/plain\r\n\r\nhello-upload\r\n--{boundary}--\r\n",
        boundary = boundary
    );
    let r = client
        .post(format!("{base}/"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .expect("POST multipart");
    assert_eq!(r.status().as_u16(), 200);
    let text = r.text().await.unwrap_or_default();
    assert!(
        text.contains("MethodNotAllowedHttpException"),
        "multipart must route to the 405 page, not crash: {}",
        text.chars().take(200).collect::<String>()
    );
    // Gateway still serves afterwards.
    let (code, _, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200);
    handle.abort();
}
