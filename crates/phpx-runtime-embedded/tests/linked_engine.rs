//! P2 promotion: live-engine evidence (linked builds only).
//!
//! Runs ONLY under `PHPX_EMBED_LINK=1` on Linux NTS (the build links libphp
//! plus the bailout-safe shim). Stub builds compile this file to zero tests.
//! Serialized with a process-global lock: the NTS engine is single-threaded,
//! and the domain guard would fail-closed on concurrent entry instead.

#![cfg(phpx_embed_linked)]

use phpx_runtime_embedded as emb;
use std::sync::Mutex;

static LIVE: Mutex<()> = Mutex::new(());

/// Lock the live engine, tolerating a poisoned mutex so one failing test
/// does not mask the others with cascade noise (the real error still fails).
fn lock_live() -> std::sync::MutexGuard<'static, ()> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn live_engine_boots() {
    let _l = lock_live();
    let out = emb::live::execute_once("return 1;").expect("boot eval");
    assert_eq!(out.status, 0, "engine must boot and run");
    assert_eq!(out.ret_long, Some(1));
}

#[test]
fn live_eval_returns_long() {
    let _l = lock_live();
    let out = emb::live::eval_expr("6*7").expect("eval");
    assert_eq!(out.status, 0);
    assert_eq!(out.ret_long, Some(42));
}

#[test]
fn live_eval_captures_echo() {
    let _l = lock_live();
    let out = emb::live::execute_once("echo 'hello-live';").expect("eval");
    assert_eq!(out.status, 0);
    assert_eq!(out.output, b"hello-live");
}

#[test]
fn live_sequential_evals_share_executor_today() {
    // Isolation reality check (gate HOLD): sequential evals share one
    // executor until the reset proof lands, so engine-global state leaks
    // across evals by design. This test pins the CURRENT reality; flipping
    // it to `clean` IS the isolation milestone.
    let _l = lock_live();
    let set = emb::live::execute_once("$GLOBALS['phpx_leak_probe'] = 123;").expect("set eval");
    assert_eq!(set.status, 0);
    let probe =
        emb::live::execute_once("echo isset($GLOBALS['phpx_leak_probe']) ? 'leak' : 'clean';")
            .expect("probe eval");
    assert_eq!(probe.status, 0);
    assert_eq!(
        probe.output, b"leak",
        "expected shared-executor leak today (isolation HOLD); if this ever reads clean, the isolation gate may be flippable"
    );
    // Hygiene for later evals in this process (best effort, same executor).
    let _ = emb::live::execute_once("unset($GLOBALS['phpx_leak_probe']);");
}
