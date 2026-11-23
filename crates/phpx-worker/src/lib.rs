use phpx_core::{PhpxError, Result};
use phpx_runtime::{PhpRequest, PhpResponse};
use phpx_runtime_process::{ExecuteOptions, PhpDiscovery};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub mod memory;
pub mod persistent;
pub use memory::{classify, sample_rss, MemoryClass, MemorySample};
pub use persistent::{
    PersistentPool, PersistentPoolConfig, WorkerEvent, WorkerSnapshot, WorkerState,
};

/// M4 hardened pool config: min/max, bounded queue, timeouts, drain, restart policy.
/// P0-A: `execution_mode` is explicit. Only `"one-shot"` runs; `"persistent"`
/// is rejected until the P0-B supervised pool exists.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_workers: u32,
    pub min_workers: u32,
    pub execution_mode: String,
    /// PHP-level heap limit (`-d memory_limit`) applied to every one-shot
    /// child. None = php default. This is a PHP-heap limit, NOT an OS RSS
    /// limit (one-shot processes are short-lived; the RSS guardian is
    /// persistent-only per ADR-0014 Phase B).
    pub php_memory_limit: Option<String>,
    pub queue_bound: usize,
    pub queue_wait_timeout: Duration,
    pub execution_timeout: Duration,
    pub drain_timeout: Duration,
    pub max_requests_per_slot: u64,
    pub restart_max_retries: u32,
    pub restart_backoff_base: Duration,
    pub restart_backoff_max: Duration,
    pub php_bin: String,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_workers: 4,
            min_workers: 1,
            execution_mode: phpx_runtime::EXECUTION_MODE_ONE_SHOT.to_string(),
            php_memory_limit: None,
            queue_bound: 16,
            queue_wait_timeout: Duration::from_secs(2),
            execution_timeout: Duration::from_secs(30),
            drain_timeout: Duration::from_secs(10),
            max_requests_per_slot: 1000,
            restart_max_retries: 5,
            restart_backoff_base: Duration::from_millis(250),
            restart_backoff_max: Duration::from_secs(30),
            php_bin: std::env::var("PHPX_PHP_BIN").unwrap_or_else(|_| "php".to_string()),
        }
    }
}

impl PoolConfig {
    /// Canonical execution mode. Defaults to one-shot when unset/blank.
    pub fn resolved_execution_mode(&self) -> &'static str {
        match phpx_runtime::ExecutionMode::parse(&self.execution_mode) {
            Some(phpx_runtime::ExecutionMode::Persistent) => {
                phpx_runtime::EXECUTION_MODE_PERSISTENT
            }
            _ => phpx_runtime::EXECUTION_MODE_ONE_SHOT,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match phpx_runtime::ExecutionMode::parse(&self.execution_mode) {
            Some(phpx_runtime::ExecutionMode::OneShot) => {}
            Some(phpx_runtime::ExecutionMode::Persistent) => {
                return Err(PhpxError::Config(
                    "execution_mode=persistent needs PersistentPool, not BoundedPool (one-shot pool rejects persistent work)"
                        .to_string(),
                ));
            }
            None => {
                return Err(PhpxError::Config(format!(
                    "execution_mode must be one-shot or persistent, got {}",
                    self.execution_mode
                )));
            }
        }
        if self.max_workers == 0 || self.max_workers > 1024 {
            return Err(PhpxError::Config(
                "max_workers must be 1-1024 (bounded, no unbounded pool)".to_string(),
            ));
        }
        if self.min_workers == 0 || self.min_workers > 1024 {
            return Err(PhpxError::Config("min_workers must be 1-1024".to_string()));
        }
        if self.min_workers > self.max_workers {
            return Err(PhpxError::Config(format!(
                "min_workers ({}) must be <= max_workers ({})",
                self.min_workers, self.max_workers
            )));
        }
        if self.queue_bound == 0 || self.queue_bound > 100_000 {
            return Err(PhpxError::Config(
                "queue_bound must be 1-100000 (bounded, backpressure mandatory)".to_string(),
            ));
        }
        if self.max_requests_per_slot == 0 {
            return Err(PhpxError::Config(
                "max_requests_per_slot must be >= 1".to_string(),
            ));
        }
        Ok(())
    }

    /// Exponential backoff with deterministic jitter for retry attempt n (0-based).
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

/// M4 pool stats: counters + live load + crash-loop state.
#[derive(Debug, Default, Clone)]
pub struct PoolStats {
    pub completed: u64,
    pub failed: u64,
    pub timeouts: u64,
    pub rejected_overload: u64,
    pub recycled: u64,
    pub restarts: u64,
    pub circuit_breaks: u64,
    pub consecutive_failures: u64,
    pub active: usize,
    pub waiting: usize,
}

struct PoolState {
    active: usize,
    waiting: usize,
    draining: bool,
    consecutive_failures: u64,
    circuit_open_until: Option<Instant>,
    circuit_breaks: u64,
    completed: u64,
    recycled: u64,
}

/// Bounded worker pool over one-shot PHP processes (M1) hardened in M4.
/// P0-A: execution mode is explicit (`execution_mode`, default one-shot);
/// persistent workers belong to PersistentPool and are rejected here by
/// `PoolConfig::validate`.
pub struct BoundedPool {
    cfg: PoolConfig,
    discovery: PhpDiscovery,
    state: Arc<(Mutex<PoolState>, Condvar)>,
    stats: Arc<Mutex<PoolStats>>,
}

impl BoundedPool {
    pub fn new(cfg: PoolConfig) -> Result<Self> {
        cfg.validate()?;
        Ok(Self {
            discovery: PhpDiscovery {
                php_bin: cfg.php_bin.clone(),
            },
            cfg,
            state: Arc::new((
                Mutex::new(PoolState {
                    active: 0,
                    waiting: 0,
                    draining: false,
                    consecutive_failures: 0,
                    circuit_open_until: None,
                    circuit_breaks: 0,
                    completed: 0,
                    recycled: 0,
                }),
                Condvar::new(),
            )),
            stats: Arc::new(Mutex::new(PoolStats::default())),
        })
    }

    pub fn config(&self) -> &PoolConfig {
        &self.cfg
    }

    pub fn stats(&self) -> PoolStats {
        self.stats.lock().unwrap().clone()
    }

    /// Point-in-time snapshot including live active/waiting + failure state.
    pub fn snapshot(&self) -> PoolStats {
        let (lock, _) = &*self.state;
        let st = lock.lock().unwrap();
        let mut s = self.stats.lock().unwrap().clone();
        s.active = st.active;
        s.waiting = st.waiting;
        s.consecutive_failures = st.consecutive_failures;
        s.circuit_breaks = st.circuit_breaks;
        s.recycled = st.recycled;
        s
    }

    fn record_rejected(&self) {
        self.stats.lock().unwrap().rejected_overload += 1;
    }

    pub fn execute(&self, req: &PhpRequest) -> Result<PhpResponse> {
        {
            let (lock, _) = &*self.state;
            let st = lock.lock().unwrap();
            if st.draining {
                drop(st);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "draining: gateway is shutting down, request rejected (retry)".to_string(),
                ));
            }
            if let Some(until) = st.circuit_open_until {
                if Instant::now() < until {
                    drop(st);
                    self.record_rejected();
                    return Err(PhpxError::Runtime(
                        "overload: crash-loop circuit open, fast-reject (backpressure)".to_string(),
                    ));
                }
            }
        }

        let (lock, cvar) = &*self.state;
        let mut guard = lock.lock().unwrap();

        if let Some(until) = guard.circuit_open_until {
            if Instant::now() >= until {
                guard.circuit_open_until = None;
            } else {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: crash-loop circuit open, fast-reject (backpressure)".to_string(),
                ));
            }
        }
        if guard.draining {
            drop(guard);
            self.record_rejected();
            return Err(PhpxError::Runtime(
                "draining: gateway is shutting down, request rejected (retry)".to_string(),
            ));
        }

        if guard.active >= self.cfg.max_workers as usize {
            if guard.waiting >= self.cfg.queue_bound {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: worker pool saturated, request rejected (backpressure)".to_string(),
                ));
            }
            guard.waiting += 1;
            let (g, res) = cvar
                .wait_timeout_while(guard, self.cfg.queue_wait_timeout, |s| {
                    s.active >= self.cfg.max_workers as usize && !s.draining
                })
                .unwrap();
            guard = g;
            guard.waiting -= 1;
            if guard.draining {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "draining: gateway is shutting down, request rejected (retry)".to_string(),
                ));
            }
            if res.timed_out() && guard.active >= self.cfg.max_workers as usize {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: queue wait timeout (backpressure)".to_string(),
                ));
            }
        }
        guard.active += 1;
        drop(guard);

        let opts = ExecuteOptions {
            timeout: self.cfg.execution_timeout,
            env: Default::default(),
            memory_limit: self.cfg.php_memory_limit.clone(),
        };
        let php_bin = self.cfg.php_bin.clone();
        let out = phpx_runtime_process::execute_one_shot(&php_bin, req, &opts);

        let is_timeout = matches!(&out, Ok(resp) if resp.status == 504);
        let is_ok = matches!(&out, Ok(resp) if resp.status == 200);

        let (completed_now, recycled_now, consecutive, circuits) = {
            let (lock, cvar) = &*self.state;
            let mut guard = lock.lock().unwrap();
            guard.active = guard.active.saturating_sub(1);
            let vals = if is_ok {
                guard.consecutive_failures = 0;
                guard.circuit_open_until = None;
                guard.completed += 1;
                let c = guard.completed;
                if self.cfg.max_requests_per_slot > 0 && c % self.cfg.max_requests_per_slot == 0 {
                    guard.recycled += 1;
                }
                (guard.completed, guard.recycled, 0u64, guard.circuit_breaks)
            } else {
                guard.consecutive_failures += 1;
                let consec = guard.consecutive_failures;
                if self.cfg.restart_max_retries == 0 {
                    guard.circuit_open_until = Some(Instant::now() + self.cfg.restart_backoff_base);
                    guard.circuit_breaks += 1;
                } else if consec >= self.cfg.restart_max_retries as u64 {
                    let attempt = (consec - self.cfg.restart_max_retries as u64).min(10) as u32;
                    let backoff = self.cfg.backoff_for_retry(attempt);
                    guard.circuit_open_until = Some(Instant::now() + backoff);
                    guard.circuit_breaks += 1;
                }
                (0, 0, consec, guard.circuit_breaks)
            };
            cvar.notify_one();
            vals
        };

        if is_ok {
            let mut s = self.stats.lock().unwrap();
            s.completed = completed_now;
            s.recycled = recycled_now;
            s.consecutive_failures = 0;
        } else {
            let mut s = self.stats.lock().unwrap();
            if is_timeout {
                s.timeouts += 1;
            } else {
                s.failed += 1;
                s.restarts += 1;
            }
            s.consecutive_failures = consecutive;
            s.circuit_breaks = circuits;
        }
        let _ = &self.discovery;
        out
    }

    /// Begin graceful drain: stop accepting new work, wake waiters.
    pub fn begin_drain(&self) {
        let (lock, cvar) = &*self.state;
        let mut st = lock.lock().unwrap();
        st.draining = true;
        cvar.notify_all();
    }

    /// Wait until active == 0 or timeout. Returns true when drained.
    pub fn drain(&self, timeout: Duration) -> bool {
        self.begin_drain();
        let (lock, cvar) = &*self.state;
        let (guard, res) = cvar
            .wait_timeout_while(lock.lock().unwrap(), timeout, |s| s.active > 0)
            .unwrap();
        drop(guard);
        if res.timed_out() {
            let (l, _) = &*self.state;
            l.lock().unwrap().active == 0
        } else {
            true
        }
    }

    /// Prometheus text exposition for M4 observability.
    /// P0-A: includes `phpx_execution_mode_info` so one-shot vs persistent
    /// executions are machine-distinguishable even before P0-B lands.
    pub fn prometheus_text(&self) -> String {
        let s = self.snapshot();
        format!(
            "# HELP phpx_execution_mode_info Selected execution mode (explicit one-shot throttle; persistent mode runs on PersistentPool).\n# TYPE phpx_execution_mode_info gauge\nphpx_execution_mode_info{{execution_mode=\"{}\",protocol_version=\"{}\"}} 1\n# HELP phpx_pool_completed_total Completed PHP executions.\n# TYPE phpx_pool_completed_total counter\nphpx_pool_completed_total {}\n# HELP phpx_pool_failed_total Failed PHP executions (non-200, non-timeout).\n# TYPE phpx_pool_failed_total counter\nphpx_pool_failed_total {}\n# HELP phpx_pool_timeouts_total Execution timeouts (HTTP 504).\n# TYPE phpx_pool_timeouts_total counter\nphpx_pool_timeouts_total {}\n# HELP phpx_pool_rejected_overload_total Rejected by backpressure (HTTP 503).\n# TYPE phpx_pool_rejected_overload_total counter\nphpx_pool_rejected_overload_total {}\n# HELP phpx_pool_recycled_total Logical worker-slot recycles (one-shot isolates per request already; persistent recycles for real).\n# TYPE phpx_pool_recycled_total counter\nphpx_pool_recycled_total {}\n# HELP phpx_pool_restarts_total Restart attempts after failures.\n# TYPE phpx_pool_restarts_total counter\nphpx_pool_restarts_total {}\n# HELP phpx_pool_circuit_breaks_total Crash-loop circuit openings.\n# TYPE phpx_pool_circuit_breaks_total counter\nphpx_pool_circuit_breaks_total {}\n# HELP phpx_pool_active Currently executing requests.\n# TYPE phpx_pool_active gauge\nphpx_pool_active {}\n# HELP phpx_pool_waiting Currently queued requests.\n# TYPE phpx_pool_waiting gauge\nphpx_pool_waiting {}\n# HELP phpx_pool_max_workers Configured max workers.\n# TYPE phpx_pool_max_workers gauge\nphpx_pool_max_workers {}\n# HELP phpx_pool_min_workers Configured min workers.\n# TYPE phpx_pool_min_workers gauge\nphpx_pool_min_workers {}\n",
            self.cfg.resolved_execution_mode(),
            phpx_runtime::PROTOCOL_VERSION,
            s.completed,
            s.failed,
            s.timeouts,
            s.rejected_overload,
            s.recycled,
            s.restarts,
            s.circuit_breaks,
            s.active,
            s.waiting,
            self.cfg.max_workers,
            self.cfg.min_workers,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_pool_config() {
        let bad = PoolConfig {
            max_workers: 0,
            ..Default::default()
        };
        assert!(BoundedPool::new(bad).is_err());
        let bad_q = PoolConfig {
            queue_bound: 0,
            ..Default::default()
        };
        assert!(BoundedPool::new(bad_q).is_err());
        let bad_min = PoolConfig {
            min_workers: 9,
            max_workers: 2,
            ..Default::default()
        };
        assert!(BoundedPool::new(bad_min).is_err());
    }

    #[test]
    fn backoff_grows_and_caps() {
        let cfg = PoolConfig::default();
        let b0 = cfg.backoff_for_retry(0);
        let b3 = cfg.backoff_for_retry(3);
        let b30 = cfg.backoff_for_retry(30);
        assert!(b3 > b0);
        assert!(b30 <= cfg.restart_backoff_max * 2);
    }

    #[test]
    fn rejects_persistent_execution_mode_in_oneshot_pool() {
        // P0-B: persistent work belongs to PersistentPool; the one-shot
        // BoundedPool refuses it so modes can never be confused.
        let bad = PoolConfig {
            execution_mode: "persistent".to_string(),
            ..Default::default()
        };
        let err = match BoundedPool::new(bad) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("persistent mode must not run on BoundedPool"),
        };
        assert!(err.contains("PersistentPool"), "{err}");
        let bad_name = PoolConfig {
            execution_mode: "bogus".to_string(),
            ..Default::default()
        };
        assert!(BoundedPool::new(bad_name).is_err());
    }

    #[test]
    fn execution_mode_metric_distinguishes_one_shot() {
        let pool = BoundedPool::new(PoolConfig::default()).unwrap();
        let text = pool.prometheus_text();
        assert!(
            text.contains("phpx_execution_mode_info{execution_mode=\"one-shot\""),
            "{text}"
        );
        assert!(text.contains("protocol_version=\"1\""), "{text}");
    }

    #[test]
    fn drain_with_no_work_is_fast() {
        let pool = BoundedPool::new(PoolConfig::default()).unwrap();
        assert!(pool.drain(Duration::from_millis(50)));
        let s = pool.snapshot();
        assert_eq!(s.active, 0);
    }
}
