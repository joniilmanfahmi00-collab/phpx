//! P2 promotion: live-engine evidence (linked builds only).
//!
//! Runs ONLY under `PHPX_EMBED_LINK=1` on Linux NTS (the build links libphp
//! plus the bailout-safe shim). Stub builds compile this file to zero tests.
//!
//! The NTS engine is single-threaded by construction: executor globals are
//! process-global, so every eval in this file is routed through ONE dedicated
//! engine thread no matter how cargo schedules the tests. That mirrors how a
//! real host must drive the engine (and matches the single-entry domain).
//! A parallel run WITHOUT this pinning trips Zend stack tracking after a
//! bailout (spurious "Maximum call stack size" on other threads' stacks).

#![cfg(phpx_embed_linked)]

use phpx_runtime_embedded as emb;
use std::sync::{mpsc, Mutex, OnceLock};

enum EngineOp {
    ExecuteOnce(String),
    EvalExpr(String),
}

struct EngineOut {
    status: i32,
    output: Vec<u8>,
    ret_long: Option<i64>,
}

type EngineResult = Result<EngineOut, String>;

fn engine_thread() -> &'static mpsc::Sender<(EngineOp, mpsc::Sender<EngineResult>)> {
    static CH: OnceLock<mpsc::Sender<(EngineOp, mpsc::Sender<EngineResult>)>> = OnceLock::new();
    CH.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<(EngineOp, mpsc::Sender<EngineResult>)>();
        std::thread::spawn(move || {
            while let Ok((op, reply)) = rx.recv() {
                let r = match op {
                    EngineOp::ExecuteOnce(code) => emb::live::execute_once(&code),
                    EngineOp::EvalExpr(code) => emb::live::eval_expr(&code),
                };
                let out = r
                    .map(|o| EngineOut {
                        status: o.status,
                        output: o.output,
                        ret_long: o.ret_long,
                    })
                    .map_err(|e| e.to_string());
                let _ = reply.send(out);
            }
        });
        tx
    })
}

/// Run one engine op on the dedicated thread (all engine interaction in
/// this file goes through here so the NTS engine never hops OS threads).
fn live_call(op: EngineOp) -> EngineResult {
    static SERIALIZE: Mutex<()> = Mutex::new(());
    let _guard = SERIALIZE.lock().unwrap_or_else(|e| e.into_inner());
    let (tx, rx) = mpsc::channel();
    engine_thread().send((op, tx)).expect("engine thread alive");
    rx.recv().expect("engine reply")
}

#[test]
fn live_engine_boots() {
    let out = live_call(EngineOp::ExecuteOnce("return 1;".to_string())).expect("boot eval");
    // Statements go through with retval NULL: status proves execution.
    // Return-value capture is eval_expr's job (tested below).
    assert_eq!(out.status, 0, "engine must boot and run");
}

#[test]
fn live_eval_returns_long() {
    let out = live_call(EngineOp::EvalExpr("6*7".to_string())).expect("eval");
    assert_eq!(out.status, 0);
    assert_eq!(out.ret_long, Some(42));
}

#[test]
fn live_eval_captures_echo() {
    let out = live_call(EngineOp::ExecuteOnce("echo 'hello-live';".to_string())).expect("eval");
    assert_eq!(out.status, 0);
    assert_eq!(out.output, b"hello-live");
}

#[test]
fn live_sequential_evals_share_executor_today() {
    // Isolation reality check (gate HOLD): sequential evals share one
    // executor, so `$GLOBALS` set in eval N is visible in eval N+1 by design.
    // This test pins the CURRENT reality; flipping it to `clean` IS the
    // isolation milestone.
    let set = live_call(EngineOp::ExecuteOnce(
        "$GLOBALS['phpx_leak_probe'] = 123;".to_string(),
    ))
    .expect("set eval");
    assert_eq!(set.status, 0);
    let probe = live_call(EngineOp::ExecuteOnce(
        "echo isset($GLOBALS['phpx_leak_probe']) ? 'leak' : 'clean';".to_string(),
    ))
    .expect("probe eval");
    assert_eq!(probe.status, 0);
    assert_eq!(
        probe.output, b"leak",
        "expected shared-executor leak today (isolation HOLD); if this ever reads clean, the isolation gate may be flippable"
    );
    // Hygiene for later evals in this executor (best effort).
    let _ = live_call(EngineOp::ExecuteOnce(
        "unset($GLOBALS['phpx_leak_probe']);".to_string(),
    ));
}

#[test]
fn live_bailout_contained_host_survives() {
    // Containment proof: an uncaught exception longjmps out of the executor.
    // The shim catches it (status 1, host alive) instead of aborting the
    // process — then the engine must serve again on the next eval.
    let bad = live_call(EngineOp::ExecuteOnce(
        "throw new Exception('boom');".to_string(),
    ))
    .expect("eval runs");
    assert_eq!(bad.status, 1, "fatal must be contained, not crash");
    let ok = live_call(EngineOp::EvalExpr("7*6".to_string())).expect("eval after bailout");
    assert_eq!(
        ok.status, 0,
        "engine must stay reusable after a contained fatal"
    );
    assert_eq!(ok.ret_long, Some(42));
}
