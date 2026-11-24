use phpx_core::{PhpxError, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Schema version for M0. Must be 1.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level PHPX configuration (schema v1, M2 gateway, M4 hardening).
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct PhpxConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub application: ApplicationConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub queue: Option<QueueConfig>,
    #[serde(default)]
    pub scheduler: Option<SchedulerConfig>,
    #[serde(default)]
    pub cache: Option<CacheConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    #[serde(default)]
    pub observability: Option<ObservabilityConfig>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub tls_cert_file: Option<String>,
    #[serde(default)]
    pub tls_key_file: Option<String>,
    #[serde(default = "default_body_limit")]
    pub request_body_limit: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ApplicationConfig {
    #[serde(default = "default_root")]
    pub root: String,
    #[serde(default = "default_docroot")]
    pub document_root: String,
    #[serde(default = "default_entry")]
    pub entrypoint: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_php_version")]
    pub php_version: String,
    #[serde(default)]
    pub process: Option<RuntimeProcessConfig>,
    #[serde(default)]
    pub embedded: Option<RuntimeEmbeddedConfig>,
}

/// M4: min/max pool, bounded queue, timeouts, drain, restart policy.
/// All new fields have serde defaults so M1-M3 configs keep parsing.
/// P0-A: `execution_mode` is explicit. Only `"one-shot"` is runnable;
/// `"persistent"` is rejected at validation until the P0-B supervised pool exists.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RuntimeProcessConfig {
    #[serde(default = "default_workers")]
    pub workers: u32,
    #[serde(default = "default_min_workers")]
    pub min_workers: u32,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    #[serde(default = "default_max_requests")]
    pub max_requests: u64,
    #[serde(default = "default_timeout")]
    pub request_timeout_seconds: u64,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_seconds: u64,
    #[serde(default = "default_memory_limit")]
    pub memory_limit: String,
    /// P0-C guardian: OS-RSS soft limit ("0" = disabled). At/over -> graceful recycle.
    #[serde(default = "default_memory_disabled")]
    pub memory_soft_limit: String,
    /// P0-C guardian: OS-RSS hard limit ("0" = disabled). At/over -> protective termination.
    #[serde(default = "default_memory_disabled")]
    pub memory_hard_limit: String,
    /// P0-C guardian: idle-worker sampling period, 1-60s.
    #[serde(default = "default_memory_sample_interval")]
    pub memory_sample_interval_seconds: u64,
    /// None = auto (workers*4, min 16). Some(n) = explicit bound 1-100000.
    #[serde(default)]
    pub queue_bound: Option<usize>,
    #[serde(default = "default_queue_wait")]
    pub queue_wait_timeout_seconds: u64,
    #[serde(default = "default_drain_timeout")]
    pub shutdown_drain_timeout_seconds: u64,
    #[serde(default = "default_restart_retries")]
    pub restart_max_retries: u32,
    #[serde(default = "default_backoff_ms")]
    pub restart_backoff_ms: u64,
    #[serde(default = "default_backoff_max_ms")]
    pub restart_backoff_max_ms: u64,
    /// P2 adaptive: disabled by default (fixed pool). When true, the pool
    /// starts at `min_workers`, grows on sustained pressure up to `workers`
    /// (ceiling), and shrinks back when idle.
    #[serde(default)]
    pub adaptive_enabled: bool,
    /// P2 adaptive control-loop period, 1-60s.
    #[serde(default = "default_adaptive_interval")]
    pub adaptive_interval_seconds: u64,
    /// P2 adaptive quiet period between scale actions, 1-300s (anti-flap).
    #[serde(default = "default_adaptive_cooldown")]
    pub adaptive_cooldown_seconds: u64,
    /// P2 adaptive scale-up trigger: queued requests at/over this while every
    /// spawned worker is busy, 1-10000.
    #[serde(default = "default_adaptive_up_depth")]
    pub adaptive_scale_up_queue_depth: usize,
    /// P2 adaptive scale-down trigger: consecutive all-idle evaluations, 1-360.
    #[serde(default = "default_adaptive_down_intervals")]
    pub adaptive_scale_down_idle_intervals: u32,
    /// P2 adaptive host safeguard: refuse scale-up under guardian memory
    /// pressure (scale-down still allowed).
    #[serde(default = "default_true")]
    pub adaptive_memory_guard: bool,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RuntimeEmbeddedConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct QueueConfig {
    #[serde(default = "default_queue_driver")]
    pub driver: String,
    pub database: Option<String>,
    /// Redis server URL when driver=redis (e.g. redis://127.0.0.1:6379).
    /// `PHPX_REDIS_URL` env also works (CLI + open_queue fallback).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_queue_workers")]
    pub workers: u32,
}

/// Scheduler section (M5 phase 2). Schedules live in `[[scheduler.schedules]]`.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SchedulerConfig {
    #[serde(default)]
    pub enabled: bool,
    /// History DB for duplicate-dispatch protection (e.g. `./data/scheduler.db`).
    /// None/empty = ephemeral memory history (dedup only within one process).
    #[serde(default)]
    pub history_file: Option<String>,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub schedules: Vec<ScheduleConfig>,
}

/// One recurring dispatch (M5 phase 2): cron + timezone + queue payload.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ScheduleConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub cron: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Explicit missed-run policy (SPEC 7.2): `skip` or `dispatch-now`.
    #[serde(default)]
    pub missed_policy: String,
    #[serde(default = "default_schedule_retries")]
    pub max_retries: u32,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_driver")]
    pub driver: String,
    pub max_memory: Option<String>,
    /// Filesystem dir when driver=filesystem (e.g. ./data/cache).
    #[serde(default)]
    pub dir: Option<String>,
    /// Redis server URL when driver=redis (e.g. redis://127.0.0.1:6379).
    /// `PHPX_REDIS_URL` env also works (CLI + open_cache fallback).
    #[serde(default)]
    pub url: Option<String>,
    /// Max entries (1-100000). None = 1000.
    #[serde(default)]
    pub max_entries: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_driver")]
    pub driver: String,
    /// Local root when driver=local (e.g. ./data/storage).
    #[serde(default)]
    pub root: Option<String>,
    /// S3 prototype fields (driver=s3).
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_true")]
    pub json_logs: bool,
    #[serde(default = "default_true")]
    pub metrics: bool,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_body_limit() -> String {
    "16MB".to_string()
}
fn default_root() -> String {
    ".".to_string()
}
fn default_docroot() -> String {
    "public".to_string()
}
fn default_entry() -> String {
    "index.php".to_string()
}
fn default_mode() -> String {
    "process".to_string()
}
fn default_php_version() -> String {
    "8.4".to_string()
}
fn default_workers() -> u32 {
    4
}
fn default_min_workers() -> u32 {
    1
}
fn default_execution_mode() -> String {
    phpx_runtime::EXECUTION_MODE_ONE_SHOT.to_string()
}
fn default_max_requests() -> u64 {
    1000
}
fn default_timeout() -> u64 {
    30
}
fn default_startup_timeout() -> u64 {
    10
}
fn default_memory_limit() -> String {
    "256M".to_string()
}
fn default_memory_disabled() -> String {
    "0".to_string()
}
fn default_memory_sample_interval() -> u64 {
    5
}
fn default_queue_wait() -> u64 {
    2
}
fn default_drain_timeout() -> u64 {
    10
}
fn default_restart_retries() -> u32 {
    5
}
fn default_backoff_ms() -> u64 {
    250
}
fn default_backoff_max_ms() -> u64 {
    30000
}
fn default_adaptive_interval() -> u64 {
    5
}
fn default_adaptive_cooldown() -> u64 {
    30
}
fn default_adaptive_up_depth() -> usize {
    2
}
fn default_adaptive_down_intervals() -> u32 {
    3
}
fn default_queue_driver() -> String {
    "sqlite".to_string()
}
fn default_queue_workers() -> u32 {
    2
}
fn default_cache_driver() -> String {
    "memory".to_string()
}
fn default_storage_driver() -> String {
    "local".to_string()
}
fn default_poll_interval() -> u64 {
    30
}
fn default_schedule_retries() -> u32 {
    3
}
fn default_true() -> bool {
    true
}

/// Parse human sizes like "16MB", "256M", "1GB", "512KB", "1024".
pub fn parse_size(s: &str) -> Result<u64> {
    let t = s.trim().to_uppercase().replace(' ', "");
    if t.is_empty() {
        return Err(PhpxError::Config("size must not be empty".to_string()));
    }
    let (num_part, mult): (&str, u64) = if let Some(v) = t.strip_suffix("GB") {
        (v, 1024 * 1024 * 1024)
    } else if let Some(v) = t.strip_suffix("MB") {
        (v, 1024 * 1024)
    } else if let Some(v) = t.strip_suffix("KB") {
        (v, 1024)
    } else if let Some(v) = t.strip_suffix('G') {
        (v, 1024 * 1024 * 1024)
    } else if let Some(v) = t.strip_suffix('M') {
        (v, 1024 * 1024)
    } else if let Some(v) = t.strip_suffix('K') {
        (v, 1024)
    } else if let Some(v) = t.strip_suffix('B') {
        (v, 1)
    } else {
        (t.as_str(), 1)
    };
    let n: u64 = num_part.parse().map_err(|_| {
        PhpxError::Config(format!(
            "invalid size '{}': expected number + optional KB/MB/GB",
            s
        ))
    })?;
    n.checked_mul(mult)
        .ok_or_else(|| PhpxError::Config(format!("size overflow: '{}'", s)))
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            tls: false,
            tls_cert_file: None,
            tls_key_file: None,
            request_body_limit: default_body_limit(),
        }
    }
}

impl Default for ApplicationConfig {
    fn default() -> Self {
        Self {
            root: default_root(),
            document_root: default_docroot(),
            entrypoint: default_entry(),
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            php_version: default_php_version(),
            process: None,
            embedded: None,
        }
    }
}

impl Default for RuntimeProcessConfig {
    fn default() -> Self {
        Self {
            workers: default_workers(),
            min_workers: default_min_workers(),
            execution_mode: default_execution_mode(),
            memory_soft_limit: default_memory_disabled(),
            memory_hard_limit: default_memory_disabled(),
            memory_sample_interval_seconds: default_memory_sample_interval(),
            max_requests: default_max_requests(),
            request_timeout_seconds: default_timeout(),
            startup_timeout_seconds: default_startup_timeout(),
            memory_limit: default_memory_limit(),
            queue_bound: None,
            queue_wait_timeout_seconds: default_queue_wait(),
            shutdown_drain_timeout_seconds: default_drain_timeout(),
            restart_max_retries: default_restart_retries(),
            restart_backoff_ms: default_backoff_ms(),
            restart_backoff_max_ms: default_backoff_max_ms(),
            adaptive_enabled: false,
            adaptive_interval_seconds: default_adaptive_interval(),
            adaptive_cooldown_seconds: default_adaptive_cooldown(),
            adaptive_scale_up_queue_depth: default_adaptive_up_depth(),
            adaptive_scale_down_idle_intervals: default_adaptive_down_intervals(),
            adaptive_memory_guard: true,
        }
    }
}

impl RuntimeProcessConfig {
    /// Effective queue bound: explicit or auto (workers*4, min 16).
    pub fn effective_queue_bound(&self) -> usize {
        match self.queue_bound {
            Some(n) => n,
            None => ((self.workers as usize) * 4).max(16),
        }
    }
}

impl PhpxConfig {
    pub fn parse_str(s: &str) -> Result<Self> {
        let cfg: Self = toml::from_str(s)
            .map_err(|e| PhpxError::Config(format!(r#"TOML parse failed: {e}"#)))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            PhpxError::Config(format!(r#"cannot read config {}: {e}"#, path.display()))
        })?;
        Self::parse_str(&text)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(PhpxError::Config(format!(
                r#"unsupported schema_version {} (expected {})"#,
                self.schema_version, SCHEMA_VERSION
            )));
        }
        if self.server.port == 0 {
            return Err(PhpxError::Config("server.port must be 1-65535".to_string()));
        }
        if self.tls_enabled() {
            match (&self.server.tls_cert_file, &self.server.tls_key_file) {
                (Some(c), Some(k)) if !c.trim().is_empty() && !k.trim().is_empty() => {}
                _ => {
                    return Err(PhpxError::Config(
                        "tls=true requires [server] tls_cert_file and tls_key_file (PEM). Set tls=false for plain HTTP (tests)".to_string(),
                    ))
                }
            }
        }
        parse_size(&self.server.request_body_limit)?;
        if self.application.root.trim().is_empty() {
            return Err(PhpxError::Config(
                "application.root must not be empty".to_string(),
            ));
        }
        if self.application.entrypoint.trim().is_empty() {
            return Err(PhpxError::Config(
                "application.entrypoint must not be empty".to_string(),
            ));
        }
        let mode = self.runtime.mode.as_str();
        if mode != "process" && mode != "embedded" {
            return Err(PhpxError::Config(format!(
                r#"runtime.mode must be process or embedded, got {mode}"#
            )));
        }
        if mode == "embedded" {
            let enabled = self
                .runtime
                .embedded
                .as_ref()
                .map(|e| e.enabled)
                .unwrap_or(false);
            if !enabled {
                return Err(PhpxError::Config(
                    "runtime.mode=embedded requires [runtime.embedded] enabled=true (experimental)"
                        .to_string(),
                ));
            }
        }
        if let Some(p) = &self.runtime.process {
            match phpx_runtime::ExecutionMode::parse(&p.execution_mode) {
                Some(phpx_runtime::ExecutionMode::OneShot) => {}
                Some(phpx_runtime::ExecutionMode::Persistent) => {
                    // P0-B: persistent supervised pool implemented; selected
                    // explicitly by the operator (fixed-size, handshake-gated).
                }
                None => {
                    return Err(PhpxError::Config(format!(
                        "runtime.process.execution_mode must be one-shot or persistent, got {}",
                        p.execution_mode
                    )));
                }
            }
            if p.workers == 0 || p.workers > 1024 {
                return Err(PhpxError::Config(
                    "runtime.process.workers must be 1-1024 (bounded, no unbounded pool)"
                        .to_string(),
                ));
            }
            if p.min_workers == 0 || p.min_workers > 1024 {
                return Err(PhpxError::Config(
                    "runtime.process.min_workers must be 1-1024".to_string(),
                ));
            }
            if p.min_workers > p.workers {
                return Err(PhpxError::Config(format!(
                    "runtime.process.min_workers ({}) must be <= workers ({})",
                    p.min_workers, p.workers
                )));
            }
            if p.max_requests == 0 {
                return Err(PhpxError::Config(
                    "runtime.process.max_requests must be >= 1 (recycle budget)".to_string(),
                ));
            }
            if p.request_timeout_seconds == 0 || p.request_timeout_seconds > 600 {
                return Err(PhpxError::Config(
                    "runtime.process.request_timeout_seconds must be 1-600".to_string(),
                ));
            }
            if p.startup_timeout_seconds == 0 || p.startup_timeout_seconds > 120 {
                return Err(PhpxError::Config(
                    "runtime.process.startup_timeout_seconds must be 1-120".to_string(),
                ));
            }
            let qb = p.effective_queue_bound();
            if qb == 0 || qb > 100_000 {
                return Err(PhpxError::Config(
                    "runtime.process.queue_bound must be 1-100000 (bounded, backpressure mandatory)".to_string(),
                ));
            }
            if p.queue_wait_timeout_seconds > 120 {
                return Err(PhpxError::Config(
                    "runtime.process.queue_wait_timeout_seconds must be 0-120".to_string(),
                ));
            }
            if p.shutdown_drain_timeout_seconds == 0 || p.shutdown_drain_timeout_seconds > 120 {
                return Err(PhpxError::Config(
                    "runtime.process.shutdown_drain_timeout_seconds must be 1-120".to_string(),
                ));
            }
            if p.restart_max_retries > 100 {
                return Err(PhpxError::Config(
                    "runtime.process.restart_max_retries must be 0-100".to_string(),
                ));
            }
            if p.restart_backoff_ms < 10 || p.restart_backoff_ms > 60_000 {
                return Err(PhpxError::Config(
                    "runtime.process.restart_backoff_ms must be 10-60000".to_string(),
                ));
            }
            if p.restart_backoff_max_ms < p.restart_backoff_ms || p.restart_backoff_max_ms > 300_000
            {
                return Err(PhpxError::Config(
                    "runtime.process.restart_backoff_max_ms must be >= restart_backoff_ms and <= 300000".to_string(),
                ));
            }
            if p.adaptive_interval_seconds == 0 || p.adaptive_interval_seconds > 60 {
                return Err(PhpxError::Config(
                    "runtime.process.adaptive_interval_seconds must be 1-60".to_string(),
                ));
            }
            if p.adaptive_cooldown_seconds == 0 || p.adaptive_cooldown_seconds > 300 {
                return Err(PhpxError::Config(
                    "runtime.process.adaptive_cooldown_seconds must be 1-300".to_string(),
                ));
            }
            if p.adaptive_scale_up_queue_depth == 0 || p.adaptive_scale_up_queue_depth > 10_000 {
                return Err(PhpxError::Config(
                    "runtime.process.adaptive_scale_up_queue_depth must be 1-10000".to_string(),
                ));
            }
            if p.adaptive_scale_down_idle_intervals == 0
                || p.adaptive_scale_down_idle_intervals > 360
            {
                return Err(PhpxError::Config(
                    "runtime.process.adaptive_scale_down_idle_intervals must be 1-360".to_string(),
                ));
            }
            parse_size(&p.memory_limit)?;
            let soft = parse_size(&p.memory_soft_limit)?;
            let hard = parse_size(&p.memory_hard_limit)?;
            if soft > 0 && hard > 0 && soft >= hard {
                return Err(PhpxError::Config(format!(
                    "runtime.process.memory_soft_limit ({}) must be below memory_hard_limit ({}) (soft=recycle, hard=terminate)",
                    p.memory_soft_limit, p.memory_hard_limit
                )));
            }
            if p.memory_sample_interval_seconds == 0 || p.memory_sample_interval_seconds > 60 {
                return Err(PhpxError::Config(
                    "runtime.process.memory_sample_interval_seconds must be 1-60".to_string(),
                ));
            }
        }
        if let Some(queue) = &self.queue {
            let d = queue.driver.to_ascii_lowercase();
            if d != "memory" && d != "sqlite" && d != "redis" {
                return Err(phpx_core::PhpxError::Config(format!(
                    "queue.driver must be memory, sqlite or redis, got {}",
                    queue.driver
                )));
            }
            if queue.workers == 0 || queue.workers > 16 {
                return Err(phpx_core::PhpxError::Config(
                    "queue.workers must be 1-16 (bounded)".to_string(),
                ));
            }
            if d == "sqlite" {
                match &queue.database {
                    Some(db) if !db.trim().is_empty() => {}
                    _ => return Err(phpx_core::PhpxError::Config(
                        "queue.driver=sqlite requires [queue] database path (e.g. ./data/queue.db)"
                            .to_string(),
                    )),
                }
            }
            if d == "redis" {
                let env_ok = std::env::var("PHPX_REDIS_URL")
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                match &queue.url {
                    Some(u) if !u.trim().is_empty() => {}
                    _ if env_ok => {}
                    _ => return Err(phpx_core::PhpxError::Config(
                        "queue.driver=redis requires [queue] url (e.g. redis://127.0.0.1:6379) or PHPX_REDIS_URL env"
                            .to_string(),
                    )),
                }
            }
        }
        if let Some(sched) = &self.scheduler {
            if sched.poll_interval_secs == 0 || sched.poll_interval_secs > 3600 {
                return Err(PhpxError::Config(
                    "scheduler.poll_interval_secs must be 1-3600".to_string(),
                ));
            }
            if sched.schedules.len() > phpx_scheduler::MAX_SCHEDULES {
                return Err(PhpxError::Config(format!(
                    "too many schedules: {} (max {}, keep the fleet bounded)",
                    sched.schedules.len(),
                    phpx_scheduler::MAX_SCHEDULES
                )));
            }
            let mut seen = HashSet::new();
            for s in &sched.schedules {
                if !seen.insert(s.id.trim().to_string()) {
                    return Err(PhpxError::Config(format!(
                        "duplicate schedule id {:?} (ids must be unique)",
                        s.id
                    )));
                }
                // Full semantic check (cron, timezone, payload, policy) lives
                // in phpx-scheduler so config and CLI can never disagree.
                phpx_scheduler::ScheduleDef::new(
                    &s.id,
                    &s.cron,
                    &s.timezone,
                    &s.payload,
                    s.enabled,
                    &s.missed_policy,
                    s.max_retries,
                )
                .map_err(|e| {
                    let text = e.to_string();
                    let inner = text.strip_prefix("configuration error: ").unwrap_or(&text);
                    PhpxError::Config(format!("invalid schedule: {inner}"))
                })?;
            }
        }
        if let Some(cache) = &self.cache {
            let d = cache.driver.to_ascii_lowercase();
            if d != "memory" && d != "filesystem" && d != "redis" {
                return Err(phpx_core::PhpxError::Config(format!(
                    "cache.driver must be memory, filesystem or redis, got {}",
                    cache.driver
                )));
            }
            if d == "filesystem" {
                match &cache.dir {
                    Some(dir) if !dir.trim().is_empty() => {}
                    _ => {
                        return Err(phpx_core::PhpxError::Config(
                            "cache.driver=filesystem requires [cache] dir (e.g. ./data/cache)"
                                .to_string(),
                        ))
                    }
                }
            }
            if d == "redis" {
                let env_ok = std::env::var("PHPX_REDIS_URL")
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                match &cache.url {
                    Some(u) if !u.trim().is_empty() => {}
                    _ if env_ok => {}
                    _ => return Err(phpx_core::PhpxError::Config(
                        "cache.driver=redis requires [cache] url (e.g. redis://127.0.0.1:6379) or PHPX_REDIS_URL env"
                            .to_string(),
                    )),
                }
            }
            if let Some(m) = &cache.max_memory {
                let bytes = parse_size(m)?;
                if !(1024..=1024 * 1024 * 1024).contains(&bytes) {
                    return Err(PhpxError::Config(
                        "cache.max_memory must parse to 1KB-1GB (e.g. 128MB)".to_string(),
                    ));
                }
            }
            if let Some(n) = cache.max_entries {
                if n == 0 || n > 100_000 {
                    return Err(PhpxError::Config(
                        "cache.max_entries must be 1-100000 (bounded)".to_string(),
                    ));
                }
            }
        }
        if let Some(s) = &self.storage {
            let d = s.driver.to_ascii_lowercase();
            if d != "local" && d != "s3" {
                return Err(phpx_core::PhpxError::Config(format!(
                    "storage.driver must be local or s3, got {}",
                    s.driver
                )));
            }
            if d == "local" {
                match &s.root {
                    Some(r) if !r.trim().is_empty() => {}
                    _ => {
                        return Err(phpx_core::PhpxError::Config(
                            "storage.driver=local requires [storage] root (e.g. ./data/storage)"
                                .to_string(),
                        ))
                    }
                }
            } else {
                match (&s.endpoint, &s.bucket) {
                    (Some(ep), Some(b))
                        if !ep.trim().is_empty() && !b.trim().is_empty() => {}
                    _ => {
                        return Err(phpx_core::PhpxError::Config(
                            "storage.driver=s3 requires [storage] endpoint + bucket (prototype, see storage.md)"
                                .to_string(),
                        ))
                    }
                }
            }
        }
        Ok(())
    }

    pub fn tls_enabled(&self) -> bool {
        self.server.tls
    }

    pub fn body_limit_bytes(&self) -> u64 {
        parse_size(&self.server.request_body_limit).unwrap_or(16 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../phpx.toml.example");

    #[test]
    fn example_parses_and_validates() {
        let cfg = PhpxConfig::parse_str(EXAMPLE).expect("example must parse");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.server.port, 8080);
    }

    #[test]
    fn execution_mode_defaults_to_one_shot_and_accepts_persistent() {
        let cfg = PhpxConfig::parse_str(EXAMPLE).expect("example parses");
        let p = cfg.runtime.process.as_ref().expect("example has process");
        assert_eq!(p.execution_mode, "one-shot");
        // P0-C guardian defaults: disabled (limits "0"), 5s idle sampling.
        assert_eq!(p.memory_soft_limit, "0");
        assert_eq!(p.memory_hard_limit, "0");
        assert_eq!(p.memory_sample_interval_seconds, 5);
        // P0-B: persistent is now a selectable, implemented mode.
        let mut persistent = cfg.clone();
        persistent.runtime.process.as_mut().unwrap().execution_mode = "persistent".to_string();
        persistent.validate().expect("persistent must validate");
        let mut bad = cfg.clone();
        bad.runtime.process.as_mut().unwrap().execution_mode = "bogus".to_string();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn memory_guardian_limits_validate() {
        let base = PhpxConfig::parse_str(EXAMPLE).expect("example parses");
        // Disabled by default: both zero parses fine.
        assert!(base.validate().is_ok());
        let with = |soft: &str, hard: &str, interval: u64| {
            let mut cfg = base.clone();
            let p = cfg.runtime.process.as_mut().unwrap();
            p.memory_soft_limit = soft.to_string();
            p.memory_hard_limit = hard.to_string();
            p.memory_sample_interval_seconds = interval;
            cfg.validate().is_ok()
        };
        // Soft >= hard is rejected.
        assert!(!with("1G", "1G", 5));
        assert!(!with("1G", "512M", 5));
        // Soft < hard validates; hard-only (soft disabled) validates.
        assert!(with("256M", "512M", 5));
        assert!(with("0", "512M", 5));
        // Bad size strings rejected; interval bounds enforced.
        assert!(!with("bogus", "512M", 5));
        assert!(!with("256M", "512M", 0));
        assert!(!with("256M", "512M", 61));
    }

    #[test]
    fn redis_drivers_validate_with_url_or_env() {
        // Hermetic: validation reads PHPX_REDIS_URL, so stash it first.
        let saved = std::env::var("PHPX_REDIS_URL").ok();
        unsafe { std::env::remove_var("PHPX_REDIS_URL") };
        let base = PhpxConfig::parse_str(EXAMPLE).expect("example parses");
        // Redis without url and without env is rejected with an actionable message.
        let mut no_url = base.clone();
        no_url.queue = Some(QueueConfig {
            driver: "redis".to_string(),
            database: None,
            url: None,
            workers: 2,
        });
        no_url.cache = Some(CacheConfig {
            driver: "redis".to_string(),
            max_memory: None,
            dir: None,
            url: None,
            max_entries: None,
        });
        let err = no_url.validate().expect_err("redis without url must fail");
        assert!(err.to_string().contains("url"), "{err}");
        // Explicit url validates.
        let mut with_url = base.clone();
        with_url.queue = Some(QueueConfig {
            driver: "redis".to_string(),
            database: None,
            url: Some("redis://127.0.0.1:6379".to_string()),
            workers: 2,
        });
        with_url.cache = Some(CacheConfig {
            driver: "redis".to_string(),
            max_memory: None,
            dir: None,
            url: Some("redis://127.0.0.1:6379".to_string()),
            max_entries: None,
        });
        assert!(with_url.validate().is_ok());
        // Unknown drivers are still rejected.
        let mut bad = base.clone();
        bad.queue = Some(QueueConfig {
            driver: "nats".to_string(),
            database: None,
            url: None,
            workers: 2,
        });
        assert!(bad.validate().is_err());
        // Env fallback validates when the config url is absent.
        unsafe { std::env::set_var("PHPX_REDIS_URL", "redis://127.0.0.1:6379") };
        assert!(no_url.validate().is_ok());
        if let Some(v) = saved {
            unsafe { std::env::set_var("PHPX_REDIS_URL", v) };
        }
    }

    #[test]
    fn rejects_bad_schema() {
        let bad = EXAMPLE.replace("schema_version = 1", "schema_version = 99");
        let err = PhpxConfig::parse_str(&bad).unwrap_err();
        assert!(err.to_string().contains("schema_version"));
    }

    #[test]
    fn rejects_min_gt_max() {
        let mut cfg = PhpxConfig::parse_str(EXAMPLE).expect("example parses");
        let p = cfg.runtime.process.get_or_insert_with(Default::default);
        p.workers = 2;
        p.min_workers = 4;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn effective_queue_bound_auto() {
        let mut p = RuntimeProcessConfig {
            workers: 4,
            queue_bound: None,
            ..Default::default()
        };
        assert_eq!(p.effective_queue_bound(), 16);
        p.queue_bound = Some(7);
        assert_eq!(p.effective_queue_bound(), 7);
    }
}
