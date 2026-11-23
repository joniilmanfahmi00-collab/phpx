use phpx_core::{PhpxError, Result};
use phpx_runtime::{PhpRequest, PhpResponse, WorkerOutcome};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

/// PHP discovery + one-shot supervised execution (M1).
/// One request = one fresh `php` process (SPEC 4.2 allowed mode).
/// Guarantees no cross-request state by construction.
#[derive(Debug, Clone)]
pub struct PhpDiscovery {
    pub php_bin: String,
}

#[derive(Debug, Clone)]
pub struct PhpInfo {
    pub bin: String,
    pub version_line: String,
    pub version_major_minor: Option<(u32, u32)>,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    /// Wall-clock limit for one PHP execution.
    pub timeout: Duration,
    /// Extra env for the child (plus PHPX_* request vars).
    pub env: HashMap<String, String>,
    /// PHP-level heap limit passed as `-d memory_limit=...` (e.g. "256M").
    /// None = leave the php default. This bounds the Zend heap, NOT OS RSS
    /// (see P0-C guardian for OS-level soft/hard RSS policy).
    pub memory_limit: Option<String>,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            env: HashMap::new(),
            memory_limit: None,
        }
    }
}

/// Restart/backoff policy (M1 skeleton, refined in ADR-0014 2.6).
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    pub max_retries_after_crash: u32,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_retries_after_crash: 0,
            backoff_base: Duration::from_millis(250),
            backoff_max: Duration::from_secs(30),
        }
    }
}

impl PhpDiscovery {
    pub fn from_env_or_default() -> Self {
        let php_bin = std::env::var("PHPX_PHP_BIN").unwrap_or_else(|_| "php".to_string());
        Self { php_bin }
    }

    pub fn inspect(&self) -> Result<PhpInfo> {
        let out = Command::new(&self.php_bin)
            .arg("--version")
            .output()
            .map_err(|e| {
                PhpxError::Prerequisite(format!(
                    "cannot run {} --version: {e}. Install PHP 8.3+ or set PHPX_PHP_BIN",
                    self.php_bin
                ))
            })?;
        if !out.status.success() {
            return Err(PhpxError::Prerequisite(format!(
                "{} --version exited {}",
                self.php_bin, out.status
            )));
        }
        let version_line = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let version_major_minor = parse_php_version(&version_line);

        let ext_out = Command::new(&self.php_bin)
            .arg("-m")
            .output()
            .map_err(|e| PhpxError::Runtime(format!("php -m failed for {}: {e}", self.php_bin)))?;
        let mut extensions: Vec<String> = String::from_utf8_lossy(&ext_out.stdout)
            .lines()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty() && !s.starts_with('['))
            .collect();
        extensions.sort();
        extensions.dedup();

        Ok(PhpInfo {
            bin: self.php_bin.clone(),
            version_line,
            version_major_minor,
            extensions,
        })
    }

    pub fn require_version(&self, major: u32, minor: u32) -> Result<PhpInfo> {
        let info = self.inspect()?;
        match info.version_major_minor {
            Some((ma, mi)) if (ma, mi) >= (major, minor) => Ok(info),
            _ => Err(PhpxError::Prerequisite(format!(
                "PHP >= {major}.{minor} required, got: {}",
                info.version_line
            ))),
        }
    }
}

/// M4: materialize the embedded SAPI-compat bootstrap to a stable temp path.
/// Written once per process (content-addressed by length); best-effort only:
/// if the file cannot be written, fall back to no prepend (vanilla getenv still works).
fn ensure_bootstrap_file() -> PathBuf {
    const BOOTSTRAP: &str = include_str!("bootstrap.php");
    let mut path = std::env::temp_dir();
    path.push(format!("phpx-bootstrap-{}.php", BOOTSTRAP.len()));
    if !path.is_file() {
        let _ = std::fs::write(&path, BOOTSTRAP);
    }
    path
}

fn parse_php_version(line: &str) -> Option<(u32, u32)> {
    // e.g. "PHP 8.3.30 (cli) ..."
    let mut parts = line.split_whitespace();
    let _php = parts.next()?;
    let ver = parts.next()?;
    let mut nums = ver.split('.');
    let major: u32 = nums.next()?.parse().ok()?;
    let minor: u32 = nums.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

/// Execute one PHP request as one fresh process.
///
/// - Sets `PHPX_REQUEST_ID`, `PHPX_METHOD`, `PHPX_PATH`, `PHPX_QUERY`.
/// - Runs `php <entrypoint>` with cwd=app_root.
/// - Pipes request body to child stdin (when non-empty).
/// - Captures stdout as response body, stderr for diagnostics (never leaked as body).
/// - Enforces timeout; kills on expiry; crash never panics the supervisor.
pub fn execute_one_shot(
    php_bin: &str,
    req: &PhpRequest,
    opts: &ExecuteOptions,
) -> Result<PhpResponse> {
    req.validate()?;
    let started = Instant::now();
    let entry = Path::new(&req.app_root).join(&req.entrypoint);

    // M4 SAPI-compat: bridge headers/query/cookies into CLI workers via env +
    // auto_prepend bootstrap (see src/bootstrap.php, embedded at compile time).
    let headers_json = serde_json::to_string(&req.headers).unwrap_or_else(|_| "{}".to_string());
    let bootstrap_path = ensure_bootstrap_file();
    let mut cmd = Command::new(php_bin);
    // PHP-level heap limit (validated size string at config layer).
    // Bounds the Zend heap only; OS RSS is governed by the P0-C guardian.
    if let Some(limit) = opts
        .memory_limit
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        cmd.arg("-d").arg(format!("memory_limit={limit}"));
    }
    cmd.arg("-d")
        .arg(format!("auto_prepend_file={}", bootstrap_path.display()))
        .arg(&entry)
        .current_dir(&req.app_root)
        .env("PHPX_REQUEST_ID", &req.request_id)
        .env("PHPX_METHOD", &req.method)
        .env("PHPX_PATH", &req.path)
        .env("PHPX_QUERY", &req.query)
        .env("PHPX_HEADERS_JSON", &headers_json)
        .env("PHPX_PROTOCOL", req.protocol_version.to_string())
        .stdin(if req.body.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (k, v) in &opts.env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| {
        PhpxError::Runtime(format!(
            "failed to spawn php {}: {e}",
            PathBuf::from(php_bin).display()
        ))
    })?;

    // P0-D fix: drain stdout/stderr concurrently. Reading only after the
    // child exits deadlocks once output exceeds the OS pipe buffer (a real
    // Laravel page is ~80KB > 64KB): the child blocks on write while the
    // supervisor waits for exit.
    let stdout_thread = child.stdout.take().map(|mut out| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_thread = child.stderr.take().map(|mut err| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            buf
        })
    });

    if !req.body.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            // Best-effort body write; broken pipe means child exited early.
            let _ = stdin.write_all(&req.body);
        }
    }

    let join_pipes = || {
        let stdout = stdout_thread
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let stderr = stderr_thread
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        (stdout, stderr)
    };

    match child
        .wait_timeout(opts.timeout)
        .map_err(|e| PhpxError::Runtime(format!("wait_timeout failed: {e}")))?
    {
        Some(status) => {
            let duration_ms = started.elapsed().as_millis() as u64;
            let (stdout, stderr) = join_pipes();
            if status.success() {
                Ok(PhpResponse {
                    request_id: req.request_id.clone(),
                    status: 200,
                    headers: HashMap::new(),
                    body: stdout,
                    duration_ms,
                    outcome: WorkerOutcome::Ok,
                })
            } else {
                let code = status.code();
                let detail = String::from_utf8_lossy(&stderr).trim().to_string();
                tracing::warn!(
                    request_id = %req.request_id,
                    exit = ?code,
                    stderr = %detail.chars().take(500).collect::<String>(),
                    "php worker exited non-zero"
                );
                Ok(PhpResponse {
                    request_id: req.request_id.clone(),
                    status: 500,
                    headers: HashMap::new(),
                    body: format!("php worker failed (exit {code:?}): {detail}").into_bytes(),
                    duration_ms,
                    outcome: WorkerOutcome::Crashed { exit_code: code },
                })
            }
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            // Unblock the drain threads: pipes hit EOF once the child dies.
            let _ = join_pipes();
            let duration_ms = started.elapsed().as_millis() as u64;
            Ok(PhpResponse {
                request_id: req.request_id.clone(),
                status: 504,
                headers: HashMap::new(),
                body: b"php execution timeout".to_vec(),
                duration_ms,
                outcome: WorkerOutcome::Timeout,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_php_version_line() {
        assert_eq!(
            parse_php_version("PHP 8.3.30 (cli) (built: Jan 13 2026)"),
            Some((8, 3))
        );
        assert_eq!(parse_php_version("garbage"), None);
    }

    #[test]
    fn discovery_finds_php() {
        let d = PhpDiscovery::from_env_or_default();
        let info = d.inspect().expect("php must exist for M1");
        assert!(!info.version_line.is_empty());
        assert!(!info.extensions.is_empty());
    }
}
