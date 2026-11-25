//! P2 embedded pool: single-flight gateway over the in-process libphp engine.
//!
//! The NTS engine lives on exactly ONE dedicated OS thread
//! (`phpx-runtime-embedded::live`), so this pool serializes all requests
//! through one slot (`active` is 0 or 1). Concurrency beyond that waits in
//! a bounded queue with explicit timeouts — same backpressure contract as
//! the process pools (503 on full/wait-timeout, 504 on execution timeout).
//!
//! Timeout asymmetry (documented Beta limit): the gateway waits at most
//! `execution_timeout` for the engine reply and reports 504 within the
//! bound, but it cannot kill NTS mid-request (that corrupts Zend state).
//! The long request runs to completion; later requests queue behind it.
//! Process pools kill the worker instead — embedded trades killability for
//! zero fork/exec cost on a dev box.

use phpx_core::{PhpxError, Result};
use phpx_runtime::{PhpRequest, PhpResponse, WorkerOutcome};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Embedded pool config. All bounds explicit; no unbounded queue.
#[derive(Debug, Clone)]
pub struct EmbeddedPoolConfig {
    pub execution_timeout: Duration,
    pub queue_bound: usize,
    pub queue_wait_timeout: Duration,
    pub drain_timeout: Duration,
}

impl Default for EmbeddedPoolConfig {
    fn default() -> Self {
        Self {
            execution_timeout: Duration::from_secs(30),
            queue_bound: 16,
            queue_wait_timeout: Duration::from_secs(2),
            drain_timeout: Duration::from_secs(10),
        }
    }
}

impl EmbeddedPoolConfig {
    pub fn validate(&self) -> Result<()> {
        if self.queue_bound == 0 || self.queue_bound > 100_000 {
            return Err(PhpxError::Config(
                "embedded queue_bound must be 1-100000 (bounded, backpressure mandatory)"
                    .to_string(),
            ));
        }
        if self.execution_timeout.is_zero() || self.execution_timeout > Duration::from_secs(600) {
            return Err(PhpxError::Config(
                "embedded execution_timeout must be 1-600s".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct EmbeddedStats {
    pub completed: u64,
    pub failed: u64,
    pub timeouts: u64,
    pub rejected_overload: u64,
    pub active: usize,
    pub waiting: usize,
}

struct EmbeddedState {
    active: usize,
    waiting: usize,
    draining: bool,
    completed: u64,
    timeouts: u64,
    rejected_overload: u64,
}

/// Single-flight pool over `phpx-runtime-embedded::live::serve_with_timeout`.
pub struct EmbeddedPool {
    cfg: EmbeddedPoolConfig,
    state: Arc<(Mutex<EmbeddedState>, Condvar)>,
    stats: Arc<Mutex<EmbeddedStats>>,
}

impl EmbeddedPool {
    pub fn new(cfg: EmbeddedPoolConfig) -> Result<Self> {
        cfg.validate()?;
        Ok(Self {
            cfg,
            state: Arc::new((
                Mutex::new(EmbeddedState {
                    active: 0,
                    waiting: 0,
                    draining: false,
                    completed: 0,
                    timeouts: 0,
                    rejected_overload: 0,
                }),
                Condvar::new(),
            )),
            stats: Arc::new(Mutex::new(EmbeddedStats::default())),
        })
    }

    pub fn config(&self) -> &EmbeddedPoolConfig {
        &self.cfg
    }

    pub fn execution_mode(&self) -> &'static str {
        phpx_runtime::EXECUTION_MODE_EMBEDDED
    }

    fn record_rejected(&self) {
        self.stats.lock().unwrap().rejected_overload += 1;
        let (lock, _) = &*self.state;
        lock.lock().unwrap().rejected_overload += 1;
    }

    /// Point-in-time load (active is 0/1 by construction) + counters.
    pub fn load(&self) -> (usize, usize) {
        let (lock, _) = &*self.state;
        let st = lock.lock().unwrap();
        (st.active, st.waiting)
    }

    pub fn snapshot(&self) -> EmbeddedStats {
        let (lock, _) = &*self.state;
        let st = lock.lock().unwrap();
        let mut s = self.stats.lock().unwrap().clone();
        s.active = st.active;
        s.waiting = st.waiting;
        s.rejected_overload = st.rejected_overload;
        s
    }

    pub fn execute(&self, req: &PhpRequest) -> Result<PhpResponse> {
        req.validate()?;
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
            if st.waiting >= self.cfg.queue_bound {
                drop(st);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: embedded pool queue full, request rejected (backpressure)"
                        .to_string(),
                ));
            }
        }
        // Claim the single slot (or wait boundedly for it).
        let (lock, cvar) = &*self.state;
        let mut guard = lock.lock().unwrap();
        if guard.active >= 1 {
            if guard.waiting >= self.cfg.queue_bound {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: embedded pool queue full, request rejected (backpressure)"
                        .to_string(),
                ));
            }
            guard.waiting += 1;
            let (g, res) = cvar
                .wait_timeout_while(guard, self.cfg.queue_wait_timeout, |s| {
                    s.active >= 1 && !s.draining
                })
                .unwrap();
            guard = g;
            guard.waiting = guard.waiting.saturating_sub(1);
            if guard.draining {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "draining: gateway is shutting down, request rejected (retry)".to_string(),
                ));
            }
            if res.timed_out() && guard.active >= 1 {
                drop(guard);
                self.record_rejected();
                return Err(PhpxError::Runtime(
                    "overload: embedded queue wait timeout (backpressure)".to_string(),
                ));
            }
        }
        guard.active = 1;
        drop(guard);

        let started = Instant::now();
        let out = self.serve_one(req);
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let (lock, cvar) = &*self.state;
        let mut st = lock.lock().unwrap();
        st.active = st.active.saturating_sub(1);
        cvar.notify_one();
        drop(st);

        match out {
            Ok(resp) => {
                let is_timeout = resp.status == 504;
                let is_ok = resp.status == 200;
                let mut s = self.stats.lock().unwrap();
                if is_timeout {
                    s.timeouts += 1;
                    let (l, _) = &*self.state;
                    l.lock().unwrap().timeouts += 1;
                } else if is_ok {
                    s.completed += 1;
                    let (l, _) = &*self.state;
                    l.lock().unwrap().completed += 1;
                } else {
                    s.failed += 1;
                }
                let mut resp = resp;
                resp.duration_ms = elapsed_ms;
                Ok(resp)
            }
            Err(e) => {
                // Transport-level failure (engine gone, bad envelope): counts
                // as failed so /metrics distinguishes it from 504 timeouts.
                self.stats.lock().unwrap().failed += 1;
                Err(e)
            }
        }
    }

    /// Map one [`PhpRequest`] onto `live::serve_with_timeout`.
    fn serve_one(&self, req: &PhpRequest) -> Result<PhpResponse> {
        let entry_abs = std::path::Path::new(&req.app_root)
            .join(&req.entrypoint)
            .to_string_lossy()
            .replace('\\', "/");
        if entry_abs.is_empty() || entry_abs.len() > 4095 {
            return Err(PhpxError::Config(
                "embedded: entry path empty or over cap".to_string(),
            ));
        }
        let headers: Vec<(String, String)> = req
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        match phpx_runtime_embedded::live::serve_with_timeout(
            &entry_abs,
            &req.method,
            &req.path,
            &req.query,
            &headers,
            &req.body,
            self.cfg.execution_timeout,
        ) {
            Ok(out) if out.status == 0 => Ok(PhpResponse {
                request_id: req.request_id.clone(),
                status: 200,
                headers: Default::default(),
                body: out.output,
                duration_ms: 0,
                outcome: WorkerOutcome::Ok,
            }),
            // Contained app failure (framework `exit`, uncaught fatal):
            // host survived with partial output. Surface as 500 WITH the
            // captured body (process mode loses it; embedded preserves it).
            Ok(out) if out.status == 1 => Ok(PhpResponse {
                request_id: req.request_id.clone(),
                status: 500,
                headers: Default::default(),
                body: if out.output.is_empty() {
                    b"php embedded app failed".to_vec()
                } else {
                    out.output
                },
                duration_ms: 0,
                outcome: WorkerOutcome::Crashed { exit_code: None },
            }),
            Ok(out) => Err(PhpxError::Runtime(
                String::from_utf8_lossy(&out.output).to_string(),
            )),
            Err(e) if e.to_string().contains("embedded execution timeout") => Ok(PhpResponse {
                request_id: req.request_id.clone(),
                status: 504,
                headers: Default::default(),
                body: b"php execution timeout (embedded)".to_vec(),
                duration_ms: self.cfg.execution_timeout.as_millis() as u64,
                outcome: WorkerOutcome::Timeout,
            }),
            Err(e) => Err(e),
        }
    }

    pub fn begin_drain(&self) {
        let (lock, cvar) = &*self.state;
        let mut st = lock.lock().unwrap();
        st.draining = true;
        cvar.notify_all();
    }

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

    pub fn prometheus_text(&self) -> String {
        let s = self.snapshot();
        format!(
            "# HELP phpx_execution_mode_info Selected execution mode (embedded single-flight, gated by promotion gates).\n# TYPE phpx_execution_mode_info gauge\nphpx_execution_mode_info{{execution_mode=\"{}\",protocol_version=\"{}\"}} 1\n# HELP phpx_pool_completed_total Completed PHP executions.\n# TYPE phpx_pool_completed_total counter\nphpx_pool_completed_total {}\n# HELP phpx_pool_failed_total Failed PHP executions (non-200, non-timeout).\n# TYPE phpx_pool_failed_total counter\nphpx_pool_failed_total {}\n# HELP phpx_pool_timeouts_total Execution timeouts (HTTP 504).\n# TYPE phpx_pool_timeouts_total counter\nphpx_pool_timeouts_total {}\n# HELP phpx_pool_rejected_overload_total Rejected by backpressure (HTTP 503).\n# TYPE phpx_pool_rejected_overload_total counter\nphpx_pool_rejected_overload_total {}\n# HELP phpx_pool_active Currently executing requests.\n# TYPE phpx_pool_active gauge\nphpx_pool_active {}\n# HELP phpx_pool_waiting Currently queued requests.\n# TYPE phpx_pool_waiting gauge\nphpx_pool_waiting {}\n# HELP phpx_pool_max_workers Configured max workers.\n# TYPE phpx_pool_max_workers gauge\nphpx_pool_max_workers {}\n# HELP phpx_pool_min_workers Configured min workers.\n# TYPE phpx_pool_min_workers gauge\nphpx_pool_min_workers {}\n",
            self.execution_mode(),
            phpx_runtime::PROTOCOL_VERSION,
            s.completed,
            s.failed,
            s.timeouts,
            s.rejected_overload,
            s.active,
            s.waiting,
            1,
            1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str) -> PhpRequest {
        PhpRequest::minimal(id, ".", "index.php")
    }

    #[test]
    fn rejects_bad_config() {
        let bad = EmbeddedPoolConfig {
            queue_bound: 0,
            ..Default::default()
        };
        assert!(bad.validate().is_err());
        let bad = EmbeddedPoolConfig {
            execution_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn stub_execute_fails_closed_actionable() {
        // On stub builds the engine is unavailable: execute must surface the
        // actionable error (gateway maps it to 500), never panic/hang.
        // On linked builds this hits the real engine without an entry file
        // and must also not hang (bounded by execution_timeout).
        let pool = EmbeddedPool::new(EmbeddedPoolConfig {
            execution_timeout: Duration::from_secs(5),
            queue_bound: 4,
            queue_wait_timeout: Duration::from_secs(2),
            drain_timeout: Duration::from_secs(2),
        })
        .unwrap();
        let r = pool.execute(&req("emb-closed-1"));
        if phpx_runtime_embedded::is_build_linked() {
            // Linked: either an engine-level error or a mapped response —
            // either way the call returns within the bound.
            let _ = r;
        } else {
            let e = r.expect_err("stub must fail closed");
            assert!(
                e.to_string().contains("unavailable"),
                "actionable stub error, got: {e}"
            );
        }
    }

    #[test]
    fn drain_unblocks_and_rejects() {
        let pool = EmbeddedPool::new(Default::default()).unwrap();
        assert!(pool.drain(Duration::from_secs(1)));
        pool.begin_drain();
        let e = pool
            .execute(&req("emb-drain-1"))
            .expect_err("draining rejects");
        assert!(e.to_string().contains("draining"), "{e}");
    }
}
