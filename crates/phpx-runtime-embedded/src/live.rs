//! P2 promotion: live engine path (real libphp execution).
//!
//! Only linked builds (`PHPX_EMBED_LINK=1` on Linux NTS) reach the FFI below;
//! stub builds expose the same API and fail closed with an actionable error.
//!
//! Threading rule (proven the hard way): the NTS engine lives on exactly ONE
//! dedicated OS thread for the whole process. Executor globals are
//! process-global, so touching the engine from different threads corrupts
//! Zend stack tracking after a bailout. Every public entry point below
//! routes through the engine thread and blocks for the reply; the domain
//! guard additionally fails closed on any concurrent entry attempt.
//! This mirrors the production integration shape (engine thread, never the
//! gateway workers' threads).

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
        pub fn phpx_embed_next_request() -> c_int;
        pub fn phpx_embed_set_request(
            method: *const c_char,
            uri: *const c_char,
            query: *const c_char,
        ) -> c_int;
        pub fn phpx_embed_set_header(name: *const c_char, value: *const c_char) -> c_int;
        pub fn phpx_embed_set_body(bytes: *const c_char, len: usize) -> c_int;
        pub fn phpx_embed_define_helpers() -> c_int;
        pub fn phpx_embed_run_include(
            abs_path: *const c_char,
            outp: *mut *mut c_char,
            outlenp: *mut usize,
        ) -> c_int;
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
enum EngineOp {
    Eval {
        code: String,
        want_retval: bool,
    },
    NextRequest,
    Serve {
        entry: String,
        method: String,
        path: String,
        query: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
}

#[cfg(phpx_embed_linked)]
enum EngineOut {
    Eval(LiveOutput),
    NextRequest(std::result::Result<(), String>),
}

/// The single engine thread: owns init, every eval, and every request
/// cycle for the life of the process. Never touches the network, never
/// blocks except on its own inbox.
#[cfg(phpx_embed_linked)]
fn engine_thread(
) -> &'static std::sync::mpsc::Sender<(EngineOp, std::sync::mpsc::Sender<EngineOut>)> {
    static CH: std::sync::OnceLock<
        std::sync::mpsc::Sender<(EngineOp, std::sync::mpsc::Sender<EngineOut>)>,
    > = std::sync::OnceLock::new();
    CH.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(EngineOp, std::sync::mpsc::Sender<EngineOut>)>();
        std::thread::spawn(move || {
            // Boot once on this thread: the NTS executor belongs to it.
            let boot: Result<()> = (|| {
                let rc = unsafe { ffi::phpx_embed_startup() };
                if rc == 0 {
                    Ok(())
                } else {
                    Err(PhpxError::Runtime(format!(
                        "php_embed_init failed (rc={rc})"
                    )))
                }
            })();
            let boot_err = boot.err().map(|e| e.to_string());
            while let Ok((op, reply)) = rx.recv() {
                let out = match op {
                    EngineOp::Eval { code, want_retval } => {
                        if let Some(e) = boot_err.as_deref() {
                            EngineOut::Eval(LiveOutput {
                                status: 2,
                                output: format!("engine not started: {e}").into_bytes(),
                                ret_long: None,
                            })
                        } else {
                            EngineOut::Eval(eval_on_thread(&code, want_retval))
                        }
                    }
                    EngineOp::NextRequest => {
                        if let Some(e) = boot_err.as_deref() {
                            EngineOut::NextRequest(Err(format!("engine not started: {e}")))
                        } else {
                            let rc = unsafe { ffi::phpx_embed_next_request() };
                            if rc == 0 {
                                EngineOut::NextRequest(Ok(()))
                            } else {
                                EngineOut::NextRequest(Err(format!(
                                    "request cycle failed (rc={rc})"
                                )))
                            }
                        }
                    }
                    EngineOp::Serve {
                        entry,
                        method,
                        path,
                        query,
                        headers,
                        body,
                    } => {
                        if let Some(e) = boot_err.as_deref() {
                            EngineOut::Eval(LiveOutput {
                                status: 2,
                                output: format!("engine not started: {e}").into_bytes(),
                                ret_long: None,
                            })
                        } else {
                            EngineOut::Eval(serve_on_thread(
                                &entry, &method, &path, &query, &headers, &body,
                            ))
                        }
                    }
                };
                let _ = reply.send(out);
            }
        });
        tx
    })
}

/// Run one op on the engine thread and wait for the reply. Fails closed if
/// the engine thread is gone (fail-closed, never silent).
#[cfg(phpx_embed_linked)]
fn run_on_engine(op: EngineOp) -> EngineOut {
    let (tx, rx) = std::sync::mpsc::channel();
    match engine_thread().send((op, tx)) {
        Ok(()) => rx.recv().unwrap_or(EngineOut::Eval(LiveOutput {
            status: 2,
            output: b"engine thread hung up".to_vec(),
            ret_long: None,
        })),
        Err(_) => EngineOut::Eval(LiveOutput {
            status: 2,
            output: b"engine thread gone".to_vec(),
            ret_long: None,
        }),
    }
}

/// Single entry: only the engine thread ever evaluates; the guard fails
/// closed if the engine is ever re-entered (e.g. a future PHP callback
/// into Rust that calls back).
#[cfg(phpx_embed_linked)]
static DOMAIN: std::sync::OnceLock<crate::EmbeddedDomain> = std::sync::OnceLock::new();

#[cfg(phpx_embed_linked)]
fn eval_on_thread(code: &str, want_retval: bool) -> LiveOutput {
    const CODE_CAP: usize = 1024 * 1024;
    if code.is_empty() || code.len() > CODE_CAP {
        return LiveOutput {
            status: 2,
            output: b"live eval refuses empty or over-cap code".to_vec(),
            ret_long: None,
        };
    }
    // Single entry: only this thread ever evaluates, so this always holds;
    // it fails closed if the engine is ever re-entered (e.g. a future PHP
    // callback into Rust that calls back).
    let domain = DOMAIN.get_or_init(crate::EmbeddedDomain::new);
    let _guard = match domain.try_enter() {
        Ok(g) => g,
        Err(e) => {
            return LiveOutput {
                status: 2,
                output: e.to_string().into_bytes(),
                ret_long: None,
            };
        }
    };
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
    let _ = scope.seal();
    LiveOutput {
        status,
        output,
        ret_long: if retislong != 0 {
            Some(retlong as i64)
        } else {
            None
        },
    }
}

/// C-string helper: NUL bytes terminate the value (single-quoted PHP and
/// NUL-terminated C strings cannot carry them; over-long values are
/// refused by the caller).
#[cfg(phpx_embed_linked)]
fn cstr_capped(s: &str, cap: usize) -> Option<std::ffi::CString> {
    let t = s.split('\0').next().unwrap_or("");
    if t.len() > cap {
        return None;
    }
    std::ffi::CString::new(t).ok()
}

/// Serve one request on the engine thread: fresh request, SAPI state,
/// helpers, then the entry file. Every FFI step is checked; the first
/// failure aborts with status 2 (internal error, distinct from the
/// contained app-failure status 1).
#[cfg(phpx_embed_linked)]
#[allow(clippy::too_many_arguments)]
fn serve_on_thread(
    entry: &str,
    method: &str,
    path: &str,
    query: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> LiveOutput {
    let fail = |msg: &str| LiveOutput {
        status: 2,
        output: msg.as_bytes().to_vec(),
        ret_long: None,
    };
    if entry.is_empty() || entry.len() > 4096 || method.is_empty() || method.len() > 16 {
        return fail("serve refuses empty/over-cap entry or method");
    }
    if path.len() > 2048 || query.len() > 2048 || body.len() > 1024 * 1024 {
        return fail("serve refuses over-cap path/query/body");
    }
    // Fresh executor first: deterministic per-call state.
    if unsafe { ffi::phpx_embed_next_request() } != 0 {
        return fail("serve request cycle failed");
    }
    // Single entry: only this thread ever evaluates, so this always holds;
    // it fails closed if the engine is ever re-entered (e.g. a future PHP
    // callback into Rust that calls back).
    let domain = DOMAIN.get_or_init(crate::EmbeddedDomain::new);
    let _guard = match domain.try_enter() {
        Ok(g) => g,
        Err(e) => {
            return LiveOutput {
                status: 2,
                output: e.to_string().into_bytes(),
                ret_long: None,
            };
        }
    };
    // Request line.
    let uri = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    let (Some(m), Some(u), Some(q)) = (
        cstr_capped(method, 15),
        cstr_capped(&uri, 2047),
        cstr_capped(query, 2047),
    ) else {
        return fail("serve refuses NUL bytes in method/uri/query");
    };
    if unsafe { ffi::phpx_embed_set_request(m.as_ptr(), u.as_ptr(), q.as_ptr()) } != 0 {
        return fail("serve set_request failed");
    }
    // Headers: Cookie and Content-Type take dedicated handling inside the
    // shim (cookie parsing, remembered content type); the rest become HTTP_*.
    for (name, value) in headers {
        let Some(n) = cstr_capped(name, 63) else {
            return fail("serve refuses bad header name");
        };
        let Some(v) = cstr_capped(value, 511) else {
            return fail("serve refuses over-cap header value");
        };
        if unsafe { ffi::phpx_embed_set_header(n.as_ptr(), v.as_ptr()) } != 0 {
            return fail("serve set_header failed");
        }
    }
    // Raw body: binary-safe, plus CONTENT_LENGTH from the byte count.
    if !body.is_empty() {
        let rc = unsafe { ffi::phpx_embed_set_body(body.as_ptr().cast(), body.len()) };
        if rc != 0 {
            return fail("serve set_body failed");
        }
    }
    // phpx_input() polyfill for the current request.
    if unsafe { ffi::phpx_embed_define_helpers() } != 0 {
        return fail("serve helpers failed");
    }
    // Entry file with output capture.
    let Some(e) = cstr_capped(entry, 4095) else {
        return fail("serve refuses bad entry path");
    };
    let mut outp: *mut c_char = std::ptr::null_mut();
    let mut outlen: usize = 0;
    let status = unsafe { ffi::phpx_embed_run_include(e.as_ptr(), &mut outp, &mut outlen) };
    let output = if outp.is_null() {
        Vec::new()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(outp as *const u8, outlen).to_vec() };
        unsafe { ffi::phpx_embed_free(outp as *mut c_void) };
        bytes
    };
    LiveOutput {
        status,
        output,
        ret_long: None,
    }
}

/// Execute PHP statements on the live engine (linked builds only).
/// Captures echo output. Statements go through with retval NULL, so pass
/// real statements (`echo 'x';`, assignments) — NOT bare expressions.
#[cfg(phpx_embed_linked)]
pub fn execute_once(code: &str) -> Result<LiveOutput> {
    match run_on_engine(EngineOp::Eval {
        code: code.to_string(),
        want_retval: false,
    }) {
        EngineOut::Eval(out) if out.status == 2 && out.ret_long.is_none() => Err(
            PhpxError::Runtime(String::from_utf8_lossy(&out.output).to_string()),
        ),
        EngineOut::Eval(out) => Ok(out),
        EngineOut::NextRequest(_) => {
            Err(PhpxError::Runtime("live engine protocol error".to_string()))
        }
    }
}

/// Evaluate a bare PHP expression on the live engine (linked builds only).
/// The engine wraps it as `return <code>;` itself, so pass `6*7` —
/// NOT `return 6*7;`. Long results come back in `ret_long`.
#[cfg(phpx_embed_linked)]
pub fn eval_expr(code: &str) -> Result<LiveOutput> {
    match run_on_engine(EngineOp::Eval {
        code: code.to_string(),
        want_retval: true,
    }) {
        EngineOut::Eval(out) if out.status == 2 && out.ret_long.is_none() => Err(
            PhpxError::Runtime(String::from_utf8_lossy(&out.output).to_string()),
        ),
        EngineOut::Eval(out) => Ok(out),
        EngineOut::NextRequest(_) => {
            Err(PhpxError::Runtime("live engine protocol error".to_string()))
        }
    }
}

/// End the current request and start a fresh one on the live engine
/// (linked builds only). Each fresh request gets a new executor:
/// superglobals, symbol tables and user state do NOT cross the boundary —
/// this is the per-request isolation primitive.
#[cfg(phpx_embed_linked)]
pub fn next_request() -> Result<()> {
    match run_on_engine(EngineOp::NextRequest) {
        EngineOut::NextRequest(r) => r.map_err(PhpxError::Runtime),
        EngineOut::Eval(_) => Err(PhpxError::Runtime("live engine protocol error".to_string())),
    }
}

/// Serve one app request through the live engine (linked builds only):
/// fresh request, SAPI state (`$_GET`/`$_COOKIE`/`$_SERVER`/body), helpers,
/// then `include` of the entry file. Returns captured output; a contained
///PHP fatal (e.g. framework `exit`) reports `status: 1` with the output
/// produced so far — the host survives.
#[cfg(phpx_embed_linked)]
pub fn serve(
    entry_abs_path: &str,
    method: &str,
    path: &str,
    query: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<LiveOutput> {
    match run_on_engine(EngineOp::Serve {
        entry: entry_abs_path.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers: headers.to_vec(),
        body: body.to_vec(),
    }) {
        EngineOut::Eval(out) if out.status == 2 && out.ret_long.is_none() => Err(
            PhpxError::Runtime(String::from_utf8_lossy(&out.output).to_string()),
        ),
        EngineOut::Eval(out) => Ok(out),
        EngineOut::NextRequest(_) => {
            Err(PhpxError::Runtime("live engine protocol error".to_string()))
        }
    }
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

/// Stub builds: same API, fail-closed without touching FFI.
#[cfg(not(phpx_embed_linked))]
pub fn next_request() -> Result<()> {
    Err(PhpxError::Runtime(format!(
        "live engine unavailable in this build (build_linked={}, M6 experimental stub). Build on Linux NTS with PHPX_EMBED_LINK=1.",
        crate::is_build_linked()
    )))
}

/// Stub builds: same API, fail-closed without touching FFI.
#[cfg(not(phpx_embed_linked))]
pub fn serve(
    _entry_abs_path: &str,
    _method: &str,
    _path: &str,
    _query: &str,
    _headers: &[(String, String)],
    _body: &[u8],
) -> Result<LiveOutput> {
    Err(PhpxError::Runtime(format!(
        "live engine unavailable in this build (build_linked={}, M6 experimental stub). Build on Linux NTS with PHPX_EMBED_LINK=1.",
        crate::is_build_linked()
    )))
}
