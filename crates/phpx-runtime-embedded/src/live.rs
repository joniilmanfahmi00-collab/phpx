//! P2 promotion: live engine path (real libphp execution).
//!
//! Only linked builds (`PHPX_EMBED_LINK=1` on Linux NTS) reach the FFI below;
//! stub builds expose the same API and fail closed with an actionable error.
//! Every call goes domain-guarded (single entry) with an isolation scope, in
//! containment order. The engine is process-global: init once, sequential
//! evals share one executor until the reset proof lands (isolation HOLD).

use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};
#[cfg(phpx_embed_linked)]
use std::ffi::c_void;
#[cfg(phpx_embed_linked)]
use std::os::raw::{c_char, c_int, c_long};

/// Eval result: `status` 0 = ok, 1 = contained bailout/eval failure.
/// A bailout is an expected contained outcome, not a transport error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveOutput {
    pub status: i32,
    pub output: Vec<u8>,
    pub ret_long: Option<i64>,
}

#[cfg(phpx_embed_linked)]
mod ffi {
    use super::*;
    extern "C" {
        pub fn phpx_embed_startup() -> c_int;
        pub fn phpx_embed_eval(
            code: *const c_char,
            code_len: usize,
            want_retval: c_int,
            outp: *mut *mut c_char,
            outlenp: *mut usize,
            retlongp: *mut c_long,
            retislongp: *mut c_int,
        ) -> c_int;
        pub fn phpx_embed_free(p: *mut c_void);
    }
}

#[cfg(phpx_embed_linked)]
static ENGINE: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();

#[cfg(phpx_embed_linked)]
fn ensure_started() -> Result<()> {
    ENGINE
        .get_or_init(|| {
            let rc = unsafe { ffi::phpx_embed_startup() };
            if rc == 0 {
                Ok(())
            } else {
                Err(format!("php_embed_init failed (rc={rc})"))
            }
        })
        .clone()
        .map_err(PhpxError::Runtime)
}

#[cfg(phpx_embed_linked)]
static DOMAIN: std::sync::OnceLock<crate::EmbeddedDomain> = std::sync::OnceLock::new();

#[cfg(phpx_embed_linked)]
fn domain() -> &'static crate::EmbeddedDomain {
    DOMAIN.get_or_init(crate::EmbeddedDomain::new)
}

/// Execute PHP statements on the live engine (linked builds only).
/// Captures echo output. Statements go through with retval NULL, so pass
/// real statements (`echo 'x';`, assignments) — NOT bare expressions.
#[cfg(phpx_embed_linked)]
pub fn execute_once(code: &str) -> Result<LiveOutput> {
    eval_raw(code, false)
}

/// Evaluate a bare PHP expression on the live engine (linked builds only).
/// The engine wraps it as `return <code>;` itself, so pass `6*7` —
/// NOT `return 6*7;`. Long results come back in `ret_long`.
#[cfg(phpx_embed_linked)]
pub fn eval_expr(code: &str) -> Result<LiveOutput> {
    eval_raw(code, true)
}

#[cfg(phpx_embed_linked)]
fn eval_raw(code: &str, want_retval: bool) -> Result<LiveOutput> {
    const CODE_CAP: usize = 1024 * 1024;
    if code.is_empty() {
        return Err(PhpxError::Config(
            "live eval refuses empty code".to_string(),
        ));
    }
    if code.len() > CODE_CAP {
        return Err(PhpxError::Runtime(format!(
            "live eval over 1MiB cap ({} bytes)",
            code.len()
        )));
    }
    ensure_started()?;
    let _guard = domain().try_enter()?;
    let mut scope = crate::isolation::IsolationScope::open("live-eval");
    scope.track(code.len());
    let mut outp: *mut c_char = std::ptr::null_mut();
    let mut outlen: usize = 0;
    let mut retlong: c_long = 0;
    let mut retislong: c_int = 0;
    let status = unsafe {
        ffi::phpx_embed_eval(
            code.as_ptr() as *const c_char,
            code.len(),
            want_retval as c_int,
            &mut outp,
            &mut outlen,
            &mut retlong,
            &mut retislong,
        )
    };
    let output = if outp.is_null() {
        Vec::new()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(outp as *const u8, outlen).to_vec() };
        unsafe { ffi::phpx_embed_free(outp as *mut c_void) };
        bytes
    };
    scope.track(output.len());
    scope.release(code.len() + output.len());
    // Always clean by construction (tracked == released above).
    let _ = scope
        .seal()
        .map_err(|e| PhpxError::Runtime(format!("live eval scope leak: {e}")))?;
    Ok(LiveOutput {
        status,
        output,
        ret_long: if retislong != 0 {
            Some(retlong as i64)
        } else {
            None
        },
    })
}

/// Stub builds: same API, fail-closed without touching FFI.
#[cfg(not(phpx_embed_linked))]
pub fn execute_once(_code: &str) -> Result<LiveOutput> {
    Err(PhpxError::Runtime(format!(
        "live engine unavailable in this build (build_linked={}, M6 experimental stub). Build on Linux NTS with PHPX_EMBED_LINK=1.",
        crate::is_build_linked()
    )))
}

/// Stub builds: same API, fail-closed without touching FFI.
#[cfg(not(phpx_embed_linked))]
pub fn eval_expr(_code: &str) -> Result<LiveOutput> {
    Err(PhpxError::Runtime(format!(
        "live engine unavailable in this build (build_linked={}, M6 experimental stub). Build on Linux NTS with PHPX_EMBED_LINK=1.",
        crate::is_build_linked()
    )))
}
