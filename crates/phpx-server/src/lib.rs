use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use phpx_config::PhpxConfig;
use phpx_core::{PhpxError, Result};
use phpx_runtime::{PhpRequest, PhpResponse};
use phpx_worker::{
    BoundedPool, EmbeddedPool, EmbeddedPoolConfig, PersistentPool, PersistentPoolConfig, PoolConfig,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

/// Shared gateway state (M2 gateway, M4 hardening, P0-B supervised pool).
/// Lifecycle note (SPEC 5.1 + ADR-0014): one-shot pool has no persistent
/// Busy incarnations; persistent pool supervises fixed workers with
/// Starting -> Ready -> Busy -> Ready plus drain/restart/Failed paths.
/// Failed = terminal for that worker with a diagnostic reason + no further
/// auto-retry; Stopped = drained pool no longer accepting work.
/// Execution pool selected by `runtime.process.execution_mode` (P0-A explicit,
/// P0-B implemented): one-shot throttle or fixed-size supervised workers.
#[derive(Debug, Clone, Copy)]
pub struct PoolLoad {
    pub active: usize,
    pub waiting: usize,
}

#[allow(clippy::large_enum_variant)]
pub enum ExecPool {
    OneShot(BoundedPool),
    // Boxed: the supervised pool owns fixed worker slots + sampler state,
    // far larger than the one-shot throttle (which stays inline: it is the
    // default hot path and boxing it would add a pointless indirection).
    Persistent(Box<PersistentPool>),
    // Embedded single-flight pool (P2): one dedicated engine thread, bounded
    // queue + explicit timeouts. Small (config + counters) so kept inline.
    Embedded(EmbeddedPool),
}

impl ExecPool {
    pub fn execution_mode(&self) -> &'static str {
        match self {
            ExecPool::OneShot(_) => phpx_runtime::EXECUTION_MODE_ONE_SHOT,
            ExecPool::Persistent(p) => p.execution_mode(),
            ExecPool::Embedded(p) => p.execution_mode(),
        }
    }

    pub fn execute(&self, req: &PhpRequest) -> Result<PhpResponse> {
        match self {
            ExecPool::OneShot(p) => p.execute(req),
            ExecPool::Persistent(p) => p.execute(req),
            ExecPool::Embedded(p) => p.execute(req),
        }
    }

    pub fn prometheus_text(&self) -> String {
        match self {
            ExecPool::OneShot(p) => p.prometheus_text(),
            ExecPool::Persistent(p) => p.prometheus_text(),
            ExecPool::Embedded(p) => p.prometheus_text(),
        }
    }

    pub fn begin_drain(&self) {
        match self {
            ExecPool::OneShot(p) => p.begin_drain(),
            ExecPool::Persistent(p) => p.begin_drain(),
            ExecPool::Embedded(p) => p.begin_drain(),
        }
    }

    /// Load snapshot for access-log dimensions (active/waiting only).
    pub fn load_snapshot(&self) -> PoolLoad {
        let (active, waiting) = match self {
            ExecPool::OneShot(p) => {
                let s = p.snapshot();
                (s.active, s.waiting)
            }
            ExecPool::Persistent(p) => p.load(),
            ExecPool::Embedded(p) => p.load(),
        };
        PoolLoad { active, waiting }
    }

    pub fn drain(&self, timeout: Duration) -> bool {
        match self {
            ExecPool::OneShot(p) => p.drain(timeout),
            ExecPool::Persistent(p) => p.drain(timeout),
            ExecPool::Embedded(p) => p.drain(timeout),
        }
    }
}

pub struct AppState {
    pub docroot: PathBuf,
    pub docroot_canonical: PathBuf,
    pub entrypoint: String,
    pub pool: ExecPool,
    pub body_limit: u64,
    pub php_bin: String,
    pub drain_timeout: Duration,
}

pub type BoxBody = Full<Bytes>;

fn text_response(status: u16, request_id: &str, body: &str) -> Response<BoxBody> {
    let mut r = Response::new(Full::new(Bytes::from(body.as_bytes().to_vec())));
    *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    r.headers_mut().insert(
        "content-type",
        hyper::header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    if let Ok(v) = hyper::header::HeaderValue::from_str(request_id) {
        r.headers_mut().insert("x-request-id", v);
    }
    r
}

fn is_overload(err: &PhpxError) -> bool {
    let m = err.to_string();
    m.contains("overload") || m.contains("draining") || m.contains("circuit")
}

fn is_draining(err: &PhpxError) -> bool {
    err.to_string().contains("draining")
}

/// M7 security: sensitive headers must never appear in logs/diagnostics.
/// Full values are still passed to PHP via header_map (app needs them),
/// but access logs only record method/path/status/duration + request_id.
/// Use redacted_header_value() whenever a header value must be displayed.
pub fn is_sensitive_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "authorization"
            | "proxy-authorization"
            | "proxy-authenticate"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "api-key"
            | "x-auth-token"
            | "token"
            | "secret"
            | "password"
            | "x-access-token"
            | "x-secret"
    ) || n.contains("token")
        || n.contains("secret")
        || n.contains("password")
        || n.contains("api-key")
        || n.contains("auth")
        || n.contains("cookie")
}

/// M7: return "[redacted]" for sensitive headers, else value unchanged.
pub fn redacted_header_value(name: &str, value: &str) -> String {
    if is_sensitive_header(name) {
        "[redacted]".to_string()
    } else {
        value.to_string()
    }
}

/// M7 observability: W3C traceparent passthrough (OTel-ready, no extra dep).
/// Returns incoming traceparent header value when present.
/// header_map keys are already lowercased by handle_one.
pub fn extract_traceparent(headers: &std::collections::HashMap<String, String>) -> Option<String> {
    headers
        .get("traceparent")
        .cloned()
        .filter(|v| !v.is_empty() && v.len() <= 256)
}

/// Reject obvious traversal / encoded-traversal before touching the FS.
/// Returns 400 reason when rejected, None when path looks structurally ok.
fn reject_unsafe_path(raw_path: &str) -> Option<&str> {
    if raw_path.is_empty() || !raw_path.starts_with('/') {
        return Some("path must start with /");
    }
    if raw_path.contains('\0') || raw_path.contains('\\') {
        return Some("bad path");
    }
    let lower = raw_path.to_ascii_lowercase();
    if lower.contains("%2e")
        || lower.contains("%2f")
        || lower.contains("%5c")
        || lower.contains("%00")
    {
        return Some("encoded traversal");
    }
    for seg in raw_path.split('/') {
        if seg == ".." {
            return Some("path traversal");
        }
    }
    None
}

/// Join URI path onto docroot without escaping. Returns None when unsafe.
fn join_docroot(docroot: &std::path::Path, uri_path: &str) -> Option<PathBuf> {
    let trimmed = uri_path.strip_prefix('/').unwrap_or(uri_path);
    // Empty -> docroot itself.
    if trimmed.is_empty() {
        return Some(docroot.to_path_buf());
    }
    let mut out = docroot.to_path_buf();
    for seg in trimmed.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return None;
        }
        // Defensive: strip windows drive / absolute components.
        let p = std::path::Path::new(seg);
        let mut ok_seg = PathBuf::new();
        for c in p.components() {
            match c {
                Component::Normal(s) => ok_seg.push(s),
                _ => return None,
            }
        }
        if ok_seg.as_os_str().is_empty() {
            return None;
        }
        out.push(ok_seg);
    }
    Some(out)
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_ascii_lowercase().as_str() {
            "html" | "htm" => "text/html; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "js" | "mjs" => "text/javascript; charset=utf-8",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            "txt" => "text/plain; charset=utf-8",
            "map" => "application/json",
            _ => "application/octet-stream",
        }
    } else {
        "application/octet-stream"
    }
}

/// Normalize the configured PHP heap limit: "0"/empty = php default,
/// anything else is passed through as `-d memory_limit=...` (validated
/// as a parseable size at config load).
fn memory_limit_opt(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() || t == "0" {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn build_state(cfg: &PhpxConfig) -> Result<Arc<AppState>> {
    let app_root_raw = std::path::Path::new(&cfg.application.root);
    let app_root_abs = if app_root_raw.is_absolute() {
        app_root_raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| PhpxError::Config(format!("cannot resolve cwd: {e}")))?
            .join(app_root_raw)
    };
    let docroot = app_root_abs.join(&cfg.application.document_root);
    if !docroot.is_dir() {
        return Err(PhpxError::Config(format!(
            "document_root not found: {} (app root {}, document_root {})",
            docroot.display(),
            app_root_abs.display(),
            cfg.application.document_root
        )));
    }
    let docroot_canonical = std::fs::canonicalize(&docroot).map_err(|e| {
        PhpxError::Config(format!(
            "cannot canonicalize docroot {}: {e}",
            docroot.display()
        ))
    })?;
    // Front controller must exist for M2.
    let entry_abs = docroot.join(&cfg.application.entrypoint);
    if !entry_abs.is_file() {
        return Err(PhpxError::Config(format!(
            "entrypoint not found: {} (document_root {}, entrypoint {})",
            entry_abs.display(),
            docroot.display(),
            cfg.application.entrypoint
        )));
    }
    let php_bin = std::env::var("PHPX_PHP_BIN").unwrap_or_else(|_| "php".to_string());
    // P2 embedded serve path: runtime.mode selects the pool. Embedded is a
    // single-flight pool over the in-process engine (explicit timeouts from
    // [runtime.embedded]); process modes keep the existing worker pools.
    // Routing (direct .php / static / front-controller) is unchanged —
    // dispatch_php maps PhpRequest.app_root+entrypoint to the entry file
    // for either pool; only the executor differs.
    if cfg.runtime.mode.as_str() == "embedded" {
        let emb = cfg.runtime.embedded.clone().unwrap_or_default();
        if !emb.enabled {
            return Err(PhpxError::Config(
                "runtime.mode=embedded requires [runtime.embedded] enabled=true (experimental)"
                    .to_string(),
            ));
        }
        let pool = ExecPool::Embedded(EmbeddedPool::new(EmbeddedPoolConfig {
            execution_timeout: Duration::from_secs(emb.request_timeout_seconds.max(1)),
            queue_bound: emb.effective_queue_bound(),
            queue_wait_timeout: Duration::from_secs(emb.queue_wait_timeout_seconds),
            drain_timeout: Duration::from_secs(emb.shutdown_drain_timeout_seconds.max(1)),
        })?);
        let drain_timeout = Duration::from_secs(emb.shutdown_drain_timeout_seconds.max(1));
        return Ok(Arc::new(AppState {
            docroot,
            docroot_canonical,
            entrypoint: cfg.application.entrypoint.clone(),
            pool,
            body_limit: cfg.body_limit_bytes(),
            php_bin,
            drain_timeout,
        }));
    }
    let proc = cfg.runtime.process.clone().unwrap_or_default();
    let timeout_s = proc.request_timeout_seconds.max(1);
    let mode = phpx_runtime::ExecutionMode::parse(&proc.execution_mode).ok_or_else(|| {
        PhpxError::Config(format!(
            "runtime.process.execution_mode must be one-shot or persistent, got {}",
            proc.execution_mode
        ))
    })?;
    let pool = match mode {
        phpx_runtime::ExecutionMode::OneShot => ExecPool::OneShot(BoundedPool::new(PoolConfig {
            max_workers: proc.workers,
            min_workers: proc.min_workers,
            execution_mode: proc.execution_mode.clone(),
            queue_bound: proc.effective_queue_bound(),
            queue_wait_timeout: Duration::from_secs(proc.queue_wait_timeout_seconds),
            execution_timeout: Duration::from_secs(timeout_s),
            drain_timeout: Duration::from_secs(proc.shutdown_drain_timeout_seconds.max(1)),
            max_requests_per_slot: proc.max_requests.max(1),
            restart_max_retries: proc.restart_max_retries,
            restart_backoff_base: Duration::from_millis(proc.restart_backoff_ms.max(10)),
            restart_backoff_max: Duration::from_millis(
                proc.restart_backoff_max_ms
                    .max(proc.restart_backoff_ms.max(10)),
            ),
            php_bin: php_bin.clone(),
            php_memory_limit: memory_limit_opt(&proc.memory_limit),
        })?),
        phpx_runtime::ExecutionMode::Persistent => {
            ExecPool::Persistent(Box::new(PersistentPool::new(PersistentPoolConfig {
                worker_count: proc.workers,
                php_bin: php_bin.clone(),
                script_path: None,
                php_memory_limit: memory_limit_opt(&proc.memory_limit),
                startup_timeout: Duration::from_secs(proc.startup_timeout_seconds.max(1)),
                execution_timeout: Duration::from_secs(timeout_s),
                queue_bound: proc.effective_queue_bound(),
                queue_wait_timeout: Duration::from_secs(proc.queue_wait_timeout_seconds),
                drain_timeout: Duration::from_secs(proc.shutdown_drain_timeout_seconds.max(1)),
                max_requests_per_worker: proc.max_requests.max(1),
                restart_max_retries: proc.restart_max_retries,
                restart_backoff_base: Duration::from_millis(proc.restart_backoff_ms.max(10)),
                restart_backoff_max: Duration::from_millis(
                    proc.restart_backoff_max_ms
                        .max(proc.restart_backoff_ms.max(10)),
                ),
                memory_soft_bytes: phpx_config::parse_size(&proc.memory_soft_limit).unwrap_or(0),
                memory_hard_bytes: phpx_config::parse_size(&proc.memory_hard_limit).unwrap_or(0),
                memory_sample_interval: Duration::from_secs(
                    proc.memory_sample_interval_seconds.max(1),
                ),
                adaptive_enabled: proc.adaptive_enabled,
                adaptive_min_workers: proc.min_workers,
                adaptive_interval: Duration::from_secs(proc.adaptive_interval_seconds.max(1)),
                adaptive_cooldown: Duration::from_secs(proc.adaptive_cooldown_seconds.max(1)),
                adaptive_scale_up_queue_depth: proc.adaptive_scale_up_queue_depth.max(1),
                adaptive_scale_down_idle_intervals: proc.adaptive_scale_down_idle_intervals.max(1),
                adaptive_memory_guard: proc.adaptive_memory_guard,
            })?))
        }
    };
    let drain_timeout = Duration::from_secs(proc.shutdown_drain_timeout_seconds.max(1));
    Ok(Arc::new(AppState {
        docroot,
        docroot_canonical,
        entrypoint: cfg.application.entrypoint.clone(),
        pool,
        body_limit: cfg.body_limit_bytes(),
        php_bin,
        drain_timeout,
    }))
}

async fn read_body_limited(
    req: Request<Incoming>,
    limit: u64,
) -> std::result::Result<Vec<u8>, Box<Response<BoxBody>>> {
    // Fast path: content-length pre-check.
    if let Some(cl) = req.headers().get("content-length") {
        if let Ok(s) = cl.to_str() {
            if let Ok(n) = s.trim().parse::<u64>() {
                if n > limit {
                    let r = text_response(413, "unknown", "request body too large");
                    return Err(Box::new(r));
                }
            }
        }
    }
    let (parts, body) = req.into_parts();
    let req2 = Request::from_parts(parts, body);
    match req2.into_body().collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes();
            if bytes.len() as u64 > limit {
                Err(Box::new(text_response(
                    413,
                    "unknown",
                    "request body too large",
                )))
            } else {
                Ok(bytes.to_vec())
            }
        }
        Err(_) => Err(Box::new(text_response(
            400,
            "unknown",
            "cannot read request body",
        ))),
    }
}

async fn handle_one(state: Arc<AppState>, req: Request<Incoming>) -> Response<BoxBody> {
    let started = Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();
    let method = req.method().as_str().to_string();
    let uri_path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    // Copy headers for access log + PHP env (redact sensitive by default in logs only).
    let mut header_map: HashMap<String, String> = HashMap::new();
    for (k, v) in req.headers().iter() {
        if let Ok(s) = v.to_str() {
            header_map.insert(k.as_str().to_ascii_lowercase(), s.to_string());
        }
    }

    // M7 OTel-ready: W3C traceparent passthrough (IDs only, safe to log for correlation).
    let traceparent = extract_traceparent(&header_map).unwrap_or_default();
    let respond = |status: u16, body: String| -> Response<BoxBody> {
        let elapsed = started.elapsed().as_millis() as u64;
        tracing::info!(
            request_id = %request_id,
            method = %method,
            path = %uri_path,
            status = %status,
            duration_ms = %elapsed,
            traceparent = %traceparent,
            "access"
        );
        text_response(status, &request_id, &body)
    };

    if let Some(reason) = reject_unsafe_path(&uri_path) {
        return respond(400, format!("bad request: {reason}"));
    }

    // M3 health endpoints (no PHP, no body read; SPEC 11 basic health checks).
    // M4 adds /metrics (Prometheus text) below; health paths unchanged for compat.
    if uri_path == "/healthz" {
        return respond(200, "ok (phpx v0.1 developer preview)".to_string());
    }
    if uri_path == "/metrics" {
        let elapsed = started.elapsed().as_millis() as u64;
        let snap = state.pool.load_snapshot();
        let mut body = state.pool.prometheus_text();
        body.push_str(&format!(
            "# HELP phpx_build_info Build info.\n# TYPE phpx_build_info gauge\nphpx_build_info{{version=\"{}\"}} 1\n",
            env!("CARGO_PKG_VERSION")
        ));
        tracing::info!(
            request_id = %request_id,
            method = %method,
            path = %uri_path,
            status = 200u16,
            duration_ms = %elapsed,
            active = snap.active,
            waiting = snap.waiting,
            traceparent = %traceparent,
            "access"
        );
        let mut r = Response::new(Full::new(Bytes::from(body.into_bytes())));
        *r.status_mut() = StatusCode::OK;
        r.headers_mut().insert(
            "content-type",
            hyper::header::HeaderValue::from_static("text/plain; version=0.0.4"),
        );
        if let Ok(v) = hyper::header::HeaderValue::from_str(&request_id) {
            r.headers_mut().insert("x-request-id", v);
        }
        return r;
    }
    if uri_path == "/readyz" {
        let ready = state.docroot.is_dir() && state.docroot.join(&state.entrypoint).is_file();
        if ready {
            return respond(200, "ready".to_string());
        }
        return respond(500, "not ready: docroot or entrypoint missing".to_string());
    }

    // Body (bounded).
    let body_bytes = match read_body_limited(req, state.body_limit).await {
        Ok(b) => b,
        Err(r) => {
            let mut r = *r;
            if let Ok(v) = hyper::header::HeaderValue::from_str(&request_id) {
                r.headers_mut().insert("x-request-id", v);
            }
            let elapsed = started.elapsed().as_millis() as u64;
            tracing::info!(request_id=%request_id, method=%method, path=%uri_path, status=%r.status().as_u16(), duration_ms=%elapsed, traceparent=%traceparent, "access");
            return r;
        }
    };
    // Rebuild minimal header info (method/path/query already captured).
    // Routing.
    let candidate = match join_docroot(&state.docroot, &uri_path) {
        Some(p) => p,
        None => return respond(400, "bad request: invalid path".to_string()),
    };

    // Direct .php dispatch (never serve raw).
    let is_php = candidate
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("php"))
        .unwrap_or(false);
    if is_php {
        // Must exist inside docroot; canonical check blocks symlink escape.
        let canon = match std::fs::canonicalize(&candidate) {
            Ok(c) => c,
            Err(_) => return respond(404, "not found".to_string()),
        };
        if !canon.starts_with(&state.docroot_canonical) {
            return respond(403, "forbidden".to_string());
        }
        if !canon.is_file() {
            return respond(404, "not found".to_string());
        }
        let rel = match canon.strip_prefix(&state.docroot_canonical) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => return respond(403, "forbidden".to_string()),
        };
        let d = Dispatch {
            request_id: &request_id,
            method: &method,
            uri_path: &uri_path,
            query: &query,
            headers: header_map,
            body: body_bytes,
            entry_rel: &rel,
            started,
        };
        return dispatch_php(state, d).await;
    }

    // Static file when exact file exists.
    if candidate.is_file() {
        let canon = match std::fs::canonicalize(&candidate) {
            Ok(c) => c,
            Err(_) => return respond(404, "not found".to_string()),
        };
        if !canon.starts_with(&state.docroot_canonical) {
            return respond(403, "forbidden".to_string());
        }
        // Never serve .php raw even if it somehow got here.
        if canon
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("php"))
            .unwrap_or(false)
        {
            return respond(
                403,
                "forbidden: php source is never served statically".to_string(),
            );
        }
        // Only GET/HEAD for static in M2.
        if method != "GET" && method != "HEAD" {
            return respond(405, "method not allowed for static asset".to_string());
        }
        match tokio::fs::read(&canon).await {
            Ok(bytes) => {
                let elapsed = started.elapsed().as_millis() as u64;
                let ctype = content_type_for(&canon);
                let mut r = Response::new(Full::new(Bytes::from(bytes)));
                *r.status_mut() = StatusCode::OK;
                r.headers_mut().insert(
                    "content-type",
                    hyper::header::HeaderValue::from_static(ctype),
                );
                if let Ok(v) = hyper::header::HeaderValue::from_str(&request_id) {
                    r.headers_mut().insert("x-request-id", v);
                }
                if method == "HEAD" {
                    *r.body_mut() = Full::new(Bytes::new());
                }
                tracing::info!(request_id=%request_id, method=%method, path=%uri_path, status=200u16, duration_ms=%elapsed, traceparent=%traceparent, "access");
                return r;
            }
            Err(e) => return respond(500, format!("cannot read static file: {e}")),
        }
    }

    // Front controller fallback (vanilla + frameworks).
    // If URI maps to directory or missing file, dispatch entrypoint with PATH_INFO = uri_path.
    let entry = state.entrypoint.clone();
    let d = Dispatch {
        request_id: &request_id,
        method: &method,
        uri_path: &uri_path,
        query: &query,
        headers: header_map,
        body: body_bytes,
        entry_rel: &entry,
        started,
    };
    dispatch_php(state, d).await
}

struct Dispatch<'a> {
    request_id: &'a str,
    method: &'a str,
    uri_path: &'a str,
    query: &'a str,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    entry_rel: &'a str,
    started: Instant,
}

async fn dispatch_php(state: Arc<AppState>, d: Dispatch<'_>) -> Response<BoxBody> {
    let mut preq = PhpRequest::minimal(d.request_id, &state.docroot.to_string_lossy(), d.entry_rel);
    preq.method = d.method.to_string();
    preq.path = d.uri_path.to_string();
    preq.query = d.query.to_string();
    let dtp = extract_traceparent(&d.headers).unwrap_or_default();
    preq.headers = d.headers;
    preq.body = d.body;
    let rid = d.request_id.to_string();
    // Blocking PHP work must not run on Tokio executor threads (ADR-0006).
    let state2 = state.clone();
    let join = tokio::task::spawn_blocking(move || state2.pool.execute(&preq)).await;
    let elapsed = d.started.elapsed().as_millis() as u64;
    match join {
        Err(_) => {
            tracing::info!(request_id=%rid, method=%d.method, path=%d.uri_path, status=500u16, duration_ms=%elapsed, traceparent=%dtp, "access");
            text_response(500, &rid, "php join failed")
        }
        Ok(Err(e)) if is_overload(&e) => {
            let draining = is_draining(&e);
            tracing::info!(request_id=%rid, method=%d.method, path=%d.uri_path, status=503u16, duration_ms=%elapsed, overload=true, traceparent=%dtp, "access");
            let mut r = if draining {
                text_response(
                    503,
                    &rid,
                    "Service Unavailable: gateway is draining, retry shortly",
                )
            } else {
                text_response(
                    503,
                    &rid,
                    "Service Unavailable: worker pool saturated, retry shortly (backpressure)",
                )
            };
            r.headers_mut()
                .insert("retry-after", hyper::header::HeaderValue::from_static("1"));
            r
        }
        Ok(Err(e)) => {
            tracing::info!(request_id=%rid, method=%d.method, path=%d.uri_path, status=500u16, duration_ms=%elapsed, error=%e.to_string(), traceparent=%dtp, "access");
            text_response(500, &rid, &format!("php dispatch failed: {e}"))
        }
        Ok(Ok(presp)) => {
            let status = presp.status;
            tracing::info!(request_id=%rid, method=%d.method, path=%d.uri_path, status=%status, duration_ms=%elapsed, outcome=?presp.outcome, traceparent=%dtp, "access");
            let mut r = Response::new(Full::new(Bytes::from(presp.body)));
            *r.status_mut() =
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            for (k, v) in presp.headers {
                if let (Ok(hn), Ok(hv)) = (
                    hyper::header::HeaderName::from_bytes(k.as_bytes()),
                    hyper::header::HeaderValue::from_str(&v),
                ) {
                    r.headers_mut().insert(hn, hv);
                }
            }
            // Propagate PHP-set content type? M1 one-shot has none; default text/html for 200.
            if !r.headers().contains_key("content-type") && status == 200 {
                r.headers_mut().insert(
                    "content-type",
                    hyper::header::HeaderValue::from_static("text/html; charset=utf-8"),
                );
            }
            if let Ok(v) = hyper::header::HeaderValue::from_str(&rid) {
                r.headers_mut().insert("x-request-id", v);
            }
            r
        }
    }
}

async fn serve_conn(
    stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    state: Arc<AppState>,
) {
    let io = TokioIo::new(stream);
    let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
        let s = state.clone();
        async move { Ok::<_, std::convert::Infallible>(handle_one(s, req).await) }
    });
    let _ = ConnBuilder::new(TokioExecutor::new())
        .serve_connection(io, svc)
        .await;
}

fn load_tls(cert_file: &str, key_file: &str) -> Result<tokio_rustls::TlsAcceptor> {
    let cert_data = std::fs::read(cert_file)
        .map_err(|e| PhpxError::Config(format!("cannot read tls_cert_file {cert_file}: {e}")))?;
    let key_data = std::fs::read(key_file)
        .map_err(|e| PhpxError::Config(format!("cannot read tls_key_file {key_file}: {e}")))?;
    let mut cert_rd = &cert_data[..];
    let certs: Vec<_> = rustls_pemfile::certs(&mut cert_rd)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| PhpxError::Config(format!("invalid cert PEM {cert_file}: {e}")))?;
    if certs.is_empty() {
        return Err(PhpxError::Config(format!("no certificates in {cert_file}")));
    }
    let mut key_rd = &key_data[..];
    let key = rustls_pemfile::private_key(&mut key_rd)
        .map_err(|e| PhpxError::Config(format!("invalid key PEM {key_file}: {e}")))?
        .ok_or_else(|| PhpxError::Config(format!("no private key in {key_file}")))?;
    let mut cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| PhpxError::Config(format!("tls config failed: {e}")))?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(cfg)))
}

/// Run forever until Ctrl-C (used by `phpx serve`). HTTP/1.1 only in M2; HTTP/2 deferred.
pub async fn serve(cfg: PhpxConfig) -> Result<()> {
    cfg.validate()?;
    let state = build_state(&cfg)?;
    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .map_err(|e| PhpxError::Config(format!("invalid listen addr: {e}")))?;
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| PhpxError::Config(format!("cannot bind {addr}: {e}")))?;
    tracing::info!(%addr, tls = cfg.server.tls, docroot = %state.docroot.display(), entry = %state.entrypoint, "phpx gateway listening (M2, HTTP/1.1)");
    if cfg.server.tls {
        tracing::info!(
            "tls enabled via Rustls (HTTP/1.1 ALPN); HTTP/2 deferred to later milestone"
        );
    } else {
        tracing::info!("plain HTTP (tests); set server.tls=true with cert/key for TLS");
    }
    let tls_acceptor = if cfg.server.tls {
        Some(load_tls(
            cfg.server.tls_cert_file.as_deref().unwrap_or(""),
            cfg.server.tls_key_file.as_deref().unwrap_or(""),
        )?)
    } else {
        None
    };
    let drain_timeout = state.drain_timeout;
    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                tracing::info!("shutdown signal (Ctrl-C/SIGTERM) received, draining (no new connections, in-flight PHP bounded by execution timeout)");
                break;
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        let s = state.clone();
                        let tls = tls_acceptor.clone();
                        tokio::spawn(async move {
                            if let Some(acc) = tls {
                                match acc.accept(stream).await {
                                    Ok(tls_stream) => serve_conn(tls_stream, s).await,
                                    Err(e) => tracing::warn!(%peer, error=%e.to_string(), "tls handshake failed"),
                                }
                            } else {
                                serve_conn(stream, s).await
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error=%e.to_string(), "accept failed");
                    }
                }
            }
        }
    }
    // M4 graceful drain: stop pool intake, wait for in-flight up to drain_timeout.
    state.pool.begin_drain();
    tracing::info!(drain_secs = drain_timeout.as_secs(), "draining pool (Failed vs Stopped: failing requests get diagnostics, drained pool reports Stopped)");
    let drained = state.pool.drain(drain_timeout);
    if drained {
        tracing::info!("drain complete: all in-flight PHP finished");
    } else {
        tracing::warn!(
            "drain timeout elapsed: in-flight PHP bounded by execution timeout, exiting anyway"
        );
    }
    Ok(())
}

/// Wait for Ctrl-C everywhere; on unix also wait for SIGTERM (systemd stop).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(s) => s,
                Err(_) => {
                    let _ = tokio::signal::ctrl_c().await;
                    return;
                }
            };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Test helper: bind 127.0.0.1:0, spawn accept loop, return addr + shutdown handles.
/// Caller must keep `handle` alive; abort to stop. Plain HTTP only.
pub async fn spawn_test_server(
    cfg: PhpxConfig,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    spawn_test_server_on(cfg, 0).await
}

/// Test helper with a fixed port (e.g. WordPress, whose install URL must
/// match the gateway address for canonical redirects). Port 0 = ephemeral.
pub async fn spawn_test_server_on(
    cfg: PhpxConfig,
    port: u16,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let state = build_state(&cfg)?;
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| PhpxError::Config(format!("test bind failed: {e}")))?;
    let addr = listener
        .local_addr()
        .map_err(|e| PhpxError::Config(format!("local_addr failed: {e}")))?;
    let handle = tokio::spawn(async move {
        // NOTE: never `while let Ok(...)`: a single transient accept error
        // (common on Windows under rapid connect/close churn, e.g. WP
        // wp-cron self-hits) must not kill the listener. Production serve()
        // logs-and-continues; the test helper does the same.
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let s = state.clone();
                    tokio::spawn(async move { serve_conn(stream, s).await });
                }
                Err(_) => continue,
            }
        }
    });
    Ok((addr, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_segments() {
        assert!(reject_unsafe_path("/..").is_some());
        assert!(reject_unsafe_path("/a/../b").is_some());
        assert!(reject_unsafe_path("/%2e%2e/x").is_some());
        assert!(reject_unsafe_path("/ok/path").is_none());
    }

    #[test]
    fn joins_docroot_safely() {
        let root = std::path::Path::new("/tmp/app/public");
        assert!(join_docroot(root, "/..").is_none());
        assert!(join_docroot(root, "/a/b.css").is_some());
    }

    #[test]
    fn m7_redacts_sensitive_headers() {
        // Sensitive: never log raw values.
        for h in [
            "authorization",
            "Authorization",
            "proxy-authorization",
            "cookie",
            "Cookie",
            "set-cookie",
            "x-api-key",
            "X-Api-Key",
            "token",
            "x-access-token",
            "secret",
            "password",
            "X-Auth-Token",
        ] {
            assert!(is_sensitive_header(h), "{h} should be sensitive");
            assert_eq!(redacted_header_value(h, "supersecret123"), "[redacted]");
        }
        // Non-sensitive: passthrough.
        for h in [
            "content-type",
            "accept",
            "x-request-id",
            "traceparent",
            "user-agent",
        ] {
            assert!(!is_sensitive_header(h), "{h} should not be sensitive");
            assert_eq!(redacted_header_value(h, "plain"), "plain");
        }
    }

    #[test]
    fn m7_rejects_encoded_traversal_variants() {
        // M7 boundary: pre-FS reject list (defense in depth; canonical check is authoritative).
        for bad in [
            "/%2e%2e/x",
            "/%2E%2E/x",
            "/a%2fb",
            "/a%5cb",
            "/%00",
            "/a\\b",
            "/../secret",
            "/a/../../b",
        ] {
            assert!(
                reject_unsafe_path(bad).is_some(),
                "{bad} should be rejected"
            );
        }
        for ok in ["/", "/index.php", "/hello.php", "/style.css", "/api/hello"] {
            assert!(
                reject_unsafe_path(ok).is_none(),
                "{ok} should pass pre-check"
            );
        }
    }

    #[test]
    fn m7_traceparent_extraction() {
        let mut m = std::collections::HashMap::new();
        assert!(extract_traceparent(&m).is_none());
        m.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
        );
        assert!(extract_traceparent(&m).is_some());
    }
}
