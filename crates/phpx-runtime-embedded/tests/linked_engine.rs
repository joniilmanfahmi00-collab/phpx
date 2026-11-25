//! P2 promotion: live-engine evidence (linked builds only).
//!
//! Runs ONLY under `PHPX_EMBED_LINK=1` on Linux NTS (the build links libphp
//! plus the bailout-safe shim). Stub builds compile this file to zero tests.
//!
//! All engine interaction goes through `emb::live`, which pins the NTS
//! engine to one dedicated OS thread — so these tests are safe under
//! cargo's default parallel harness (the pre-pinning version tripped Zend
//! stack tracking when different tests touched the engine from different
//! threads after a bailout).

#![cfg(phpx_embed_linked)]

use phpx_runtime_embedded as emb;
use std::sync::{Mutex, OnceLock};

// Test-level serialization: the engine thread already serializes evals, but
// tests that assert executor state (leak vs clean) need ATOMICITY across
// their own eval sequences — another test's next_request() must not slip
// between this test's set and probe. One lock for the whole test body.
static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[test]
fn live_engine_boots() {
    let _t = lock_tests();
    let out = emb::live::execute_once("return 1;").expect("boot eval");
    // Statements go through with retval NULL: status proves execution.
    // Return-value capture is eval_expr's job (tested below).
    assert_eq!(out.status, 0, "engine must boot and run");
}

#[test]
fn live_eval_returns_long() {
    let _t = lock_tests();
    let out = emb::live::eval_expr("6*7").expect("eval");
    assert_eq!(out.status, 0);
    assert_eq!(out.ret_long, Some(42));
}

#[test]
fn live_eval_captures_echo() {
    let _t = lock_tests();
    let out = emb::live::execute_once("echo 'hello-live';").expect("eval");
    assert_eq!(out.status, 0);
    assert_eq!(out.output, b"hello-live");
}

#[test]
fn live_sequential_evals_share_executor_without_cycle() {
    let _t = lock_tests();
    // Shared-executor reality without a request cycle: state set in eval N
    // is visible in eval N+1 by design. Pins the mechanism the cycle fixes.
    let set = emb::live::execute_once("$GLOBALS['phpx_leak_probe'] = 123;").expect("set eval");
    assert_eq!(set.status, 0);
    let probe =
        emb::live::execute_once("echo isset($GLOBALS['phpx_leak_probe']) ? 'leak' : 'clean';")
            .expect("probe eval");
    assert_eq!(probe.status, 0);
    assert_eq!(
        probe.output, b"leak",
        "without a request cycle the executor is shared"
    );
    let _ = emb::live::execute_once("unset($GLOBALS['phpx_leak_probe']);");
    emb::live::next_request().expect("cycle for later tests");
}

#[test]
fn live_request_cycle_isolates_state() {
    let _t = lock_tests();
    // P2 isolation proof: set state, cycle the request, prove it is gone.
    // A fresh executor per request is the isolation primitive the gate needs.
    let set = emb::live::execute_once("$GLOBALS['phpx_iso_probe'] = 7;").expect("set eval");
    assert_eq!(set.status, 0);
    emb::live::next_request().expect("request cycle");
    let probe =
        emb::live::execute_once("echo isset($GLOBALS['phpx_iso_probe']) ? 'leak' : 'clean';")
            .expect("probe eval");
    assert_eq!(probe.status, 0);
    assert_eq!(
        probe.output, b"clean",
        "a request cycle must reset executor state"
    );
    let arith = emb::live::eval_expr("6*7").expect("eval after cycle");
    assert_eq!(arith.status, 0);
    assert_eq!(arith.ret_long, Some(42));
    emb::live::next_request().expect("cycle for later tests");
}

#[test]
fn live_vanilla_fixture_parity() {
    let _t = lock_tests();
    // P2 compat evidence (1/4 subsets): the real M4 vanilla fixture executes
    // through the engine via include and renders the documented body.
    // `header()` is a no-op under embed (headers_sent=1 at init); getenv falls
    // back to "direct", so the output is deterministic.
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php/public/index.php")
        .to_string_lossy()
        .to_string();
    assert!(
        std::path::Path::new(&root).is_file(),
        "vanilla fixture must exist at {root}"
    );
    emb::live::next_request().expect("fresh request for parity");
    let out = emb::live::execute_once(&format!("include {root:?};")).expect("fixture include");
    assert_eq!(out.status, 0, "fixture must execute cleanly");
    let body = String::from_utf8_lossy(&out.output);
    assert!(
        body.contains("hello from phpx vanilla"),
        "parity body missing: {body}"
    );
    assert!(
        body.contains("rid=direct"),
        "deterministic rid expected without PHPX_REQUEST_ID: {body}"
    );
    emb::live::next_request().expect("cycle for later tests");
}

#[test]
fn live_bailout_contained_host_survives() {
    let _t = lock_tests();
    // Containment proof: an uncaught exception longjmps out of the executor.
    // The shim catches it (status 1, host alive) instead of aborting the
    // process — then the engine must serve again on the next eval.
    let bad = emb::live::execute_once("throw new Exception('boom');").expect("eval runs");
    assert_eq!(bad.status, 1, "fatal must be contained, not crash");
    let ok = emb::live::eval_expr("7*6").expect("eval after bailout");
    assert_eq!(
        ok.status, 0,
        "engine must stay reusable after a contained fatal"
    );
    assert_eq!(ok.ret_long, Some(42));
    emb::live::next_request().expect("cycle for later tests");
}

#[test]
fn live_bailout_then_cycle_recovers_cleanly() {
    let _t = lock_tests();
    // Belt and suspenders: a request cycle after a contained fatal leaves a
    // provably clean executor behind.
    let bad = emb::live::execute_once("throw new Exception('b2');").expect("eval runs");
    assert_eq!(bad.status, 1);
    emb::live::next_request().expect("cycle after bailout");
    let probe = emb::live::execute_once("echo 'clean';").expect("probe eval");
    assert_eq!(probe.status, 0);
    assert_eq!(probe.output, b"clean");
}
