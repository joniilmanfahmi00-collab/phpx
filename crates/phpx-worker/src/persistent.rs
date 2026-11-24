use phpx_core::{PhpxError, Result};
use phpx_runtime::{PhpRequest, PhpResponse, PROTOCOL_VERSION};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Embedded persistent worker script (handshake + framed loop + SAPI reset).
const WORKER_SCRIPT: &str = include_str!("worker-persistent.php");

fn materialize_worker_script() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "phpx-worker-persistent-{}.php",
        WORKER_SCRIPT.len()
    ));
    if !path.is_file() {
        let _ = std::fs::write(&path, WORKER_SCRIPT);
    }
    path
}

// ---------------------------------------------------------------------------
// P0-B: deterministic worker lifecycle (SPEC 5.1 + ADR-0014, implemented).
// ---------------------------------------------------------------------------

/// Supervised worker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Starting,
    Ready,
    Busy,
    Draining,
    Stopped,
    Unhealthy,
    Restarting,
    Failed,
}

impl WorkerState {
    pub fn as_str(self) -> &'static str {
        match self {
            WorkerState::Starting => "starting",
            WorkerState::Ready => "ready",
            WorkerState::Busy => "busy",
            WorkerState::Draining => "draining",
            WorkerState::Stopped => "stopped",
            WorkerState::Unhealthy => "unhealthy",
            WorkerState::Restarting => "restarting",
            WorkerState::Failed => "failed",
        }
    }

    /// Numeric encoding for `phpx_worker_state`.
    pub fn as_metric_value(self) -> u64 {
        match self {
            WorkerState::Starting => 0,
            WorkerState::Ready => 1,
            WorkerState::Busy => 2,
            WorkerState::Draining => 3,
            WorkerState::Stopped => 4,
            WorkerState::Unhealthy => 5,
            WorkerState::Restarting => 6,
            WorkerState::Failed => 7,
        }
    }

    pub fn can_accept_work(self) -> bool {
        self == WorkerState::Ready
    }

    /// Pure transition table. `None` = illegal transition (never applied).
    /// Budget escalation to `Failed` is a supervisor decision, not a transition.
    pub fn transition(self, event: WorkerEvent) -> Option<WorkerState> {
        use WorkerEvent::*;
        use WorkerState::*;
        match (self, event) {
            (Starting, HandshakeOk) => Some(Ready),
            (Starting, HandshakeFailed) => Some(Restarting),
            (Starting, Shutdown) => Some(Stopped),
            (Ready, Assign) => Some(Busy),
            (Ready, DrainRequested) => Some(Draining),
            (Ready, Shutdown) => Some(Stopped),
            (Busy, CompleteOk) => Some(Ready),
            (Busy, CompleteFailed) => Some(Ready),
            (Busy, CrashDetected) => Some(Unhealthy),
            (Busy, TimeoutDetected) => Some(Unhealthy),
            (Busy, ProtocolError) => Some(Unhealthy),
            (Busy, DrainRequested) => Some(Draining),
            (Busy, Shutdown) => Some(Stopped),
            (Draining, CompleteOk) => Some(Stopped),
            (Draining, CompleteFailed) => Some(Stopped),
            (Draining, Shutdown) => Some(Stopped),
            (Unhealthy, RestartOk) => Some(Ready),
            (Unhealthy, RestartFailed) => Some(Restarting),
            (Unhealthy, Shutdown) => Some(Stopped),
            (Restarting, HandshakeOk) => Some(Ready),
            (Restarting, HandshakeFailed) => Some(Restarting),
            (Restarting, Shutdown) => Some(Stopped),
            (Failed, Shutdown) => Some(Stopped),
            _ => None,
        }
    }
}

/// Lifecycle events driving the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEvent {
    HandshakeOk,
    HandshakeFailed,
    Assign,
    CompleteOk,
    CompleteFailed,
    CrashDetected,
    TimeoutDetected,
    ProtocolError,
    DrainRequested,
    RestartOk,
    RestartFailed,
    Shutdown,
}

/// Point-in-time view of one supervised worker (metrics, CLI, tests).
#[derive(Debug, Clone)]
pub struct WorkerSnapshot {
    pub id: u64,
    pub incarnation: u64,
    pub pid: Option<u32>,
    pub state: WorkerState,
    pub uptime_secs: u64,
    pub completed: u64,
    pub restarts: u64,
    pub exit_reason: Option<String>,
    /// P0-C: last sampled OS RSS (0 = never sampled).
    pub rss_bytes: u64,
    /// P0-C: max sampled OS RSS over the current incarnation.
    pub peak_rss_bytes: u64,
    /// P0-C: last successful spawn+handshake duration.
    pub startup_duration_ms: u64,
    /// P0-C: last kill+wait (termination) duration.
    pub shutdown_duration_ms: u64,
    /// P0-C: seconds since the last successful request or clean sample.
    pub last_ok_age_secs: Option<u64>,
    /// P0-C: sampler's verdict (false when RSS is over a limit or sampling fails).
    pub healthy: bool,
    /// P0-C: restart counts by cause ("crash", "timeout", "protocol",
    /// "recycle-budget", "soft-limit", "hard-limit", "demand-retry").
    pub restarts_by_reason: Vec<(String, u64)>,
    /// P0-C: soft/hard guardian trigger counts for this worker incarnation chain.
    pub soft_hits: u64,
    pub hard_hits: u64,
    /// P1-A: last unplanned-restart incident timeline (detection ->
    /// restart start -> replacement ready). None = no incident yet.
    pub last_recovery: Option<RecoverySnapshot>,
}

/// P1-A: point-in-time view of one recovery incident. Durations are measured
/// from clearly defined events (plan asks for them separately, not as one
/// ambiguous "recovery time"): failure detection, restart initiation,
/// replacement readiness.
#[derive(Debug, Clone)]
pub struct RecoverySnapshot {
    pub cause: String,
    pub detection_to_restart_ms: Option<u64>,
    pub restart_to_ready_ms: Option<u64>,
    pub total_ms: Option<u64>,
    pub age_secs: u64,
}

/// P2 adaptive scale decision (pure: unit-tested, no I/O).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveAction {
    Hold,
    ScaleUp,
    ScaleDown,
}

/// P2 adaptive live observation (one control-loop evaluation).
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveSignal {
    /// Queued requests still waiting for a worker.
    pub waiting: usize,
    /// Requests currently executing.
    pub active: usize,
    /// Spawned workers (slots not parked as Stopped spares).
    pub current: usize,
    /// Consecutive all-idle evaluations so far (caller bumps on Hold).
    pub idle_streak: u32,
    /// A worker recently hit a guardian soft/hard limit.
    pub memory_pressure: bool,
    /// Draining, crash-loop circuit open, or inside the cooldown.
    pub blocked: bool,
}

/// P2 adaptive tuning (the subset of the pool config the law needs).
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveTuning {
    /// Floor (and initial size when adaptive is enabled).
    pub min_w: usize,
    /// Ceiling (`worker_count`).
    pub max_w: usize,
    /// Scale-up trigger: queued requests at/over this while saturated.
    pub up_depth: usize,
    /// Scale-down trigger: consecutive all-idle evaluations.
    pub down_intervals: u32,
}

/// P2 adaptive control law. One worker per evaluation at most; the caller
/// (control loop) enforces the interval, the cooldown, and the idle streak.
///
/// - `blocked` (draining/circuit/cooldown) always holds.
/// - `memory_pressure` blocks scale-up only (healing the floor and
///   scale-down still allowed).
/// - Scale-up: every spawned worker busy (`active >= current`) with
///   `waiting >= up_depth` queued behind them, below the ceiling.
/// - Scale-down: strictly more than the floor, fully idle
///   (`waiting == 0 && active == 0`) for `down_intervals` straight evaluations.
/// - Out-of-range drift self-heals toward [min, max].
pub fn adaptive_decide(sig: AdaptiveSignal, tun: AdaptiveTuning) -> AdaptiveAction {
    if sig.blocked {
        return AdaptiveAction::Hold;
    }
    if sig.current < tun.min_w {
        return AdaptiveAction::ScaleUp;
    }
    if sig.current > tun.max_w {
        return AdaptiveAction::ScaleDown;
    }
    if sig.current < tun.max_w
        && sig.waiting >= tun.up_depth
        && sig.active >= sig.current
        && !sig.memory_pressure
    {
        return AdaptiveAction::ScaleUp;
    }
    if sig.current > tun.min_w
        && sig.waiting == 0
        && sig.active == 0
        && sig.idle_streak + 1 >= tun.down_intervals
    {
        return AdaptiveAction::ScaleDown;
    }
    AdaptiveAction::Hold
}

// ---------------------------------------------------------------------------
// Pool configuration (fixed-size supervised pool by default; P2 adaptive
// scaling is opt-in via `adaptive_enabled` and stays off unless configured).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PersistentPoolConfig {
    pub worker_count: u32,
    pub php_bin: String,
    pub script_path: Option<PathBuf>,
    /// PHP-level heap limit (`-d memory_limit`) for every supervised worker.
    /// Bounds the Zend heap only, NOT OS RSS (see soft/hard below).
    pub php_memory_limit: Option<String>,
    pub startup_timeout: Duration,
    pub execution_timeout: Duration,
    pub queue_bound: usize,
    pub queue_wait_timeout: Duration,
    pub drain_timeout: Duration,
    pub max_requests_per_worker: u64,
    pub restart_max_retries: u32,
    pub restart_backoff_base: Duration,
    pub restart_backoff_max: Duration,
    /// P0-C guardian: OS-RSS soft limit, 0 = disabled. At/over -> graceful recycle.
    pub memory_soft_bytes: u64,
    /// P0-C guardian: OS-RSS hard limit, 0 = disabled. At/over -> protective termination.
    pub memory_hard_bytes: u64,
    /// P0-C guardian: idle-worker sampling period.
    pub memory_sample_interval: Duration,
    /// P2 adaptive: disabled by default (fixed pool). When enabled, the pool
    /// starts at `adaptive_min_workers`, grows on sustained pressure up to
    /// `worker_count` (ceiling), and shrinks back when idle. One worker per
    /// interval at most; cooldown + hysteresis prevent oscillation.
    pub adaptive_enabled: bool,
    /// P2 adaptive floor (and initial size when enabled). Must be 1..=worker_count.
    pub adaptive_min_workers: u32,
    /// P2 adaptive control-loop period.
    pub adaptive_interval: Duration,
    /// P2 adaptive quiet period between scale actions (anti-flap).
    pub adaptive_cooldown: Duration,
    /// P2 adaptive scale-up trigger: queued requests at/over this while every
    /// spawned worker is busy.
    pub adaptive_scale_up_queue_depth: usize,
    /// P2 adaptive scale-down trigger: consecutive all-idle evaluations.
    pub adaptive_scale_down_idle_intervals: u32,
    /// P2 adaptive host safeguard: refuse scale-up while any worker shows
    /// memory pressure (guardian soft/hard signal). Scale-down still allowed.
    pub adaptive_memory_guard: bool,
}

impl Default for PersistentPoolConfig {
    fn default() -> Self {
        Self {
            worker_count: 2,
            php_bin: std::env::var("PHPX_PHP_BIN").unwrap_or_else(|_| "php".to_string()),
            script_path: None,
            php_memory_limit: None,
            startup_timeout: Duration::from_secs(10),
            execution_timeout: Duration::from_secs(30),
            queue_bound: 16,
            queue_wait_timeout: Duration::from_secs(2),
            drain_timeout: Duration::from_secs(10),
            max_requests_per_worker: 1000,
            restart_max_retries: 5,
            restart_backoff_base: Duration::from_millis(250),
            restart_backoff_max: Duration::from_secs(30),
            memory_soft_bytes: 0,
            memory_hard_bytes: 0,
            memory_sample_interval: Duration::from_secs(5),
            adaptive_enabled: false,
            adaptive_min_workers: 1,
            adaptive_interval: Duration::from_secs(5),
            adaptive_cooldown: Duration::from_secs(30),
            adaptive_scale_up_queue_depth: 2,
            adaptive_scale_down_idle_intervals: 3,
            adaptive_memory_guard: true,
        }
    }
}

impl PersistentPoolConfig {
    pub fn validate(&self) -> Result<()> {
        if self.worker_count == 0 || self.worker_count > 1024 {
            return Err(PhpxError::Config(
                "persistent worker_count must be 1-1024 (fixed-size pool, never unbounded)"
                    .to_string(),
            ));
        }
        if self.queue_bound == 0 || self.queue_bound > 100_000 {
            return Err(PhpxError::Config(
                "queue_bound must be 1-100000 (bounded, backpressure mandatory)".to_string(),
            ));
        }
        if self.max_requests_per_worker == 0 {
            return Err(PhpxError::Config(
                "max_requests_per_worker must be >= 1 (recycle budget)".to_string(),
            ));
        }
        if self.restart_max_retries > 100 {
            return Err(PhpxError::Config(
                "restart_max_retries must be 0-100".to_string(),
            ));
        }
        if self.memory_soft_bytes > 0
            && self.memory_hard_bytes > 0
            && self.memory_soft_bytes >= self.memory_hard_bytes
        {
            return Err(PhpxError::Config(
                "memory_soft_bytes must be below memory_hard_bytes (soft=recycle, hard=terminate)"
                    .to_string(),
            ));
        }
        if self.memory_sample_interval.is_zero()
            || self.memory_sample_interval > Duration::from_secs(60)
        {
            return Err(PhpxError::Config(
                "memory_sample_interval must be 1-60s".to_string(),
            ));
        }
        if self.adaptive_interval.is_zero() || self.adaptive_interval > Duration::from_secs(60) {
            return Err(PhpxError::Config(
                "adaptive_interval must be 1-60s".to_string(),
            ));
        }
        if self.adaptive_cooldown > Duration::from_secs(300) {
            return Err(PhpxError::Config(
                "adaptive_cooldown must be 0-300s".to_string(),
            ));
        }
        if self.adaptive_scale_up_queue_depth == 0 || self.adaptive_scale_up_queue_depth > 10_000 {
            return Err(PhpxError::Config(
                "adaptive_scale_up_queue_depth must be 1-10000".to_string(),
            ));
        }
        if self.adaptive_scale_down_idle_intervals == 0
            || self.adaptive_scale_down_idle_intervals > 360
        {
            return Err(PhpxError::Config(
                "adaptive_scale_down_idle_intervals must be 1-360".to_string(),
            ));
        }
        if self.adaptive_enabled
            && (self.adaptive_min_workers == 0 || self.adaptive_min_workers > self.worker_count)
        {
            return Err(PhpxError::Config(format!(
                "adaptive_min_workers ({}) must be 1..=worker_count ({})",
                self.adaptive_min_workers, self.worker_count
            )));
        }
        Ok(())
    }

    /// Exponential backoff with deterministic jitter for retry attempt n.
    pub fn backoff_for_retry(&self, attempt: u32) -> Duration {
        let mut ms = self.restart_backoff_base.as_millis() as u64;
        for _ in 0..attempt.min(10) {
            ms = (ms * 2).min(self.restart_backoff_max.as_millis() as u64);
        }
        let jitter_pct = (attempt as u64 * 37 + 11) % 26;
        ms += ms * jitter_pct / 100;
        ms = ms.min(self.restart_backoff_max.as_millis() as u64 * 2);
        Duration::from_millis(ms)
    }
}

// ---------------------------------------------------------------------------
// Internal worker slot (one supervised OS process).
// ---------------------------------------------------------------------------

struct WorkerChild {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    /// Bounded stderr tail (drained continuously so a chatty worker can
    /// never fill the OS pipe and hang; read on crash for diagnostics).
    stderr_log: Arc<Mutex<std::collections::VecDeque<u8>>>,
    /// Drain-thread handle; joined only on the crash path (completeness of
    /// the tail). Elsewhere the thread exits by itself on EOF when the
    /// child dies, and the handle is dropped with the slot.
    stderr_thread: Option<std::thread::JoinHandle<()>>,
}

/// Max bytes kept per worker stderr tail (diagnostics only, bounded).
const STDERR_TAIL_CAP: usize = 16 * 1024;

fn spawn_stderr_drain(
    err: std::process::ChildStderr,
) -> (
    Arc<Mutex<std::collections::VecDeque<u8>>>,
    std::thread::JoinHandle<()>,
) {
    let log = Arc::new(Mutex::new(std::collections::VecDeque::<u8>::new()));
    let thread_log = log.clone();
    let handle = std::thread::spawn(move || {
        let mut err = err;
        let mut buf = [0u8; 4096];
        loop {
            match err.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut log = thread_log.lock().unwrap();
                    log.extend(buf[..n].iter());
                    while log.len() > STDERR_TAIL_CAP {
                        log.pop_front();
                    }
                }
            }
        }
    });
    (log, handle)
}

struct WorkerSlot {
    id: u64,
    incarnation: u64,
    pid: Option<u32>,
    state: WorkerState,
    started_at: Option<Instant>,
    completed: u64,
    restarts: u64,
    restarts_by_reason: Vec<(String, u64)>,
    consecutive_startup_failures: u64,
    restart_not_before: Option<Instant>,
    exit_reason: Option<String>,
    child: Option<WorkerChild>,
    // P0-C guardian observations (reset on every respawn).
    rss_bytes: u64,
    peak_rss_bytes: u64,
    last_sample_at: Option<Instant>,
    last_ok_at: Option<Instant>,
    healthy: bool,
    startup_duration_ms: u64,
    shutdown_duration_ms: u64,
    soft_hits: u64,
    hard_hits: u64,
    // P1-A: last unplanned-restart incident (kept across respawns so the
    // timeline survives the recovery it describes; cleared only by... never:
    // "last incident" is sticky by design).
    last_recovery: Option<RecoveryIncident>,
}

/// P1-A: timestamps of one unplanned restart, from clearly defined events.
#[derive(Debug, Clone, Copy)]
struct RecoveryIncident {
    cause: &'static str,
    detected_at: Instant,
    restart_started_at: Option<Instant>,
    ready_at: Option<Instant>,
}

impl WorkerSlot {
    fn fresh(id: u64) -> Self {
        Self {
            id,
            incarnation: 0,
            pid: None,
            state: WorkerState::Starting,
            started_at: None,
            completed: 0,
            restarts: 0,
            restarts_by_reason: Vec::new(),
            consecutive_startup_failures: 0,
            restart_not_before: None,
            exit_reason: None,
            child: None,
            rss_bytes: 0,
            peak_rss_bytes: 0,
            last_sample_at: None,
            last_ok_at: None,
            healthy: true,
            startup_duration_ms: 0,
            shutdown_duration_ms: 0,
            soft_hits: 0,
            hard_hits: 0,
            last_recovery: None,
        }
    }

    /// Reset per-incarnation observations after a (re)spawn.
    fn reset_incarnation_stats(&mut self) {
        self.rss_bytes = 0;
        self.peak_rss_bytes = 0;
        self.last_sample_at = None;
        self.last_ok_at = Some(Instant::now());
        self.healthy = true;
    }

    fn note_restart(&mut self, cause: &'static str) {
        self.restarts += 1;
        if let Some(entry) = self.restarts_by_reason.iter_mut().find(|(c, _)| c == cause) {
            entry.1 += 1;
        } else {
            self.restarts_by_reason.push((cause.to_string(), 1));
        }
    }

    /// P1-A: record failure detection (start of a recovery timeline).
    fn note_detected(&mut self, cause: &'static str) {
        self.last_recovery = Some(RecoveryIncident {
            cause,
            detected_at: Instant::now(),
            restart_started_at: None,
            ready_at: None,
        });
    }

    /// P1-A: record restart initiation (end of detection->restart leg).
    fn note_restart_started(&mut self) {
        let now = Instant::now();
        if let Some(inc) = self.last_recovery.as_mut() {
            if inc.restart_started_at.is_none() {
                inc.restart_started_at = Some(now);
            }
        }
    }

    /// P1-A: record replacement readiness (end of restart->ready leg).
    fn note_restart_ready(&mut self) {
        let now = Instant::now();
        if let Some(inc) = self.last_recovery.as_mut() {
            if inc.ready_at.is_none() {
                inc.ready_at = Some(now);
            }
        }
    }

    fn recovery_snapshot(&self) -> Option<RecoverySnapshot> {
        let inc = self.last_recovery?;
        let to_ms = |a: Instant, b: Instant| b.duration_since(a).as_millis() as u64;
        Some(RecoverySnapshot {
            cause: inc.cause.to_string(),
            detection_to_restart_ms: inc.restart_started_at.map(|t| to_ms(inc.detected_at, t)),
            restart_to_ready_ms: match (inc.restart_started_at, inc.ready_at) {
                (Some(s), Some(r)) => Some(to_ms(s, r)),
                _ => None,
            },
            total_ms: inc.ready_at.map(|t| to_ms(inc.detected_at, t)),
            age_secs: inc.detected_at.elapsed().as_secs(),
        })
    }
}

impl WorkerSlot {
    fn apply(&mut self, event: WorkerEvent) {
        if let Some(next) = self.state.transition(event) {
            self.state = next;
        }
    }

    fn uptime_secs(&self) -> u64 {
        self.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }
}

#[derive(Debug, Default)]
struct PoolShared {
    waiting: usize,
    draining: bool,
    consecutive_failures: u64,
    circuit_open_until: Option<Instant>,
    circuit_breaks: u64,
    completed: u64,
    failed: u64,
    timeouts: u64,
    rejected_overload: u64,
    recycled: u64,
    restarts: u64,
    active: usize,
    recycling: bool,
    // P2 adaptive observations (guarded by the same lock).
    /// Currently spawned workers (slots not Stopped).
    spawned: usize,
    scale_ups: u64,
    scale_downs: u64,
    last_scale_at: Option<Instant>,
    /// Consecutive control-loop evaluations with zero load.
    idle_streak: u32,
    /// Set when a worker hits a guardian soft/hard limit; cleared by the
    /// next Below sample. Blocks scale-up only (memory safeguard).
    memory_pressure: bool,
}

/// Supervised pool of long-lived PHP workers (P0-B fixed-size by default).
///
/// Exactly one request per worker at a time. Blocking pipe I/O runs on the
/// calling thread, which the gateway invokes via `spawn_blocking` (ADR-0006),
/// never on Tokio executor threads.
///
/// P0-C: a background sampler thread periodically samples OS RSS of idle
/// (Ready) workers and enforces the soft/hard memory policy. Request
/// completion is an additional sampling point, so a hot worker is checked
/// even when it never idles.
pub struct PersistentPool {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: PersistentPoolConfig,
    script_path: PathBuf,
    slots: Vec<Mutex<WorkerSlot>>,
    shared: Arc<(Mutex<PoolShared>, Condvar)>,
    stop_sampler: std::sync::atomic::AtomicBool,
}

enum ExchangeOutcome {
    Responded(PhpResponse),
    Timeout,
    Died(String),
    Protocol(String),
}

fn read_handshake(stdout: ChildStdout, timeout: Duration) -> Result<(BufReader<ChildStdout>, u32)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let res = reader.read_line(&mut line).map(|_| line);
        let _ = tx.send((res, reader));
    });
    match rx.recv_timeout(timeout) {
        Ok((Ok(line), reader)) => {
            let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| {
                PhpxError::Runtime(format!("persistent worker handshake not JSON: {e}"))
            })?;
            if v.get("phpx").and_then(|x| x.as_str()) != Some("ready") {
                return Err(PhpxError::Runtime(format!(
                    "persistent worker handshake missing ready marker: {}",
                    line.trim().chars().take(200).collect::<String>()
                )));
            }
            let ver = v
                .get("protocol_version")
                .and_then(|x| x.as_u64())
                .unwrap_or(0) as u32;
            if ver != PROTOCOL_VERSION {
                return Err(PhpxError::Runtime(format!(
                    "persistent worker protocol_version {ver} != {PROTOCOL_VERSION}"
                )));
            }
            let pid = v.get("pid").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            Ok((reader, if pid == 0 { None } else { Some(pid) }.unwrap_or(0)))
        }
        Ok((Err(e), _)) => Err(PhpxError::Runtime(format!(
            "persistent worker handshake read failed (EOF crash?): {e}"
        ))),
        Err(_) => Err(PhpxError::Runtime(
            "persistent worker handshake timeout (php hung or wrong script)".to_string(),
        )),
    }
}

fn spawn_supervised(
    php_bin: &str,
    script: &Path,
    startup_timeout: Duration,
    memory_limit: Option<&str>,
) -> Result<(WorkerChild, u32, u64)> {
    let t0 = Instant::now();
    let mut cmd = Command::new(php_bin);
    // PHP-level heap limit (validated size string at config layer).
    // Bounds the Zend heap only; OS RSS is governed by the soft/hard guardian.
    if let Some(limit) = memory_limit.filter(|s| !s.trim().is_empty()) {
        cmd.arg("-d").arg(format!("memory_limit={limit}"));
    }
    let mut child = cmd
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            PhpxError::Runtime(format!("persistent worker spawn failed ({php_bin}): {e}"))
        })?;
    let pid = child.id();
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| PhpxError::Runtime("persistent worker stdin unavailable".to_string()))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        let _ = child.kill();
        PhpxError::Runtime("persistent worker stdout unavailable".to_string())
    })?;
    // Drain stderr continuously (bounded tail): a chatty worker must never
    // fill the OS pipe and hang on a blocked write.
    let (stderr_log, stderr_thread) = match child.stderr.take() {
        Some(err) => {
            let (log, handle) = spawn_stderr_drain(err);
            (log, Some(handle))
        }
        None => (
            Arc::new(Mutex::new(std::collections::VecDeque::new())),
            None,
        ),
    };
    match read_handshake(stdout, startup_timeout) {
        Ok((reader, reported_pid)) => Ok((
            WorkerChild {
                child,
                stdin: Some(stdin),
                stdout: Some(reader),
                stderr_log,
                stderr_thread,
            },
            if reported_pid != 0 { reported_pid } else { pid },
            t0.elapsed().as_millis() as u64,
        )),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e)
        }
    }
}

/// One framed request/response exchange with a hard wall-clock bound.
/// Pipes are moved into the I/O thread and returned on completion; on
/// timeout the thread keeps them (it unblocks when the kill lands) while
/// the supervisor proceeds to restart with fresh pipes.
fn exchange(wc: &mut WorkerChild, req_bytes: &[u8], timeout: Duration) -> ExchangeOutcome {
    let Some(stdin) = wc.stdin.take() else {
        return ExchangeOutcome::Protocol("worker stdin missing".to_string());
    };
    let Some(stdout) = wc.stdout.take() else {
        wc.stdin = Some(stdin);
        return ExchangeOutcome::Protocol("worker stdout missing".to_string());
    };
    let (tx, rx) = mpsc::channel();
    let bytes = req_bytes.to_vec();
    std::thread::spawn(move || {
        let mut stdin = stdin;
        let mut stdout = stdout;
        let res: std::result::Result<PhpResponse, String> = (|| {
            stdin
                .write_all(&bytes)
                .map_err(|e| format!("frame write failed: {e}"))?;
            stdin
                .flush()
                .map_err(|e| format!("frame flush failed: {e}"))?;
            phpx_runtime::decode_frame::<PhpResponse, _>(&mut stdout)
                .map_err(|e| format!("frame read failed: {e}"))
        })();
        let _ = tx.send((res, stdin, stdout));
    });
    match rx.recv_timeout(timeout) {
        Ok((Ok(mut resp), stdin_back, stdout_back)) => {
            // Pipes belong to a live worker again.
            wc.stdin = Some(stdin_back);
            wc.stdout = Some(stdout_back);
            // Freshness: the worker must echo our request id.
            resp.request_id.truncate(120);
            ExchangeOutcome::Responded(resp)
        }
        Ok((Err(detail), stdin_back, stdout_back)) => {
            // I/O failed: worker died (broken pipe / EOF / fatal text on
            // stdout) or desynced while alive. A dying process and a
            // desynced one look identical at the first instant, so give a
            // crashing worker a grace period to exit: only a process that is
            // still alive afterwards is treated as desynced (unsafe to
            // reuse). This keeps crash-vs-protocol attribution deterministic
            // for the common PHP-fatal case.
            let mut exited = wc.child.try_wait().ok().flatten().is_some();
            if !exited {
                for _ in 0..20 {
                    std::thread::sleep(Duration::from_millis(10));
                    if wc.child.try_wait().ok().flatten().is_some() {
                        exited = true;
                        break;
                    }
                }
            }
            if exited {
                let code = wc.child.wait().ok().and_then(|s| s.code());
                // Join the drain thread so the tail includes the final bytes
                // (the process is dead, so the thread has reached EOF).
                if let Some(handle) = wc.stderr_thread.take() {
                    let _ = handle.join();
                }
                let tail = {
                    let log = wc.stderr_log.lock().unwrap();
                    let bytes: Vec<u8> = log.iter().cloned().collect();
                    String::from_utf8_lossy(&bytes)
                        .trim()
                        .chars()
                        .take(300)
                        .collect::<String>()
                };
                drop((stdin_back, stdout_back));
                ExchangeOutcome::Died(format!("exit={code:?} stderr={tail} ({detail})"))
            } else {
                // Still alive but framing desynced: unsafe to reuse.
                wc.stdin = Some(stdin_back);
                wc.stdout = Some(stdout_back);
                ExchangeOutcome::Protocol(detail)
            }
        }
        Err(_) => {
            // Timeout: kill; the I/O thread unblocks on process death and
            // drops the pipes. Fresh pipes arrive with the replacement.
            let _ = wc.child.kill();
            let _ = wc.child.wait();
            ExchangeOutcome::Timeout
        }
    }
}

impl PersistentPool {
    pub fn new(cfg: PersistentPoolConfig) -> Result<Self> {
        cfg.validate()?;
        let script_path = cfg
            .script_path
            .clone()
            .unwrap_or_else(materialize_worker_script);
        // Capacity is always `worker_count` slots. Fixed mode spawns all of
        // them; adaptive mode spawns `adaptive_min_workers` and parks the
        // rest as Stopped spares that later scale-ups can claim.
        let initial = if cfg.adaptive_enabled {
            cfg.adaptive_min_workers
        } else {
            cfg.worker_count
        };
        let mut slots = Vec::with_capacity(cfg.worker_count as usize);
        let mut first_errors: Vec<String> = Vec::new();
        for id in 0..cfg.worker_count {
            let mut slot = WorkerSlot::fresh(id as u64);
            if id >= initial {
                slot.state = WorkerState::Stopped;
                slots.push(Mutex::new(slot));
                continue;
            }
            match spawn_supervised(
                &cfg.php_bin,
                &script_path,
                cfg.startup_timeout,
                cfg.php_memory_limit.as_deref(),
            ) {
                Ok((wc, pid, startup_ms)) => {
                    slot.child = Some(wc);
                    slot.pid = Some(pid);
                    slot.incarnation = 1;
                    slot.started_at = Some(Instant::now());
                    slot.reset_incarnation_stats();
                    slot.startup_duration_ms = startup_ms;
                    slot.apply(WorkerEvent::HandshakeOk);
                }
                Err(e) => {
                    slot.consecutive_startup_failures = 1;
                    slot.exit_reason = Some(e.to_string());
                    if 1 > cfg.restart_max_retries {
                        slot.state = WorkerState::Failed;
                    } else {
                        slot.apply(WorkerEvent::HandshakeFailed);
                        slot.restart_not_before = Some(Instant::now() + cfg.backoff_for_retry(1));
                    }
                    first_errors.push(format!("worker {id}: {e}"));
                }
            }
            slots.push(Mutex::new(slot));
        }
        let ready = slots
            .iter()
            .filter(|s| s.lock().unwrap().state == WorkerState::Ready)
            .count();
        if ready == 0 {
            return Err(PhpxError::Runtime(format!(
                "persistent pool failed to start: 0/{} workers ready (startup_timeout={}s). Reasons: {}",
                cfg.worker_count,
                cfg.startup_timeout.as_secs(),
                first_errors.join("; ").chars().take(500).collect::<String>()
            )));
        }
        Ok(Self {
            inner: {
                let inner = Arc::new(Inner {
                    cfg,
                    script_path,
                    slots: slots.into_iter().collect(),
                    shared: Arc::new((Mutex::new(PoolShared::default()), Condvar::new())),
                    stop_sampler: std::sync::atomic::AtomicBool::new(false),
                });
                // Spawned capacity: everything not parked as a spare.
                // (Failed initial slots still occupy capacity; the demand
                // path retries them, and adaptive healing tops up.)
                let spawned = inner
                    .slots
                    .iter()
                    .filter(|m| m.lock().unwrap().state != WorkerState::Stopped)
                    .count();
                inner.shared.0.lock().unwrap().spawned = spawned;
                // P0-C guardian sampler (only when a limit is configured).
                if inner.cfg.memory_soft_bytes > 0 || inner.cfg.memory_hard_bytes > 0 {
                    let sampler = inner.clone();
                    std::thread::spawn(move || sampler_loop(sampler));
                }
                // P2 adaptive control loop (only when explicitly enabled).
                if inner.cfg.adaptive_enabled {
                    let ctl = inner.clone();
                    std::thread::spawn(move || adaptive_loop(ctl));
                }
                inner
            },
        })
    }

    pub fn config(&self) -> &PersistentPoolConfig {
        &self.inner.cfg
    }

    pub fn execution_mode(&self) -> &'static str {
        phpx_runtime::EXECUTION_MODE_PERSISTENT
    }

    /// Current queue load for access logs (active, waiting).
    pub fn load(&self) -> (usize, usize) {
        let sh = self.inner.shared.0.lock().unwrap();
        (sh.active, sh.waiting)
    }

    /// Live per-worker snapshots (metrics, CLI, tests).
    pub fn worker_snapshots(&self) -> Vec<WorkerSnapshot> {
        self.inner
            .slots
            .iter()
            .map(|m| {
                let s = m.lock().unwrap();
                WorkerSnapshot {
                    id: s.id,
                    incarnation: s.incarnation,
                    pid: s.pid,
                    state: s.state,
                    uptime_secs: s.uptime_secs(),
                    completed: s.completed,
                    restarts: s.restarts,
                    exit_reason: s.exit_reason.clone(),
                    rss_bytes: s.rss_bytes,
                    peak_rss_bytes: s.peak_rss_bytes,
                    startup_duration_ms: s.startup_duration_ms,
                    shutdown_duration_ms: s.shutdown_duration_ms,
                    last_ok_age_secs: s.last_ok_at.map(|t| t.elapsed().as_secs()),
                    healthy: s.healthy,
                    restarts_by_reason: s.restarts_by_reason.clone(),
                    soft_hits: s.soft_hits,
                    hard_hits: s.hard_hits,
                    last_recovery: s.recovery_snapshot(),
                }
            })
            .collect()
    }

    pub fn ready_count(&self) -> usize {
        self.inner
            .slots
            .iter()
            .filter(|m| m.lock().unwrap().state == WorkerState::Ready)
            .count()
    }

    /// Currently spawned workers (slots not parked as Stopped spares).
    /// Fixed mode: always `worker_count`. Adaptive mode: between
    /// `adaptive_min_workers` and `worker_count`.
    pub fn spawned_count(&self) -> usize {
        self.inner.shared.0.lock().unwrap().spawned
    }

    /// Whether P2 adaptive scaling is enabled on this pool.
    pub fn adaptive_enabled(&self) -> bool {
        self.inner.cfg.adaptive_enabled
    }

    fn record_rejected(&self) {
        self.inner.shared.0.lock().unwrap().rejected_overload += 1;
    }

    /// Collect terminal per-worker diagnostics for fail-fast errors.
    /// Non-blocking: a slot locked by in-flight I/O is skipped (it cannot
    /// be observed, and must never trigger a fail-fast guess). Failed slots
    /// never hold their lock across I/O, so they are always observable.
    fn failed_diagnostics(&self) -> Vec<String> {
        self.inner
            .slots
            .iter()
            .filter_map(|m| {
                let s = m.try_lock().ok()?;
                if s.state == WorkerState::Failed {
                    Some(format!(
                        "worker {}{}",
                        s.id,
                        s.exit_reason
                            .as_deref()
                            .map(|r| format!(": {r}"))
                            .unwrap_or_default()
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    /// True when a Restarting worker exists. Non-blocking: an unobservable
    /// (locked) slot counts as potentially recovering, so callers keep
    /// waiting instead of failing fast on a guess.
    fn any_restarting(&self) -> bool {
        self.inner.slots.iter().any(|m| match m.try_lock() {
            Ok(s) => s.state == WorkerState::Restarting,
            Err(_) => true,
        })
    }

    fn failed_error(&self, failed: Vec<String>) -> PhpxError {
        PhpxError::Runtime(format!(
            "persistent workers Failed, no capacity ({}). Recovery: fix php/worker script and restart gateway",
            failed.join("; ").chars().take(400).collect::<String>()
        ))
    }

    /// Claim a Ready worker, waiting boundedly. Returned guard is held for
    /// the whole exchange so drain/restart cannot race the I/O.
    fn claim_ready_slot(&self, deadline: Instant) -> Result<MutexGuard<'_, WorkerSlot>> {
        loop {
            // Fast path: any Ready worker.
            for m in &self.inner.slots {
                if let Ok(mut g) = m.try_lock() {
                    if g.state.can_accept_work() {
                        g.apply(WorkerEvent::Assign);
                        return Ok(g);
                    }
                }
            }
            // Demand-driven retry: one Restarting worker past its backoff.
            let now = Instant::now();
            if now < deadline {
                for m in &self.inner.slots {
                    let eligible = if let Ok(g) = m.try_lock() {
                        g.state == WorkerState::Restarting
                            && g.restart_not_before.map(|t| now >= t).unwrap_or(true)
                    } else {
                        false
                    };
                    if eligible {
                        // Re-lock blocking for the restart attempt.
                        let mut g2 = m.lock().unwrap();
                        let now2 = Instant::now();
                        if g2.state == WorkerState::Restarting
                            && g2.restart_not_before.map(|t| now2 >= t).unwrap_or(true)
                        {
                            self.restart_locked(
                                &mut g2,
                                "demand-driven retry".to_string(),
                                "demand-retry",
                            );
                            if g2.state.can_accept_work() {
                                g2.apply(WorkerEvent::Assign);
                                return Ok(g2);
                            }
                        }
                    }
                }
            }
            let now = Instant::now();
            if now >= deadline {
                // Surface terminal diagnostics when workers are Failed.
                let failed = self.failed_diagnostics();
                if !failed.is_empty() {
                    return Err(self.failed_error(failed));
                }
                return Err(PhpxError::Runtime(
                    "overload: persistent pool saturated, request rejected (backpressure)"
                        .to_string(),
                ));
            }
            // Fail fast: no Ready, nothing left that can recover (no
            // Restarting workers), and at least one Failed worker.
            if !self.any_restarting() {
                let failed = self.failed_diagnostics();
                if !failed.is_empty() {
                    return Err(self.failed_error(failed));
                }
            }
            // Re-check the deadline: the scans above never block, but a
            // condvar wake can land after expiry, and serving past the
            // queue bound would break backpressure. Never serve stale.
            if Instant::now() >= deadline {
                let failed = self.failed_diagnostics();
                if !failed.is_empty() {
                    return Err(self.failed_error(failed));
                }
                return Err(PhpxError::Runtime(
                    "overload: persistent pool saturated, request rejected (backpressure)"
                        .to_string(),
                ));
            }
            let (lock, cvar) = &*self.inner.shared;
            let guard = lock.lock().unwrap();
            if guard.draining {
                return Err(PhpxError::Runtime(
                    "draining: gateway is shutting down, request rejected (retry)".to_string(),
                ));
            }
            if let Some(until) = guard.circuit_open_until {
                if Instant::now() < until {
                    return Err(PhpxError::Runtime(
                        "overload: crash-loop circuit open, fast-reject (backpressure)".to_string(),
                    ));
                }
            }
            let wait = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::ZERO);
            let (g, _) = cvar.wait_timeout(guard, wait).unwrap();
            drop(g);
        }
    }

    /// Kill + respawn the worker in `slot`. Success requires a fresh
    /// handshake; otherwise the worker stays Restarting (budget remains) or
    /// goes Failed with a diagnostic reason (budget exhausted).
    /// `cause` is the short machine-readable restart reason recorded in
    /// `restarts_by_reason` and the `phpx_worker_restarts_total{reason}` metric.
    fn restart_locked(&self, slot: &mut WorkerSlot, reason: String, cause: &'static str) {
        if slot.exit_reason.is_none() {
            slot.exit_reason = Some(reason.clone());
        }
        slot.note_detected(cause);
        self.inner.respawn_locked(slot, cause);
        if slot.state == WorkerState::Ready {
            // Clear the stale failure reason on recovery.
            slot.exit_reason = None;
        }
        let (_, cvar) = &*self.inner.shared;
        cvar.notify_all();
    }
    pub fn execute(&self, req: &PhpRequest) -> Result<PhpResponse> {
        req.validate()?;
        {
            let (lock, _) = &*self.inner.shared;
            let sh = lock.lock().unwrap();
            if sh.draining {
                drop(sh);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "draining: gateway is shutting down, request rejected (retry)".to_string(),
                ));
            }
            if let Some(until) = sh.circuit_open_until {
                if Instant::now() < until {
                    drop(sh);
                    self.record_rejected();
                    return Err(PhpxError::Runtime(
                        "overload: crash-loop circuit open, fast-reject (backpressure)".to_string(),
                    ));
                }
            }
            if sh.waiting >= self.inner.cfg.queue_bound {
                drop(sh);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: persistent pool queue full, request rejected (backpressure)"
                        .to_string(),
                ));
            }
        }
        {
            let (lock, _) = &*self.inner.shared;
            lock.lock().unwrap().waiting += 1;
        }
        let deadline = Instant::now() + self.inner.cfg.queue_wait_timeout;
        let mut guard = match self.claim_ready_slot(deadline) {
            Ok(g) => g,
            Err(e) => {
                let (lock, cvar) = &*self.inner.shared;
                let mut sh = lock.lock().unwrap();
                sh.waiting = sh.waiting.saturating_sub(1);
                if e.to_string().contains("overload") || e.to_string().contains("circuit") {
                    sh.rejected_overload += 1;
                }
                cvar.notify_all();
                return Err(e);
            }
        };
        {
            let (lock, _) = &*self.inner.shared;
            let mut sh = lock.lock().unwrap();
            sh.waiting = sh.waiting.saturating_sub(1);
            sh.active += 1;
            // Expire a stale circuit while holding the lock.
            if let Some(until) = sh.circuit_open_until {
                if Instant::now() >= until {
                    sh.circuit_open_until = None;
                }
            }
        }

        let req_bytes = phpx_runtime::encode_frame(req)?;
        let outcome = match guard.child.as_mut() {
            Some(wc) => exchange(wc, &req_bytes, self.inner.cfg.execution_timeout),
            None => ExchangeOutcome::Protocol("worker has no process".to_string()),
        };

        // Post-exchange accounting (slot guard held; shared taken briefly).
        let response = match outcome {
            ExchangeOutcome::Responded(mut resp) => {
                if resp.request_id != req.request_id {
                    guard.apply(WorkerEvent::ProtocolError);
                    let reason = format!(
                        "request-id mismatch (want {}, got {})",
                        req.request_id,
                        resp.request_id.chars().take(60).collect::<String>()
                    );
                    self.restart_locked(&mut guard, reason, "protocol");
                    self.finish_request(false, false);
                    resp.status = 500;
                    resp.body = b"persistent worker protocol error".to_vec();
                    resp.outcome = phpx_runtime::WorkerOutcome::ProtocolError {
                        detail: "request-id mismatch".to_string(),
                    };
                    resp
                } else if resp.status == 200 {
                    guard.apply(WorkerEvent::CompleteOk);
                    guard.completed += 1;
                    guard.last_ok_at = Some(Instant::now());
                    self.finish_request(true, false);
                    // Recycle budget: graceful replacement, staggered.
                    if self.inner.cfg.max_requests_per_worker > 0
                        && guard.completed % self.inner.cfg.max_requests_per_worker == 0
                    {
                        self.inner.recycle_locked(&mut guard, "recycle-budget");
                    }
                    resp
                } else {
                    // App-level failure (404/500 with a live worker): count,
                    // keep the worker Ready.
                    guard.apply(WorkerEvent::CompleteFailed);
                    self.finish_request(false, false);
                    resp
                }
            }
            ExchangeOutcome::Timeout => {
                guard.apply(WorkerEvent::TimeoutDetected);
                // Child already killed by exchange(); restart now.
                // Mirror one-shot: timeout is an Ok(504), not a transport Err.
                self.restart_locked(&mut guard, "execution timeout".to_string(), "timeout");
                self.finish_request(false, true);
                PhpResponse {
                    request_id: req.request_id.clone(),
                    status: 504,
                    headers: Default::default(),
                    body: b"php execution timeout".to_vec(),
                    duration_ms: self.inner.cfg.execution_timeout.as_millis() as u64,
                    outcome: phpx_runtime::WorkerOutcome::Timeout,
                }
            }
            ExchangeOutcome::Died(reason) => {
                guard.apply(WorkerEvent::CrashDetected);
                self.restart_locked(&mut guard, format!("worker died: {reason}"), "crash");
                // A fast crash is a failure, not a timeout (timeouts bucket
                // is reserved for execution-timeout 504s).
                self.finish_request(false, false);
                PhpResponse {
                    request_id: req.request_id.clone(),
                    status: 500,
                    headers: Default::default(),
                    body: format!("php worker failed: {reason}")
                        .chars()
                        .take(500)
                        .collect::<String>()
                        .into_bytes(),
                    duration_ms: 0,
                    outcome: phpx_runtime::WorkerOutcome::Crashed { exit_code: None },
                }
            }
            ExchangeOutcome::Protocol(detail) => {
                guard.apply(WorkerEvent::ProtocolError);
                self.restart_locked(&mut guard, format!("protocol error: {detail}"), "protocol");
                // Protocol desync is a failure, not a timeout.
                self.finish_request(false, false);
                PhpResponse {
                    request_id: req.request_id.clone(),
                    status: 500,
                    headers: Default::default(),
                    body: b"persistent worker protocol error".to_vec(),
                    duration_ms: 0,
                    outcome: phpx_runtime::WorkerOutcome::ProtocolError { detail },
                }
            }
        };

        // P0-C: request completion is a guardian sampling point. The worker
        // is idle here, so soft (graceful recycle) / hard (protective
        // termination) policy applies synchronously before the next claim.
        if guard.state == WorkerState::Ready && self.inner.guardian_enabled() {
            self.inner.enforce_memory_locked(&mut guard);
        }

        {
            let (lock, cvar) = &*self.inner.shared;
            let mut sh = lock.lock().unwrap();
            sh.active = sh.active.saturating_sub(1);
            // Draining worker finished its last request.
            if guard.state == WorkerState::Draining {
                guard.apply(WorkerEvent::CompleteOk);
                if let Some(mut wc) = guard.child.take() {
                    let _ = wc.child.kill();
                    let _ = wc.child.wait();
                }
                guard.pid = None;
            }
            cvar.notify_all();
        }
        Ok(response)
    }

    /// Shared accounting: ok counts completion; failures feed the
    /// consecutive-failure circuit (rolling budget, reset on success).
    fn finish_request(&self, ok: bool, is_timeout: bool) {
        let (lock, _) = &*self.inner.shared;
        let mut sh = lock.lock().unwrap();
        if ok {
            sh.completed += 1;
            sh.consecutive_failures = 0;
            sh.circuit_open_until = None;
        } else {
            if is_timeout {
                sh.timeouts += 1;
            } else {
                sh.failed += 1;
            }
            sh.consecutive_failures += 1;
            let consec = sh.consecutive_failures;
            if self.inner.cfg.restart_max_retries == 0
                || consec >= self.inner.cfg.restart_max_retries as u64
            {
                let attempt = consec
                    .saturating_sub(self.inner.cfg.restart_max_retries as u64)
                    .min(10) as u32;
                let backoff = self.inner.cfg.backoff_for_retry(attempt);
                sh.circuit_open_until = Some(Instant::now() + backoff);
                sh.circuit_breaks += 1;
            }
        }
    }

    /// Begin graceful drain: stop intake; in-flight requests finish.
    pub fn begin_drain(&self) {
        let (lock, cvar) = &*self.inner.shared;
        let mut sh = lock.lock().unwrap();
        sh.draining = true;
        for m in &self.inner.slots {
            if let Ok(mut s) = m.try_lock() {
                if matches!(s.state, WorkerState::Ready | WorkerState::Busy) {
                    s.apply(WorkerEvent::DrainRequested);
                }
            }
        }
        cvar.notify_all();
    }

    /// Wait for in-flight work up to `timeout`, then terminate all workers.
    /// Returns true when the pool drained cleanly within the deadline.
    pub fn drain(&self, timeout: Duration) -> bool {
        self.begin_drain();
        let (lock, cvar) = &*self.inner.shared;
        let (guard, res) = cvar
            .wait_timeout_while(lock.lock().unwrap(), timeout, |s| s.active > 0)
            .unwrap();
        let clean = !res.timed_out() || guard.active == 0;
        drop(guard);
        for m in &self.inner.slots {
            if let Ok(mut s) = m.lock() {
                if let Some(mut wc) = s.child.take() {
                    let _ = wc.child.kill();
                    let _ = wc.child.wait();
                }
                s.pid = None;
                s.apply(WorkerEvent::Shutdown);
                // Force terminal even from Failed/Restarting.
                s.state = WorkerState::Stopped;
            }
        }
        let (lock, cvar) = &*self.inner.shared;
        lock.lock().unwrap().active = 0;
        cvar.notify_all();
        clean
    }

    /// Prometheus exposition: mode info + pool aggregates + per-worker series.
    pub fn prometheus_text(&self) -> String {
        let snaps = self.worker_snapshots();
        let sh = self.inner.shared.0.lock().unwrap();
        let mut out = format!(
            "# HELP phpx_execution_mode_info Selected execution mode (P0-B: persistent supervised pool).\n# TYPE phpx_execution_mode_info gauge\nphpx_execution_mode_info{{execution_mode=\"persistent\",protocol_version=\"{}\"}} 1\n# HELP phpx_pool_completed_total Completed PHP executions.\n# TYPE phpx_pool_completed_total counter\nphpx_pool_completed_total {}\n# HELP phpx_pool_failed_total Failed PHP executions (non-200).\n# TYPE phpx_pool_failed_total counter\nphpx_pool_failed_total {}\n# HELP phpx_pool_timeouts_total Execution timeouts (HTTP 504).\n# TYPE phpx_pool_timeouts_total counter\nphpx_pool_timeouts_total {}\n# HELP phpx_pool_rejected_overload_total Rejected by backpressure (HTTP 503).\n# TYPE phpx_pool_rejected_overload_total counter\nphpx_pool_rejected_overload_total {}\n# HELP phpx_pool_recycled_total Graceful worker replacements.\n# TYPE phpx_pool_recycled_total counter\nphpx_pool_recycled_total {}\n# HELP phpx_pool_restarts_total Worker restarts after failures.\n# TYPE phpx_pool_restarts_total counter\nphpx_pool_restarts_total {}\n# HELP phpx_pool_circuit_breaks_total Crash-loop circuit openings.\n# TYPE phpx_pool_circuit_breaks_total counter\nphpx_pool_circuit_breaks_total {}\n# HELP phpx_pool_active Currently executing requests.\n# TYPE phpx_pool_active gauge\nphpx_pool_active {}\n# HELP phpx_pool_waiting Currently queued requests.\n# TYPE phpx_pool_waiting gauge\nphpx_pool_waiting {}\n# HELP phpx_pool_max_workers Configured fixed worker count.\n# TYPE phpx_pool_max_workers gauge\nphpx_pool_max_workers {}\n# HELP phpx_pool_min_workers Configured worker count floor (fixed pool size, or adaptive floor from min_workers).\n# TYPE phpx_pool_min_workers gauge\nphpx_pool_min_workers {}\n# HELP phpx_pool_workers_current Currently spawned workers (adaptive spares excluded; fixed mode always equals max).\n# TYPE phpx_pool_workers_current gauge\nphpx_pool_workers_current {}\n# HELP phpx_pool_adaptive_enabled Adaptive scaling enabled (1) or fixed pool (0).\n# TYPE phpx_pool_adaptive_enabled gauge\nphpx_pool_adaptive_enabled {}\n# HELP phpx_pool_adaptive_scale_events_total Scale actions by direction (up = spare claimed and spawned, down = idle worker retired).\n# TYPE phpx_pool_adaptive_scale_events_total counter\nphpx_pool_adaptive_scale_events_total{{direction=\"up\"}} {}\nphpx_pool_adaptive_scale_events_total{{direction=\"down\"}} {}\n",
            PROTOCOL_VERSION,
            sh.completed,
            sh.failed,
            sh.timeouts,
            sh.rejected_overload,
            sh.recycled,
            sh.restarts,
            sh.circuit_breaks,
            sh.active,
            sh.waiting,
            self.inner.cfg.worker_count,
            self.inner.cfg.worker_count,
            sh.spawned,
            u64::from(self.inner.cfg.adaptive_enabled),
            sh.scale_ups,
            sh.scale_downs,
        );
        out.push_str(
            "# HELP phpx_worker_state Supervised worker state (0=starting,1=ready,2=busy,3=draining,4=stopped,5=unhealthy,6=restarting,7=failed).\n# TYPE phpx_worker_state gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_state{{worker_id=\"{}\"}} {}\n",
                s.id,
                s.state.as_metric_value()
            ));
        }
        out.push_str(
            "# HELP phpx_worker_requests_total Completed requests per worker.\n# TYPE phpx_worker_requests_total counter\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_requests_total{{worker_id=\"{}\"}} {}\n",
                s.id, s.completed
            ));
        }
        out.push_str(
            "# HELP phpx_worker_restarts_total Restarts per worker incarnation chain.\n# TYPE phpx_worker_restarts_total counter\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_restarts_total{{worker_id=\"{}\"}} {}\n",
                s.id, s.restarts
            ));
        }
        out.push_str(
            "# HELP phpx_worker_uptime_seconds Worker process uptime.\n# TYPE phpx_worker_uptime_seconds gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_uptime_seconds{{worker_id=\"{}\"}} {}\n",
                s.id, s.uptime_secs
            ));
        }
        // P0-C guardian series.
        out.push_str(
            "# HELP phpx_worker_rss_bytes Last sampled OS resident set size (0 = never sampled). RSS is not the PHP heap; see memory docs.\n# TYPE phpx_worker_rss_bytes gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_rss_bytes{{worker_id=\"{}\"}} {}\n",
                s.id, s.rss_bytes
            ));
        }
        out.push_str(
            "# HELP phpx_worker_peak_rss_bytes Max sampled RSS over the current incarnation.\n# TYPE phpx_worker_peak_rss_bytes gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_peak_rss_bytes{{worker_id=\"{}\"}} {}\n",
                s.id, s.peak_rss_bytes
            ));
        }
        out.push_str(
            "# HELP phpx_worker_startup_duration_seconds Last successful spawn+handshake duration.\n# TYPE phpx_worker_startup_duration_seconds gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_startup_duration_seconds{{worker_id=\"{}\"}} {}\n",
                s.id,
                s.startup_duration_ms as f64 / 1000.0
            ));
        }
        out.push_str(
            "# HELP phpx_worker_shutdown_duration_seconds Last kill+wait duration.\n# TYPE phpx_worker_shutdown_duration_seconds gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_shutdown_duration_seconds{{worker_id=\"{}\"}} {}\n",
                s.id,
                s.shutdown_duration_ms as f64 / 1000.0
            ));
        }
        out.push_str(
            "# HELP phpx_worker_healthy Sampler verdict (1 = alive and under limits, 0 = over limit or unsampleable).\n# TYPE phpx_worker_healthy gauge\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_healthy{{worker_id=\"{}\"}} {}\n",
                s.id,
                u64::from(s.healthy)
            ));
        }
        out.push_str(
            "# HELP phpx_worker_restarts_by_reason_total Restarts per worker by cause.\n# TYPE phpx_worker_restarts_by_reason_total counter\n",
        );
        for s in &snaps {
            for (cause, count) in &s.restarts_by_reason {
                out.push_str(&format!(
                    "phpx_worker_restarts_by_reason_total{{worker_id=\"{}\",reason=\"{}\"}} {}\n",
                    s.id, cause, count
                ));
            }
        }
        out.push_str(
            "# HELP phpx_worker_memory_limit_hits_total Guardian trigger counts (threshold=soft|hard). Sum across workers for pool totals.\n# TYPE phpx_worker_memory_limit_hits_total counter\n",
        );
        for s in &snaps {
            out.push_str(&format!(
                "phpx_worker_memory_limit_hits_total{{worker_id=\"{}\",threshold=\"soft\"}} {}\n",
                s.id, s.soft_hits
            ));
            out.push_str(&format!(
                "phpx_worker_memory_limit_hits_total{{worker_id=\"{}\",threshold=\"hard\"}} {}\n",
                s.id, s.hard_hits
            ));
        }
        out.push_str(
            "# HELP phpx_worker_last_recovery_duration_seconds Detection-to-ready duration of the last unplanned restart (0 = none yet).\n# TYPE phpx_worker_last_recovery_duration_seconds gauge\n",
        );
        for s in &snaps {
            let v = s
                .last_recovery
                .as_ref()
                .and_then(|r| r.total_ms)
                .unwrap_or(0) as f64
                / 1000.0;
            out.push_str(&format!(
                "phpx_worker_last_recovery_duration_seconds{{worker_id=\"{}\"}} {}\n",
                s.id, v
            ));
        }
        out
    }
}

impl Inner {
    fn guardian_enabled(&self) -> bool {
        self.cfg.memory_soft_bytes > 0 || self.cfg.memory_hard_bytes > 0
    }

    /// P0-C guardian sweep: sample every idle (Ready) worker. Busy workers
    /// are skipped without blocking (request completion samples them
    /// instead). Never panics; a dead sampler must not kill the gateway.
    fn sweep_guardian(&self) {
        for m in &self.slots {
            let Ok(mut s) = m.try_lock() else { continue };
            if s.state == WorkerState::Ready {
                self.enforce_memory_locked(&mut s);
            }
        }
    }

    /// Shared kill+respawn used by crash/timeout/protocol recovery and the
    /// hard memory limit. Measures shutdown duration; success bumps the
    /// incarnation and records the restart cause.
    fn respawn_locked(&self, slot: &mut WorkerSlot, cause: &'static str) {
        slot.note_restart_started();
        if let Some(mut wc) = slot.child.take() {
            let t0 = Instant::now();
            let _ = wc.child.kill();
            let _ = wc.child.wait();
            slot.shutdown_duration_ms = t0.elapsed().as_millis() as u64;
            slot.pid = None;
        }
        match spawn_supervised(
            &self.cfg.php_bin,
            &self.script_path,
            self.cfg.startup_timeout,
            self.cfg.php_memory_limit.as_deref(),
        ) {
            Ok((wc, pid, startup_ms)) => {
                slot.child = Some(wc);
                slot.pid = Some(pid);
                slot.incarnation += 1;
                slot.started_at = Some(Instant::now());
                slot.consecutive_startup_failures = 0;
                slot.restart_not_before = None;
                slot.reset_incarnation_stats();
                slot.startup_duration_ms = startup_ms;
                slot.note_restart(cause);
                slot.note_restart_ready();
                self.shared.0.lock().unwrap().restarts += 1;
                if slot.state == WorkerState::Unhealthy {
                    slot.apply(WorkerEvent::RestartOk);
                } else {
                    slot.apply(WorkerEvent::HandshakeOk);
                }
            }
            Err(e) => {
                slot.consecutive_startup_failures += 1;
                let n = slot.consecutive_startup_failures;
                slot.exit_reason = Some(format!("restart failed ({n} consecutive): {e}"));
                if n > self.cfg.restart_max_retries as u64 {
                    slot.state = WorkerState::Failed;
                } else {
                    if slot.state == WorkerState::Unhealthy {
                        slot.apply(WorkerEvent::RestartFailed);
                    } else {
                        slot.apply(WorkerEvent::HandshakeFailed);
                    }
                    slot.restart_not_before =
                        Some(Instant::now() + self.cfg.backoff_for_retry(n.min(10) as u32));
                }
            }
        }
    }

    /// Graceful recycle: spawn the replacement first; swap only on a
    /// successful handshake. Only one replacement at a time (staggered);
    /// others defer to a later request. The shared lock is released during
    /// the spawn so admissions keep flowing. `cause` is recorded as the
    /// restart reason ("recycle-budget" or "soft-limit").
    fn recycle_locked(&self, slot: &mut WorkerSlot, cause: &'static str) {
        {
            let (lock, _) = &*self.shared;
            let mut sh = lock.lock().unwrap();
            if sh.recycling {
                return;
            }
            sh.recycling = true;
        }
        let attempt = spawn_supervised(
            &self.cfg.php_bin,
            &self.script_path,
            self.cfg.startup_timeout,
            self.cfg.php_memory_limit.as_deref(),
        );
        {
            let (lock, _) = &*self.shared;
            let mut sh = lock.lock().unwrap();
            match attempt {
                Ok((wc, pid, startup_ms)) => {
                    if let Some(mut old) = slot.child.take() {
                        let t0 = Instant::now();
                        let _ = old.child.kill();
                        let _ = old.child.wait();
                        slot.shutdown_duration_ms = t0.elapsed().as_millis() as u64;
                    }
                    slot.child = Some(wc);
                    slot.pid = Some(pid);
                    slot.incarnation += 1;
                    slot.started_at = Some(Instant::now());
                    slot.completed = 0;
                    slot.reset_incarnation_stats();
                    slot.startup_duration_ms = startup_ms;
                    slot.note_restart(cause);
                    sh.recycled += 1;
                    sh.restarts += 1;
                }
                Err(e) => {
                    // Keep serving on the current worker; retry next budget hit.
                    slot.exit_reason = Some(format!("recycle deferred: {e}"));
                }
            }
            sh.recycling = false;
        }
    }

    /// P2 adaptive scale-up: claim one Stopped spare and spawn into it.
    /// The spare is marked Starting under its lock first, so the (slow)
    /// spawn runs unlocked and admissions/metrics keep flowing. On spawn
    /// failure the spare parks back to Stopped with a diagnostic; the
    /// cooldown (set on every attempt) prevents a hot spawn loop.
    fn adaptive_scale_up(&self) -> bool {
        let reserved: Option<usize> =
            (0..self.slots.len()).find(|&i| match self.slots[i].try_lock() {
                Ok(mut s) => {
                    if s.state == WorkerState::Stopped && s.child.is_none() {
                        s.state = WorkerState::Starting;
                        s.consecutive_startup_failures = 0;
                        s.exit_reason = None;
                        true
                    } else {
                        false
                    }
                }
                Err(_) => false,
            });
        let Some(idx) = reserved else {
            return false;
        };
        let attempt = spawn_supervised(
            &self.cfg.php_bin,
            &self.script_path,
            self.cfg.startup_timeout,
            self.cfg.php_memory_limit.as_deref(),
        );
        let mut s = self.slots[idx].lock().unwrap();
        if s.state != WorkerState::Starting || s.child.is_some() {
            // Lost a race (drain/teardown touched the spare): no orphaned
            // processes — reap the stray child if we spawned one.
            drop(s);
            if let Ok((mut wc, _, _)) = attempt {
                let _ = wc.child.kill();
                let _ = wc.child.wait();
            }
            return false;
        }
        match attempt {
            Ok((wc, pid, startup_ms)) => {
                s.child = Some(wc);
                s.pid = Some(pid);
                s.incarnation += 1;
                s.started_at = Some(Instant::now());
                s.completed = 0;
                s.reset_incarnation_stats();
                s.startup_duration_ms = startup_ms;
                s.apply(WorkerEvent::HandshakeOk);
                let (lock, cvar) = &*self.shared;
                let mut sh = lock.lock().unwrap();
                sh.spawned += 1;
                sh.scale_ups += 1;
                sh.last_scale_at = Some(Instant::now());
                sh.idle_streak = 0;
                drop(sh);
                cvar.notify_all();
                true
            }
            Err(e) => {
                s.state = WorkerState::Stopped;
                s.exit_reason = Some(format!("adaptive scale-up deferred: {e}"));
                let (lock, _) = &*self.shared;
                lock.lock().unwrap().last_scale_at = Some(Instant::now());
                false
            }
        }
    }

    /// P2 adaptive scale-down: retire one idle (Ready) worker to a Stopped
    /// spare, preferring the highest id so low ids stay stable for metrics.
    /// The slot lock is held from the Ready re-check through the kill, so no
    /// claim can interleave (claims need Assign under the same lock).
    fn adaptive_scale_down(&self) -> bool {
        for m in self.slots.iter().rev() {
            let Ok(mut s) = m.try_lock() else {
                continue;
            };
            if s.state != WorkerState::Ready {
                continue;
            }
            if let Some(mut wc) = s.child.take() {
                let t0 = Instant::now();
                let _ = wc.child.kill();
                let _ = wc.child.wait();
                s.shutdown_duration_ms = t0.elapsed().as_millis() as u64;
            }
            s.pid = None;
            s.apply(WorkerEvent::Shutdown);
            s.exit_reason = None;
            let (lock, cvar) = &*self.shared;
            let mut sh = lock.lock().unwrap();
            sh.spawned = sh.spawned.saturating_sub(1);
            sh.scale_downs += 1;
            sh.last_scale_at = Some(Instant::now());
            sh.idle_streak = 0;
            drop(sh);
            cvar.notify_all();
            return true;
        }
        false
    }

    /// P2 adaptive control evaluation: snapshot signals, decide, act.
    /// Never blocks on slot locks (try_lock only); a contested view simply
    /// defers action to the next interval.
    fn adaptive_tick(&self) {
        let (waiting, active, spawned, idle_streak, memory_pressure, blocked) = {
            let (lock, _) = &*self.shared;
            let sh = lock.lock().unwrap();
            if sh.draining {
                return;
            }
            let circuit_open = sh
                .circuit_open_until
                .map(|t| Instant::now() < t)
                .unwrap_or(false);
            let cooldown_ok = sh
                .last_scale_at
                .map(|t| t.elapsed() >= self.cfg.adaptive_cooldown)
                .unwrap_or(true);
            (
                sh.waiting,
                sh.active,
                sh.spawned,
                sh.idle_streak,
                self.cfg.adaptive_memory_guard && sh.memory_pressure,
                circuit_open || !cooldown_ok,
            )
        };
        let all_idle = waiting == 0 && active == 0;
        let sig = AdaptiveSignal {
            waiting,
            active,
            current: spawned,
            idle_streak,
            memory_pressure,
            blocked,
        };
        let tun = AdaptiveTuning {
            min_w: self.cfg.adaptive_min_workers as usize,
            max_w: self.cfg.worker_count as usize,
            up_depth: self.cfg.adaptive_scale_up_queue_depth,
            down_intervals: self.cfg.adaptive_scale_down_idle_intervals,
        };
        match adaptive_decide(sig, tun) {
            AdaptiveAction::Hold => {
                let (lock, _) = &*self.shared;
                let mut sh = lock.lock().unwrap();
                if all_idle {
                    sh.idle_streak = sh.idle_streak.saturating_add(1);
                } else {
                    sh.idle_streak = 0;
                }
            }
            AdaptiveAction::ScaleUp => {
                self.adaptive_scale_up();
            }
            AdaptiveAction::ScaleDown => {
                self.adaptive_scale_down();
            }
        }
    }
    /// P0-C: sample one worker's OS RSS and enforce soft/hard policy.
    /// Called with the slot guard held, when the worker is idle (Ready):
    /// from request completion and from the sampler thread.
    /// - Below: update observations, mark healthy.
    /// - Soft: count a soft hit, graceful recycle (replacement first).
    /// - Hard: count a hard hit, protective termination + respawn.
    fn enforce_memory_locked(&self, slot: &mut WorkerSlot) {
        let Some(pid) = slot.pid else { return };
        let Some(sample) = crate::memory::sample_rss(pid) else {
            // Process gone or unsampleable: leave it; the exchange path or
            // the next handshake will surface the failure. Never guess.
            slot.healthy = false;
            return;
        };
        let now = Instant::now();
        slot.rss_bytes = sample.rss_bytes;
        slot.peak_rss_bytes = slot.peak_rss_bytes.max(sample.rss_bytes);
        slot.last_sample_at = Some(now);
        match crate::memory::classify(
            sample.rss_bytes,
            self.cfg.memory_soft_bytes,
            self.cfg.memory_hard_bytes,
        ) {
            crate::memory::MemoryClass::Below => {
                slot.healthy = true;
                slot.last_ok_at = Some(now);
                self.shared.0.lock().unwrap().memory_pressure = false;
            }
            crate::memory::MemoryClass::Soft => {
                slot.soft_hits += 1;
                slot.healthy = false;
                self.shared.0.lock().unwrap().memory_pressure = true;
                let pid_before = slot.pid;
                self.recycle_locked(slot, "soft-limit");
                // recycle_locked resets incarnation stats on success; when
                // it defers (spawn failed), the stale exit_reason explains why.
                if slot.state == WorkerState::Ready && slot.pid != pid_before {
                    slot.exit_reason = None;
                }
            }
            crate::memory::MemoryClass::Hard => {
                slot.hard_hits += 1;
                slot.healthy = false;
                self.shared.0.lock().unwrap().memory_pressure = true;
                let pid_before = slot.pid;
                let reason = format!(
                    "hard memory limit: rss={} >= hard={}",
                    sample.rss_bytes, self.cfg.memory_hard_bytes
                );
                slot.note_detected("hard-limit");
                self.respawn_locked(slot, "hard-limit");
                if slot.state == WorkerState::Ready && slot.pid != pid_before {
                    slot.exit_reason = None;
                } else if slot.exit_reason.is_none() {
                    slot.exit_reason = Some(reason);
                }
            }
        }
    }
}

/// P2 adaptive control loop: one evaluation per `adaptive_interval`.
/// Sleeps in small chunks so pool teardown stops the thread promptly
/// (shares `stop_sampler` with the guardian loop). Never touches Busy
/// workers; scale-down only retires Ready slots under their own lock.
fn adaptive_loop(inner: Arc<Inner>) {
    use std::sync::atomic::Ordering;
    let interval = inner.cfg.adaptive_interval.max(Duration::from_secs(1));
    let step = Duration::from_millis(250);
    let mut slept = Duration::ZERO;
    loop {
        if inner.stop_sampler.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(step.min(interval.saturating_sub(slept).max(Duration::from_millis(1))));
        slept += step;
        if slept < interval {
            continue;
        }
        slept = Duration::ZERO;
        inner.adaptive_tick();
    }
}

/// P0-C guardian sampler: every `memory_sample_interval`, sweep idle workers.
/// Sleeps in small chunks so pool teardown stops the thread promptly.
/// Never touches Busy workers; request completion samples those instead.
fn sampler_loop(inner: Arc<Inner>) {
    use std::sync::atomic::Ordering;
    let interval = inner.cfg.memory_sample_interval.max(Duration::from_secs(1));
    let step = Duration::from_millis(250);
    let mut slept = Duration::ZERO;
    loop {
        if inner.stop_sampler.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(step.min(interval.saturating_sub(slept).max(Duration::from_millis(1))));
        slept += step;
        if slept < interval {
            continue;
        }
        slept = Duration::ZERO;
        inner.sweep_guardian();
    }
}

impl Drop for PersistentPool {
    fn drop(&mut self) {
        // Stop the sampler first so it cannot respawn while we reap.
        self.inner
            .stop_sampler
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // Best-effort: never orphan php processes on pool teardown.
        for m in &self.inner.slots {
            if let Ok(mut s) = m.try_lock() {
                if let Some(mut wc) = s.child.take() {
                    let _ = wc.child.kill();
                    let _ = wc.child.wait();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_transitions_are_deterministic() {
        use WorkerEvent::*;
        use WorkerState::*;
        // Happy path.
        assert_eq!(Starting.transition(HandshakeOk), Some(Ready));
        assert_eq!(Ready.transition(Assign), Some(Busy));
        assert_eq!(Busy.transition(CompleteOk), Some(Ready));
        // Failure path.
        assert_eq!(Busy.transition(CrashDetected), Some(Unhealthy));
        assert_eq!(Unhealthy.transition(RestartOk), Some(Ready));
        assert_eq!(Unhealthy.transition(RestartFailed), Some(Restarting));
        assert_eq!(Restarting.transition(HandshakeOk), Some(Ready));
        assert_eq!(Restarting.transition(HandshakeFailed), Some(Restarting));
        // Drain path.
        assert_eq!(Ready.transition(DrainRequested), Some(Draining));
        assert_eq!(Busy.transition(DrainRequested), Some(Draining));
        assert_eq!(Draining.transition(CompleteOk), Some(Stopped));
        // Illegal: never assign without a handshake.
        assert_eq!(Restarting.transition(Assign), None);
        assert_eq!(Starting.transition(Assign), None);
        assert_eq!(Failed.transition(Assign), None);
        assert_eq!(Stopped.transition(Assign), None);
    }

    #[test]
    fn failed_is_terminal_except_shutdown() {
        use WorkerEvent::*;
        use WorkerState::*;
        for ev in [
            HandshakeOk,
            HandshakeFailed,
            Assign,
            CompleteOk,
            CompleteFailed,
            CrashDetected,
            TimeoutDetected,
            ProtocolError,
            DrainRequested,
            RestartOk,
            RestartFailed,
        ] {
            assert_eq!(
                Failed.transition(ev),
                None,
                "Failed + {ev:?} must be terminal"
            );
        }
        assert_eq!(Failed.transition(Shutdown), Some(Stopped));
    }

    #[test]
    fn backoff_grows_and_caps() {
        let cfg = PersistentPoolConfig::default();
        let b0 = cfg.backoff_for_retry(0);
        let b3 = cfg.backoff_for_retry(3);
        let b30 = cfg.backoff_for_retry(30);
        assert!(b3 > b0);
        assert!(b30 <= cfg.restart_backoff_max * 2);
    }

    /// P2 adaptive control law (pure decision, no pool needed).
    /// Floor 1, ceiling 4, up_depth 2, down_intervals 3.
    fn decide(
        waiting: usize,
        active: usize,
        current: usize,
        idle_streak: u32,
        memory_pressure: bool,
        blocked: bool,
    ) -> AdaptiveAction {
        adaptive_decide(
            AdaptiveSignal {
                waiting,
                active,
                current,
                idle_streak,
                memory_pressure,
                blocked,
            },
            AdaptiveTuning {
                min_w: 1,
                max_w: 4,
                up_depth: 2,
                down_intervals: 3,
            },
        )
    }

    #[test]
    fn adaptive_scales_up_only_when_saturated_and_queued() {
        use AdaptiveAction::*;
        // 1/1 busy + 2 queued -> up.
        assert_eq!(decide(2, 1, 1, 0, false, false), ScaleUp);
        // Below the queue-depth trigger -> hold.
        assert_eq!(decide(1, 1, 1, 0, false, false), Hold);
        // Spare capacity idle (a Ready worker would claim first) -> hold.
        assert_eq!(decide(2, 0, 2, 0, false, false), Hold);
        // At the ceiling -> hold even under pressure.
        assert_eq!(decide(9, 4, 4, 0, false, false), Hold);
        // Memory pressure blocks scale-up (guardian is already recycling).
        assert_eq!(decide(9, 2, 2, 0, true, false), Hold);
        // Cooldown / circuit / drain (blocked) -> hold.
        assert_eq!(decide(9, 2, 2, 0, false, true), Hold);
    }

    #[test]
    fn adaptive_scales_down_only_after_idle_hysteresis() {
        use AdaptiveAction::*;
        // Three straight all-idle evals: hold, hold, down.
        assert_eq!(decide(0, 0, 3, 0, false, false), Hold);
        assert_eq!(decide(0, 0, 3, 1, false, false), Hold);
        assert_eq!(decide(0, 0, 3, 2, false, false), ScaleDown);
        // Any load resets the case -> hold.
        assert_eq!(decide(1, 0, 3, 2, false, false), Hold);
        assert_eq!(decide(0, 1, 3, 2, false, false), Hold);
        // At the floor -> hold even when idle.
        assert_eq!(decide(0, 0, 1, 9, false, false), Hold);
        // Blocked (cooldown/circuit/drain) -> hold.
        assert_eq!(decide(0, 0, 3, 9, false, true), Hold);
        // Memory pressure does NOT block scale-down.
        assert_eq!(decide(0, 0, 3, 9, true, false), ScaleDown);
    }

    #[test]
    fn adaptive_heals_toward_configured_range() {
        use AdaptiveAction::*;
        // Below the floor -> up (unless blocked).
        assert_eq!(decide(0, 0, 0, 0, false, false), ScaleUp);
        assert_eq!(decide(0, 0, 0, 0, true, false), ScaleUp);
        assert_eq!(decide(0, 0, 0, 0, false, true), Hold);
        // Above the ceiling -> down.
        assert_eq!(decide(0, 0, 9, 0, false, false), ScaleDown);
    }

    #[test]
    fn rejects_bad_pool_config() {
        let bad = PersistentPoolConfig {
            worker_count: 0,
            ..Default::default()
        };
        assert!(bad.validate().is_err());
        let bad_q = PersistentPoolConfig {
            queue_bound: 0,
            ..Default::default()
        };
        assert!(bad_q.validate().is_err());
    }
}
