//! P2 compat parity: the remaining three M4 sim fixtures executing through
//! the live engine (linked builds only). Each test is ONE atomic `serve`
//! call on the dedicated engine thread, plus a test-level mutex so
//! multi-eval tests never interleave with another test's request cycle.

#![cfg(phpx_embed_linked)]

use phpx_runtime_embedded as emb;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn fixture(app: &str) -> String {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/{app}/public/index.php"));
    assert!(p.is_file(), "sim fixture missing: {}", p.display());
    p.to_string_lossy().to_string()
}

fn serve(
    app: &str,
    method: &str,
    path: &str,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
    body: &[u8],
) -> emb::live::LiveOutput {
    let q = query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let h = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Vec<_>>();
    emb::live::serve(&fixture(app), method, path, &q, &h, body).expect("serve runs")
}

#[test]
fn parity_laravel_routing_and_json() {
    let _t = lock_tests();
    // Root route (no exit): status 0, fixture marker + method echo.
    let r = serve("laravel-minimal", "GET", "/", &[], &[], &[]);
    assert_eq!(r.status, 0, "root must serve cleanly");
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("laravel-minimal ok"), "{b}");
    assert!(b.contains("method=GET"), "{b}");

    // JSON API route ends in `exit` (framework style): the exit is
    // contained as status 1 with the rendered body intact.
    let r = serve(
        "laravel-minimal",
        "GET",
        "/api/hello",
        &[],
        &[("accept", "application/json")],
        &[],
    );
    assert_eq!(r.status, 1, "framework exit maps to contained status 1");
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("laravel-sim"), "{b}");
    assert!(b.contains("hello laravel"), "{b}");

    // Urlencoded form body round-trips through phpx_input + cookies in.
    let r = serve(
        "laravel-minimal",
        "POST",
        "/submit",
        &[],
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("cookie", "phpx_test=laravel-cookie"),
        ],
        b"pname=ara-form&other=1",
    );
    assert_eq!(r.status, 0, "form post must serve cleanly");
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("method=POST"), "{b}");
    assert!(b.contains("laravel-cookie"), "{b}");
    assert!(b.contains("pname=ara-form"), "{b}");
}

#[test]
fn parity_wordpress_query_and_search() {
    let _t = lock_tests();
    let r = serve("wordpress-minimal", "GET", "/", &[], &[], &[]);
    assert_eq!(r.status, 0);
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("wordpress-minimal ok"), "{b}");
    assert!(b.contains("Hello WP"), "{b}");

    let r = serve("wordpress-minimal", "GET", "/", &[("p", "2")], &[], &[]);
    assert_eq!(r.status, 0);
    assert!(
        String::from_utf8_lossy(&r.output).contains("single p=2"),
        "p=2 must route: {}",
        String::from_utf8_lossy(&r.output)
    );

    let r = serve(
        "wordpress-minimal",
        "GET",
        "/",
        &[("s", "hello")],
        &[("cookie", "phpx_test=wp-cookie")],
        &[],
    );
    assert_eq!(r.status, 0);
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("search s=hello"), "{b}");
    assert!(b.contains("cookie=wp-cookie"), "{b}");
}

#[test]
fn parity_woocommerce_cart_and_checkout() {
    let _t = lock_tests();
    let r = serve(
        "woocommerce-minimal",
        "GET",
        "/",
        &[("add_to_cart", "101")],
        &[],
        &[],
    );
    assert_eq!(r.status, 0);
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("cart=101"), "{b}");
    assert!(b.contains("total_cents=1500"), "{b}");

    // Checkout ends in `exit` (redirect-style flow): contained status 1
    // with the JSON body intact.
    let r = serve(
        "woocommerce-minimal",
        "GET",
        "/",
        &[("add_to_cart", "202"), ("checkout", "1")],
        &[],
        &[],
    );
    assert_eq!(r.status, 1, "checkout exit maps to contained status 1");
    let b = String::from_utf8_lossy(&r.output);
    assert!(b.contains("\"checkout\":\"ok\""), "{b}");
    assert!(b.contains("202"), "{b}");
}
