use phpx_config::PhpxConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

// P0-D tahap 2: REAL upstream WordPress (wordpress.org zip, wp-cli install,
// unmodified source) served by PHPX's own gateway. Apache/nginx/php-fpm are
// never the server under test.
//
// Provisioning (needs running MySQL + wp-cli + network):
//   scripts/mysql-smoke.sh start   (local isolated mysqld; CI uses a service)
//   scripts/setup-wordpress-smoke.sh
// or set PHPX_WORDPRESS_DIR to a scripted install. The install URL must be
// http://127.0.0.1:18312 (fixed test gateway port below); the setup scripts
// enforce this via PHPX_WP_URL. Without a provisioned install the tests SKIP
// loudly (a DB server cannot be conjured inside a unit test); CI provisions,
// so the gate holds where it matters.

const GATEWAY_PORT: u16 = 18312;

fn wordpress_dir() -> Option<PathBuf> {
    let dir = if let Ok(d) = std::env::var("PHPX_WORDPRESS_DIR") {
        PathBuf::from(d)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/smoke/wordpress")
    };
    if dir.join("wp-config.php").is_file()
        && dir.join("wp-includes/version.php").is_file()
        && dir.join("index.php").is_file()
    {
        Some(dir)
    } else {
        None
    }
}

fn cfg_for_wordpress(root: &std::path::Path) -> PhpxConfig {
    let root = root.to_string_lossy().to_string();
    let toml = format!(
        "schema_version = 1\n[server]\nhost = \"127.0.0.1\"\nport = {GATEWAY_PORT}\ntls = false\nrequest_body_limit = \"1MB\"\n[application]\nroot = \'{root}\'\ndocument_root = \".\"\nentrypoint = \"index.php\"\n[runtime]\nmode = \"process\"\nphp_version = \"8.3\"\n[runtime.process]\nexecution_mode = \"one-shot\"\nworkers = 4\nmin_workers = 1\nmax_requests = 100\nrequest_timeout_seconds = 60\nstartup_timeout_seconds = 15\nmemory_limit = \"512M\"\nqueue_wait_timeout_seconds = 5\nshutdown_drain_timeout_seconds = 5\nrestart_max_retries = 3\nrestart_backoff_ms = 50\nrestart_backoff_max_ms = 500\n"
    );
    PhpxConfig::parse_str(&toml).expect("wordpress fixture config must parse")
}

fn shared_server() -> Option<SocketAddr> {
    // The gateway must NOT live on any single test's runtime: whichever test
    // wins the init race would take the server down at its own completion,
    // dropping the other tests' in-flight connections (that was the
    // intermittent IncompleteMessage). Instead the server runs on a dedicated
    // runtime parked in a background thread for the binary's lifetime —
    // mirroring production `serve()`, which never shares a test runtime.
    // Fixed port is required: WP's install URL (siteurl) must match the
    // gateway address or canonical redirects break the smoke.
    static SRV: OnceLock<Option<SocketAddr>> = OnceLock::new();
    let dir = wordpress_dir()?;
    *SRV.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("test gateway runtime");
            rt.block_on(async move {
                let cfg = cfg_for_wordpress(&dir);
                match phpx_server::spawn_test_server_on(cfg, GATEWAY_PORT).await {
                    Ok((addr, _handle)) => {
                        let _ = tx.send(Some(addr));
                        // Park forever: the runtime, listener and workers stay
                        // alive until the test binary exits. Intentionally
                        // never aborted (shared across tests in this file).
                        std::future::pending::<()>().await;
                    }
                    Err(e) => {
                        eprintln!(
                            "SKIP p0d_wordpress: gateway failed to bind :{GATEWAY_PORT}: {e}"
                        );
                        // Signal failure (e.g. port held by a leftover manual
                        // server): the tests skip instead of failing obscurely.
                        let _ = tx.send(None);
                    }
                }
            });
        });
        rx.recv_timeout(Duration::from_secs(60))
            .expect("test gateway bind timed out")
    })
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

fn require_wp() -> Option<String> {
    if wordpress_dir().is_none() {
        eprintln!(
            "SKIP p0d_wordpress: no provisioned install. Run scripts/setup-wordpress-smoke.sh \
             (after scripts/mysql-smoke.sh start) or set PHPX_WORDPRESS_DIR to a scripted install."
        );
        return None;
    }
    Some(format!("http://127.0.0.1:{GATEWAY_PORT}"))
}

fn assert_no_fatal(body: &str) {
    for marker in [
        "Fatal error",
        "Exception",
        "Allowed memory size",
        "Error establishing a database connection",
        "There has been a critical error",
    ] {
        assert!(
            !body.contains(marker),
            "WP failure marker {marker} in body head: {}",
            body.chars().take(300).collect::<String>()
        );
    }
}

// NOTE: multi_thread flavor mirrors production `serve()` (Tokio multi-thread
// runtime). The default current-thread test runtime starves under concurrent
// slow (multi-second) PHP boots + keep-alive reuse and drops connections.
#[tokio::test]
async fn wordpress_boots_and_serves_front_page() {
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, headers) = get(&client, &base, "/").await;
    assert_eq!(
        code,
        200,
        "front page must be 200, head: {}",
        body.chars().take(300).collect::<String>()
    );
    assert!(
        body.len() > 10_000,
        "front page suspiciously small: {}",
        body.len()
    );
    assert!(
        body.contains("<title>PHPX Smoke</title>"),
        "not the scripted install (title mismatch — use a setup-script install)"
    );
    assert!(body.contains("wp-content"), "missing theme asset URLs");
    assert!(headers.contains_key("x-request-id"));
    assert_no_fatal(&body);
    // Repeatability: second boot serves again (one-shot isolation).
    let (code, _, _) = get(&client, &base, "/").await;
    assert_eq!(code, 200);
}

#[tokio::test]
async fn wordpress_search_uses_query_loop() {
    // NOTE: `?p=N` shortlinks are intentionally NOT tested: with pretty
    // permalinks on, WP answers them with a canonical redirect
    // (`header("Location: ..."); exit`), whose header is swallowed by CLI
    // SAPI, arriving as an empty 200 (see sapi-audit.md). Search keeps the
    // query form and exercises `$_GET` + the loop without redirecting.
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, _) = get(&client, &base, "/?s=hello").await;
    assert_eq!(code, 200, "search must be 200");
    assert!(
        body.contains("Hello world!"),
        "search should find the stock post"
    );
    assert_no_fatal(&body);
}

#[tokio::test]
async fn wordpress_pretty_permalink_and_login() {
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    // Setup script sets /%postname%/; the fallback routes to index.php with
    // the original REQUEST_URI, which WP parses (no Apache rewrite needed).
    let (code, body, _) = get(&client, &base, "/hello-world/").await;
    assert_eq!(
        code,
        200,
        "pretty permalink must resolve, head: {}",
        body.chars().take(300).collect::<String>()
    );
    assert!(
        body.contains("Hello world!"),
        "pretty permalink content missing"
    );
    // Direct .php dispatch: login form renders (no auth performed).
    let (code, body, _) = get(&client, &base, "/wp-login.php").await;
    assert_eq!(code, 200, "login page must be 200");
    assert!(body.contains("Log In"), "login form missing");
}

#[tokio::test]
async fn wordpress_unknown_url_reports_not_found_body() {
    // CLI-SAPI limit (see sapi-audit.md): PHP-set status is not propagated,
    // so WP's 404 template arrives as HTTP 200 with the not-found body.
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, _) = get(&client, &base, "/this-page-does-not-exist-xyz/").await;
    assert!(
        body.to_ascii_lowercase().contains("not found"),
        "expected WP 404 template, got: {}",
        body.chars().take(300).collect::<String>()
    );
    assert_eq!(
        code, 200,
        "documents current CLI-SAPI status limit (WP 404 template -> gateway 200)"
    );
}

/// Read the deployment-side application password (`scripts/setup-wordpress-*`
/// writes it). None = the install was not provisioned by the script.
fn app_password(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(".phpx-app-password")).ok()?;
    let pw = text.trim().to_string();
    if pw.is_empty() {
        None
    } else {
        Some(pw)
    }
}

/// WooCommerce is an optional layer on the same install. Skip loudly when its
/// plugin file is absent (provision via scripts/setup-woocommerce-smoke.*).
fn woocommerce_active() -> Option<()> {
    let dir = wordpress_dir()?;
    if dir
        .join("wp-content/plugins/woocommerce/woocommerce.php")
        .is_file()
    {
        Some(())
    } else {
        eprintln!(
            "SKIP woocommerce tests: plugin not installed (run scripts/setup-woocommerce-smoke.*)"
        );
        None
    }
}

#[tokio::test]
async fn wordpress_rest_api_serves_json() {
    // P1-B integration: JSON API over the gateway (index, posts, search).
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, _) = get(&client, &base, "/wp-json/").await;
    assert_eq!(code, 200, "REST index must be 200");
    // Note: PHP json_encode escapes slashes, so the raw body holds `wp\/v2`.
    assert!(
        body.contains("wp\\/v2"),
        "missing namespaces: {}",
        body.chars().take(200).collect::<String>()
    );
    let (code, body, _) = get(&client, &base, "/wp-json/wp/v2/posts?per_page=5").await;
    assert_eq!(code, 200, "REST posts must be 200");
    assert!(
        body.contains("Hello world!"),
        "stock post missing from REST: {}",
        body.chars().take(200).collect::<String>()
    );
    assert!(body.contains("Hello PHPX"), "test post missing from REST");
    let (code, body, _) = get(&client, &base, "/wp-json/wp/v2/search?search=hello").await;
    assert_eq!(code, 200, "REST search must be 200");
    assert!(
        body.contains("Hello"),
        "search found nothing: {}",
        body.chars().take(200).collect::<String>()
    );
}

#[tokio::test]
async fn wordpress_login_error_and_success() {
    // P1-B integration: form login needs $_POST (urlencoded parsing in the
    // bootstrap) plus the test cookie (fixed value, sent manually since
    // Set-Cookie is swallowed by CLI SAPI). Wrong creds -> error text;
    // right creds -> redirect swallowed to an empty 200 (documented limit).
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let login = |user: &str, pass: &str| {
        let client = &client;
        let base = base.clone();
        let (user, pass) = (user.to_string(), pass.to_string());
        async move {
            let params = [
                ("log", user.as_str()),
                ("pwd", pass.as_str()),
                ("wp-submit", "Log In"),
                ("testcookie", "1"),
            ];
            let r = client
                .post(format!("{base}/wp-login.php"))
                .header("cookie", "wordpress_test_cookie=WP+Cookie+check")
                .form(&params)
                .send()
                .await
                .expect("POST login");
            let status = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            (status, body)
        }
    };
    let (code, body) = login("admin", "WRONG-pass").await;
    assert_eq!(code, 200);
    assert!(
        body.contains("incorrect"),
        "wrong creds must show the password error: {}",
        body.chars().take(300).collect::<String>()
    );
    let (code, body) = login("admin", "admin").await;
    assert_eq!(code, 200);
    assert!(
        !body.contains("incorrect") && body.len() < 1024,
        "successful login redirects (swallowed Location -> empty 200), got {} bytes",
        body.len()
    );
}

#[tokio::test]
async fn wordpress_comment_roundtrip() {
    // P1-B integration: POST a comment (urlencoded form -> $_POST) and read
    // it back via the REST API — a real DB write through the gateway.
    // Requires auto-approved comments (setup script sets comment_moderation /
    // whitelist / previously_approved to 0).
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let marker = "smoke-comment-phpx-int";
    let params = [
        ("comment_post_ID", "1"),
        ("author", "Phpx Integ"),
        ("email", "integ@phpx.test"),
        ("comment", marker),
        ("submit", "Post Comment"),
    ];
    let r = client
        .post(format!("{base}/wp-comments-post.php"))
        .form(&params)
        .send()
        .await
        .expect("POST comment");
    assert_eq!(r.status().as_u16(), 200);
    let (code, body, _) = get(
        &client,
        &base,
        "/wp-json/wp/v2/comments?post=1&per_page=100",
    )
    .await;
    assert_eq!(code, 200);
    assert!(
        body.contains(marker),
        "posted comment must be readable via REST (auto-approved?): {}",
        body.chars().take(300).collect::<String>()
    );
}

#[tokio::test]
async fn wordpress_media_upload_auth_chain() {
    // P1-B integration: application-password auth passes end to end
    // (Authorization: Basic -> PHP_AUTH_* via CGI parity), reaching the
    // upload handler — which then reports rest_upload_no_data because
    // php://input/$_FILES are unavailable under CLI SAPI (documented limit).
    // Without credentials the same endpoint reports rest_cannot_create.
    let Some(base) = require_wp() else { return };
    let Some(dir) = wordpress_dir() else { return };
    let Some(secret) = app_password(&dir) else {
        eprintln!("SKIP wordpress_media_upload_auth_chain: no .phpx-app-password (provision with scripts/setup-wordpress-smoke.*)");
        return;
    };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    // 1x1 transparent PNG.
    const PIXEL: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x60,
        0x18, 0x05, 0xA3, 0x60, 0x14, 0x8C, 0x02, 0x00, 0x08, 0x0F, 0x01, 0x9B, 0x71, 0x7B, 0x0B,
        0x87, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    // No credentials -> capability error (auth enforced).
    let r = client
        .post(format!("{base}/wp-json/wp/v2/media"))
        .header("content-type", "image/png")
        .header(
            "content-disposition",
            "attachment; filename=\"smoke-pixel.png\"",
        )
        .body(PIXEL.to_vec())
        .send()
        .await
        .expect("POST media unauthenticated");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap_or_default();
    assert!(
        body.contains("rest_cannot_create"),
        "unauthenticated upload must be refused with rest_cannot_create: {}",
        body.chars().take(200).collect::<String>()
    );
    // Valid application password -> auth passes; upload bytes cannot flow
    // (php://input unavailable under CLI SAPI), pinned as rest_upload_no_data.
    let r = client
        .post(format!("{base}/wp-json/wp/v2/media"))
        .basic_auth("admin", Some(secret))
        .header("content-type", "image/png")
        .header(
            "content-disposition",
            "attachment; filename=\"smoke-pixel.png\"",
        )
        .body(PIXEL.to_vec())
        .send()
        .await
        .expect("POST media authenticated");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap_or_default();
    assert!(
        !body.contains("rest_cannot_create") && !body.contains("rest_forbidden"),
        "valid app password must pass auth, got: {}",
        body.chars().take(200).collect::<String>()
    );
    assert!(
        body.contains("rest_upload_no_data"),
        "documents the php://input SAPI gap (auth passed, bytes cannot): {}",
        body.chars().take(200).collect::<String>()
    );
}

// --- P0-D item 5: WooCommerce (real upstream plugin, unmodified) ---
//
// The shop loop, product page, and REST catalog run through PHPX; cart and
// checkout flows hit documented CLI-SAPI walls (redirects swallowed because
// `Location` headers never propagate; sessions need cookies the gateway
// cannot round-trip). Each test pins the honest boundary.

#[tokio::test]
async fn woocommerce_shop_lists_product() {
    let Some(_) = woocommerce_active() else {
        return;
    };
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, _) = get(&client, &base, "/shop/").await;
    assert_eq!(code, 200, "shop must be 200");
    assert!(
        body.contains("Smoke Pixel"),
        "smoke product missing from shop"
    );
    assert!(body.contains("Add to cart"), "shop must offer add-to-cart");
}

#[tokio::test]
async fn woocommerce_product_page() {
    let Some(_) = woocommerce_active() else {
        return;
    };
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    // Pretty permalink for the scripted product (no Apache rewrite needed).
    let (code, body, _) = get(&client, &base, "/product/smoke-pixel/").await;
    assert_eq!(code, 200, "product page must be 200");
    assert!(body.contains("Smoke Pixel"), "product name missing");
    assert!(body.contains("25"), "product price missing");
    assert!(body.contains("Add to cart"), "product must be purchasable");
}

#[tokio::test]
async fn woocommerce_add_to_cart_redirect_swallowed() {
    // add-to-cart issues a redirect, whose Location is swallowed by CLI SAPI:
    // the gateway returns the fallback page (HTTP 200) instead of following
    // it. Pinned as the documented redirect limit (same class as `?p=N`).
    let Some(_) = woocommerce_active() else {
        return;
    };
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, _) = get(&client, &base, "/?add-to-cart=12").await;
    assert_eq!(code, 200);
    assert!(
        body.contains("PHPX Smoke"),
        "swallowed redirect must fall back to the front page: {}",
        body.chars().take(200).collect::<String>()
    );
    assert!(body.len() > 10_000, "unexpectedly small cart response");
}

#[tokio::test]
async fn woocommerce_checkout_empty_cart_redirects() {
    // With no session cookie the cart is empty, so checkout redirects away;
    // the swallowed redirect arrives as an empty 200 (documented limit).
    // (A filled checkout needs cookie round-trips, which CLI SAPI drops.)
    let Some(_) = woocommerce_active() else {
        return;
    };
    let Some(base) = require_wp() else { return };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let (code, body, _) = get(&client, &base, "/checkout/").await;
    assert_eq!(code, 200);
    assert!(
        body.trim().is_empty(),
        "empty-cart checkout should redirect-swallow to empty 200, got {} bytes",
        body.len()
    );
}

#[tokio::test]
async fn woocommerce_rest_api_lists_product() {
    // P0-D integration: WooCommerce REST catalog over the gateway, auth via
    // the same application-password mechanism as core REST.
    let Some(_) = woocommerce_active() else {
        return;
    };
    let Some(base) = require_wp() else { return };
    let Some(dir) = wordpress_dir() else { return };
    let Some(secret) = app_password(&dir) else {
        eprintln!("SKIP woocommerce_rest_api_lists_product: no .phpx-app-password (provisioned by setup-wordpress-smoke.*)");
        return;
    };
    let _ = shared_server();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .unwrap();
    let r = client
        .get(format!("{base}/wp-json/wc/v3/products?per_page=5"))
        .basic_auth("admin", Some(secret))
        .send()
        .await
        .expect("GET wc products");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap_or_default();
    assert!(
        body.contains("Smoke Pixel"),
        "catalog must include the scripted product: {}",
        body.chars().take(200).collect::<String>()
    );
    assert!(
        body.contains("\"price\":\"25"),
        "catalog must carry the scripted price: {}",
        body.chars().take(200).collect::<String>()
    );
    // Without credentials the catalog refuses with a JSON error, not data.
    let r = client
        .get(format!("{base}/wp-json/wc/v3/products?per_page=5"))
        .send()
        .await
        .expect("GET wc products unauthenticated");
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap_or_default();
    assert!(
        body.contains("woocommerce_rest_cannot_view"),
        "unauthenticated catalog must be refused: {}",
        body.chars().take(200).collect::<String>()
    );
}
