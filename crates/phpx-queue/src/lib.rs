use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
pub const MAX_QUEUE_DEPTH: usize = 10_000;
pub const DEFAULT_VISIBILITY_TIMEOUT_SECS: u64 = 60;

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn backoff_secs(attempt: u32) -> u64 {
    let shift = attempt.min(8) as u64;
    let base = 1u64 << shift;
    let capped = base.min(300);
    let jitter = attempt as u64 * 137 % 5;
    capped + jitter
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub payload: String,
    pub attempts: u32,
    pub max_retries: u32,
    pub not_before: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub driver: String,
    pub depth: usize,
    pub delayed: usize,
    pub running: usize,
    pub dead_letter: usize,
    pub max_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailOutcome {
    Retrying { attempts: u32, not_before: u64 },
    Dead { attempts: u32 },
}

pub trait QueueDriver: Send {
    fn enqueue(&self, payload: &str, max_retries: u32, delay_secs: u64) -> Result<Job>;
    fn reserve(&self, visibility_timeout_secs: u64) -> Result<Option<Job>>;
    fn ack(&self, id: &str) -> Result<bool>;
    fn fail(&self, id: &str) -> Result<FailOutcome>;
    fn status(&self) -> Result<QueueStatus>;
}

fn check_payload(payload: &str) -> Result<()> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(PhpxError::Config(format!(
            "queue payload too large: {} bytes (max {})",
            payload.len(),
            MAX_PAYLOAD_BYTES
        )));
    }
    if payload.trim().is_empty() {
        return Err(PhpxError::Config(
            "queue payload must not be empty".to_string(),
        ));
    }
    Ok(())
}

struct RunningEntry {
    job: Job,
    lease_until: u64,
}

struct MemoryInner {
    queued: VecDeque<Job>,
    running: HashMap<String, RunningEntry>,
    dead: Vec<Job>,
    max_depth: usize,
}

pub struct MemoryQueue {
    driver_name: String,
    inner: Mutex<MemoryInner>,
}

impl MemoryQueue {
    pub fn new(max_depth: usize) -> Self {
        Self {
            driver_name: "memory".to_string(),
            inner: Mutex::new(MemoryInner {
                queued: VecDeque::new(),
                running: HashMap::new(),
                dead: Vec::new(),
                max_depth: max_depth.clamp(1, MAX_QUEUE_DEPTH),
            }),
        }
    }

    fn sweep_expired(inner: &mut MemoryInner, now: u64) {
        let expired: Vec<String> = inner
            .running
            .iter()
            .filter(|(_, e)| e.lease_until <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for id in expired {
            if let Some(entry) = inner.running.remove(&id) {
                inner.queued.push_back(entry.job);
            }
        }
    }
}

impl QueueDriver for MemoryQueue {
    fn enqueue(&self, payload: &str, max_retries: u32, delay_secs: u64) -> Result<Job> {
        check_payload(payload)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("memory queue lock poisoned: {e}")))?;
        let depth = inner.queued.len() + inner.running.len();
        if depth >= inner.max_depth {
            return Err(PhpxError::Runtime(format!(
                "queue full: depth {depth} >= bound {} (backpressure, never unbounded)",
                inner.max_depth
            )));
        }
        let now = now_unix();
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            payload: payload.to_string(),
            attempts: 0,
            max_retries: max_retries.min(25),
            not_before: now + delay_secs,
            created_at: now,
        };
        inner.queued.push_back(job.clone());
        Ok(job)
    }

    fn reserve(&self, visibility_timeout_secs: u64) -> Result<Option<Job>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("memory queue lock poisoned: {e}")))?;
        let now = now_unix();
        Self::sweep_expired(&mut inner, now);
        let pos = inner.queued.iter().position(|j| j.not_before <= now);
        if let Some(idx) = pos {
            let job = inner.queued.remove(idx).expect("position valid");
            let lease = now + visibility_timeout_secs.max(1);
            inner.running.insert(
                job.id.clone(),
                RunningEntry {
                    job: job.clone(),
                    lease_until: lease,
                },
            );
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    fn ack(&self, id: &str) -> Result<bool> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("memory queue lock poisoned: {e}")))?;
        Ok(inner.running.remove(id).is_some())
    }

    fn fail(&self, id: &str) -> Result<FailOutcome> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("memory queue lock poisoned: {e}")))?;
        let entry = inner.running.remove(id);
        match entry {
            None => Err(PhpxError::Runtime(format!(
                "queue fail: unknown running job {id}"
            ))),
            Some(mut entry) => {
                entry.job.attempts += 1;
                if entry.job.attempts > entry.job.max_retries {
                    let attempts = entry.job.attempts;
                    inner.dead.push(entry.job);
                    Ok(FailOutcome::Dead { attempts })
                } else {
                    let nb = now_unix() + backoff_secs(entry.job.attempts);
                    entry.job.not_before = nb;
                    let attempts = entry.job.attempts;
                    inner.queued.push_back(entry.job);
                    Ok(FailOutcome::Retrying {
                        attempts,
                        not_before: nb,
                    })
                }
            }
        }
    }

    fn status(&self) -> Result<QueueStatus> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("memory queue lock poisoned: {e}")))?;
        let now = now_unix();
        Self::sweep_expired(&mut inner, now);
        let delayed = inner.queued.iter().filter(|j| j.not_before > now).count();
        Ok(QueueStatus {
            driver: self.driver_name.clone(),
            depth: inner.queued.len() + inner.running.len(),
            delayed,
            running: inner.running.len(),
            dead_letter: inner.dead.len(),
            max_depth: inner.max_depth,
        })
    }
}

pub struct RedisQueue {
    driver_name: String,
    conn: Mutex<redis::Connection>,
    prefix: String,
    max_depth: usize,
    seq: AtomicU64,
}

impl RedisQueue {
    /// Open a Redis-backed queue. `namespace` isolates fleets sharing one
    /// server (tests use a uuid per case; CLI uses "default"). Keys:
    /// `{prefix}:jobs` (HASH id->job JSON), `{prefix}:ready` (ZSET id by
    /// not_before), `{prefix}:running` (HASH id->lease_until),
    /// `{prefix}:dead` (HASH id->job JSON).
    pub fn open(url: &str, namespace: &str, max_depth: usize) -> Result<Self> {
        let ns = namespace.trim();
        if ns.is_empty() {
            return Err(PhpxError::Config(
                "redis queue namespace must not be empty (isolates fleets on one server)"
                    .to_string(),
            ));
        }
        let client = redis::Client::open(url)
            .map_err(|e| PhpxError::Runtime(format!("redis open failed: {e}")))?;
        let mut conn = client
            .get_connection_with_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| {
                PhpxError::Runtime(format!(
                    "redis connect failed (is a server listening?): {e}"
                ))
            })?;
        let _: () = redis::cmd("PING")
            .query(&mut conn)
            .map_err(|e| PhpxError::Runtime(format!("redis ping failed: {e}")))?;
        Ok(Self {
            driver_name: "redis".to_string(),
            conn: Mutex::new(conn),
            prefix: format!("phpx:q:{ns}"),
            max_depth: max_depth.clamp(1, MAX_QUEUE_DEPTH),
            seq: AtomicU64::new(0),
        })
    }

    fn locked(&self) -> Result<std::sync::MutexGuard<'_, redis::Connection>> {
        self.conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("redis connection lock poisoned: {e}")))
    }

    fn jobs_key(&self) -> String {
        format!("{}:jobs", self.prefix)
    }
    fn ready_key(&self) -> String {
        format!("{}:ready", self.prefix)
    }
    fn running_key(&self) -> String {
        format!("{}:running", self.prefix)
    }
    fn dead_key(&self) -> String {
        format!("{}:dead", self.prefix)
    }

    /// Score orders by visibility time, FIFO within the same second.
    fn score(&self, not_before: u64) -> f64 {
        let frac = (self.seq.fetch_add(1, Ordering::Relaxed) % 1000) as f64 / 1000.0;
        not_before as f64 + frac
    }

    /// Return expired leases to the ready set (mirrors the in-memory sweep).
    fn sweep_locked(conn: &mut redis::Connection, prefix: &str, now: u64) -> Result<()> {
        use redis::Commands;
        let running_key = format!("{prefix}:running");
        let jobs_key = format!("{prefix}:jobs");
        let ready_key = format!("{prefix}:ready");
        let running: Vec<(String, i64)> = conn
            .hgetall(&running_key)
            .map_err(|e| PhpxError::Runtime(format!("redis lease sweep failed: {e}")))?;
        for (id, lease) in running {
            if lease.max(0) as u64 <= now {
                let json: Option<String> = conn
                    .hget(&jobs_key, &id)
                    .map_err(|e| PhpxError::Runtime(format!("redis sweep read failed: {e}")))?;
                if let Some(json) = json {
                    if let Ok(job) = serde_json::from_str::<Job>(&json) {
                        let score = job.not_before as f64;
                        let _: usize = conn.zadd(&ready_key, &id, score).map_err(|e| {
                            PhpxError::Runtime(format!("redis sweep requeue failed: {e}"))
                        })?;
                    } else {
                        let _: usize = conn.hdel(&jobs_key, &id).map_err(|e| {
                            PhpxError::Runtime(format!("redis sweep cleanup failed: {e}"))
                        })?;
                    }
                }
                let _: usize = conn
                    .hdel(&running_key, &id)
                    .map_err(|e| PhpxError::Runtime(format!("redis sweep release failed: {e}")))?;
            }
        }
        Ok(())
    }

    /// Delete every key under this namespace (test hygiene; never touches
    /// other namespaces on the shared server).
    pub fn flush_namespace(&self) -> Result<()> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let pattern = format!("{}:*", self.prefix);
        let keys: Vec<String> = conn
            .scan_match(&pattern)
            .map_err(|e| PhpxError::Runtime(format!("redis scan failed: {e}")))?
            .map(|r| r.map_err(|e| PhpxError::Runtime(format!("redis scan failed: {e}"))))
            .collect::<Result<Vec<_>>>()?;
        if !keys.is_empty() {
            let _: usize = conn
                .del(keys)
                .map_err(|e| PhpxError::Runtime(format!("redis flush failed: {e}")))?;
        }
        Ok(())
    }
}

impl QueueDriver for RedisQueue {
    fn enqueue(&self, payload: &str, max_retries: u32, delay_secs: u64) -> Result<Job> {
        use redis::Commands;
        check_payload(payload)?;
        let mut conn = self.locked()?;
        let ready: usize = conn
            .zcard(self.ready_key())
            .map_err(|e| PhpxError::Runtime(format!("redis depth check failed: {e}")))?;
        let running: usize = conn
            .hlen(self.running_key())
            .map_err(|e| PhpxError::Runtime(format!("redis depth check failed: {e}")))?;
        let depth = ready + running;
        if depth >= self.max_depth {
            return Err(PhpxError::Runtime(format!(
                "queue full: depth {depth} >= bound {} (backpressure, never unbounded)",
                self.max_depth
            )));
        }
        let now = now_unix();
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            payload: payload.to_string(),
            attempts: 0,
            max_retries: max_retries.min(25),
            not_before: now + delay_secs,
            created_at: now,
        };
        let json = serde_json::to_string(&job)
            .map_err(|e| PhpxError::Runtime(format!("redis encode failed: {e}")))?;
        let score = self.score(job.not_before);
        let _: bool = conn
            .hset(self.jobs_key(), &job.id, &json)
            .map_err(|e| PhpxError::Runtime(format!("redis enqueue failed: {e}")))?;
        if let Err(e) = conn.zadd::<_, _, _, ()>(self.ready_key(), &job.id, score) {
            let _: Option<usize> = conn.hdel(self.jobs_key(), &job.id).ok();
            return Err(PhpxError::Runtime(format!("redis enqueue failed: {e}")));
        }
        Ok(job)
    }

    fn reserve(&self, visibility_timeout_secs: u64) -> Result<Option<Job>> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let now = now_unix();
        Self::sweep_locked(&mut conn, &self.prefix, now)?;
        let due: Vec<String> = conn
            .zrangebyscore_limit(self.ready_key(), 0, now as isize, 0, 1)
            .map_err(|e| PhpxError::Runtime(format!("redis reserve scan failed: {e}")))?;
        let Some(id) = due.into_iter().next() else {
            return Ok(None);
        };
        let claimed: usize = conn
            .zrem(self.ready_key(), &id)
            .map_err(|e| PhpxError::Runtime(format!("redis claim failed: {e}")))?;
        if claimed == 0 {
            return Ok(None);
        }
        let json: Option<String> = conn
            .hget(self.jobs_key(), &id)
            .map_err(|e| PhpxError::Runtime(format!("redis reserve read failed: {e}")))?;
        let Some(json) = json else {
            return Ok(None);
        };
        let job: Job = serde_json::from_str(&json)
            .map_err(|e| PhpxError::Runtime(format!("redis decode failed: {e}")))?;
        let lease = now + visibility_timeout_secs.max(1);
        let _: bool = conn
            .hset(self.running_key(), &id, lease as i64)
            .map_err(|e| PhpxError::Runtime(format!("redis lease failed: {e}")))?;
        Ok(Some(job))
    }

    fn ack(&self, id: &str) -> Result<bool> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let running: bool = conn
            .hexists(self.running_key(), id)
            .map_err(|e| PhpxError::Runtime(format!("redis ack check failed: {e}")))?;
        if !running {
            return Ok(false);
        }
        let _: usize = conn
            .hdel(self.running_key(), id)
            .map_err(|e| PhpxError::Runtime(format!("redis ack failed: {e}")))?;
        let _: usize = conn
            .hdel(self.jobs_key(), id)
            .map_err(|e| PhpxError::Runtime(format!("redis ack cleanup failed: {e}")))?;
        Ok(true)
    }

    fn fail(&self, id: &str) -> Result<FailOutcome> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let lease: Option<i64> = conn
            .hget(self.running_key(), id)
            .map_err(|e| PhpxError::Runtime(format!("redis fail check failed: {e}")))?;
        if lease.is_none() {
            return Err(PhpxError::Runtime(format!(
                "queue fail: unknown running job {id}"
            )));
        }
        let json: Option<String> = conn
            .hget(self.jobs_key(), id)
            .map_err(|e| PhpxError::Runtime(format!("redis fail read failed: {e}")))?;
        let Some(json) = json else {
            return Err(PhpxError::Runtime(format!(
                "queue fail: unknown running job {id}"
            )));
        };
        let mut job: Job = serde_json::from_str(&json)
            .map_err(|e| PhpxError::Runtime(format!("redis decode failed: {e}")))?;
        job.attempts += 1;
        if job.attempts > job.max_retries {
            let attempts = job.attempts;
            let json = serde_json::to_string(&job)
                .map_err(|e| PhpxError::Runtime(format!("redis encode failed: {e}")))?;
            let _: bool = conn
                .hset(self.dead_key(), id, &json)
                .map_err(|e| PhpxError::Runtime(format!("redis dead-letter failed: {e}")))?;
            let _: usize = conn.hdel(self.running_key(), id).map_err(|e| {
                PhpxError::Runtime(format!("redis dead-letter cleanup failed: {e}"))
            })?;
            let _: usize = conn.hdel(self.jobs_key(), id).map_err(|e| {
                PhpxError::Runtime(format!("redis dead-letter cleanup failed: {e}"))
            })?;
            Ok(FailOutcome::Dead { attempts })
        } else {
            let nb = now_unix() + backoff_secs(job.attempts);
            job.not_before = nb;
            let attempts = job.attempts;
            let json = serde_json::to_string(&job)
                .map_err(|e| PhpxError::Runtime(format!("redis encode failed: {e}")))?;
            let _: bool = conn
                .hset(self.jobs_key(), id, &json)
                .map_err(|e| PhpxError::Runtime(format!("redis retry update failed: {e}")))?;
            let _: usize = conn
                .hdel(self.running_key(), id)
                .map_err(|e| PhpxError::Runtime(format!("redis retry release failed: {e}")))?;
            let score = self.score(nb);
            let _: usize = conn
                .zadd(self.ready_key(), id, score)
                .map_err(|e| PhpxError::Runtime(format!("redis retry requeue failed: {e}")))?;
            Ok(FailOutcome::Retrying {
                attempts,
                not_before: nb,
            })
        }
    }

    fn status(&self) -> Result<QueueStatus> {
        use redis::Commands;
        let mut conn = self.locked()?;
        let now = now_unix();
        Self::sweep_locked(&mut conn, &self.prefix, now)?;
        let depth: usize = conn
            .zcard(self.ready_key())
            .map_err(|e| PhpxError::Runtime(format!("redis count failed: {e}")))?;
        let running: usize = conn
            .hlen(self.running_key())
            .map_err(|e| PhpxError::Runtime(format!("redis count failed: {e}")))?;
        let delayed: usize = conn
            .zcount(self.ready_key(), format!("({now}"), "+inf")
            .map_err(|e| PhpxError::Runtime(format!("redis count failed: {e}")))?;
        let dead: usize = conn
            .hlen(self.dead_key())
            .map_err(|e| PhpxError::Runtime(format!("redis count failed: {e}")))?;
        Ok(QueueStatus {
            driver: self.driver_name.clone(),
            depth: depth + running,
            delayed,
            running,
            dead_letter: dead,
            max_depth: self.max_depth,
        })
    }
}

pub struct SqliteQueue {
    driver_name: String,
    conn: Mutex<rusqlite::Connection>,
    max_depth: usize,
}

impl SqliteQueue {
    pub fn open(path: &Path, max_depth: usize) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    PhpxError::Io(format!("cannot create queue dir {}: {e}", parent.display()))
                })?;
            }
        }
        let conn = rusqlite::Connection::open(path).map_err(|e| {
            PhpxError::Io(format!("cannot open sqlite queue {}: {e}", path.display()))
        })?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                payload TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                not_before INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT 'queued',
                lease_until INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_state_notbefore ON jobs(state, not_before);
            CREATE INDEX IF NOT EXISTS idx_jobs_lease ON jobs(state, lease_until);",
        )
        .map_err(|e| PhpxError::Runtime(format!("sqlite migrate failed: {e}")))?;
        let q = Self {
            driver_name: "sqlite".to_string(),
            conn: Mutex::new(conn),
            max_depth: max_depth.clamp(1, MAX_QUEUE_DEPTH),
        };
        q.recover_running()?;
        Ok(q)
    }

    fn recover_running(&self) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("sqlite lock poisoned: {e}")))?;
        let n = conn
            .execute(
                "UPDATE jobs SET state='queued', lease_until=0 WHERE state='running'",
                [],
            )
            .map_err(|e| PhpxError::Runtime(format!("sqlite recover failed: {e}")))?;
        Ok(n)
    }

    fn depth_locked(conn: &rusqlite::Connection) -> Result<usize> {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE state IN ('queued','running')",
                [],
                |r| r.get(0),
            )
            .map_err(|e| PhpxError::Runtime(format!("sqlite count failed: {e}")))?;
        Ok(n.max(0) as usize)
    }
}

impl QueueDriver for SqliteQueue {
    fn enqueue(&self, payload: &str, max_retries: u32, delay_secs: u64) -> Result<Job> {
        check_payload(payload)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("sqlite lock poisoned: {e}")))?;
        let depth = Self::depth_locked(&conn)?;
        if depth >= self.max_depth {
            return Err(PhpxError::Runtime(format!(
                "queue full: depth {depth} >= bound {} (backpressure, never unbounded)",
                self.max_depth
            )));
        }
        let now = now_unix();
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            payload: payload.to_string(),
            attempts: 0,
            max_retries: max_retries.min(25),
            not_before: now + delay_secs,
            created_at: now,
        };
        conn.execute(
            "INSERT INTO jobs (id, payload, attempts, max_retries, not_before, created_at, state, lease_until) VALUES (?1,?2,?3,?4,?5,?6,'queued',0)",
            rusqlite::params![job.id, job.payload, job.attempts as i64, job.max_retries as i64, job.not_before as i64, job.created_at as i64],
        )
        .map_err(|e| PhpxError::Runtime(format!("sqlite enqueue failed: {e}")))?;
        Ok(job)
    }

    fn reserve(&self, visibility_timeout_secs: u64) -> Result<Option<Job>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("sqlite lock poisoned: {e}")))?;
        let now = now_unix() as i64;
        conn.execute(
            "UPDATE jobs SET state='queued', lease_until=0 WHERE state='running' AND lease_until <= ?1",
            rusqlite::params![now],
        )
        .map_err(|e| PhpxError::Runtime(format!("sqlite lease sweep failed: {e}")))?;
        let row: Option<(String, String, i64, i64, i64, i64)> = conn
            .query_row(
                "SELECT id, payload, attempts, max_retries, not_before, created_at FROM jobs WHERE state='queued' AND not_before <= ?1 ORDER BY created_at ASC LIMIT 1",
                rusqlite::params![now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()
            .map_err(|e| PhpxError::Runtime(format!("sqlite reserve select failed: {e}")))?;
        match row {
            None => Ok(None),
            Some((id, payload, attempts, max_retries, not_before, created_at)) => {
                let lease = now + visibility_timeout_secs.max(1) as i64;
                conn.execute(
                    "UPDATE jobs SET state='running', lease_until=?1 WHERE id=?2",
                    rusqlite::params![lease, id],
                )
                .map_err(|e| PhpxError::Runtime(format!("sqlite claim failed: {e}")))?;
                Ok(Some(Job {
                    id,
                    payload,
                    attempts: attempts.max(0) as u32,
                    max_retries: max_retries.max(0) as u32,
                    not_before: not_before.max(0) as u64,
                    created_at: created_at.max(0) as u64,
                }))
            }
        }
    }

    fn ack(&self, id: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("sqlite lock poisoned: {e}")))?;
        let n = conn
            .execute(
                "DELETE FROM jobs WHERE id=?1 AND state='running'",
                rusqlite::params![id],
            )
            .map_err(|e| PhpxError::Runtime(format!("sqlite ack failed: {e}")))?;
        Ok(n == 1)
    }

    fn fail(&self, id: &str) -> Result<FailOutcome> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("sqlite lock poisoned: {e}")))?;
        let row: Option<(i64, i64)> = conn
            .query_row(
                "SELECT attempts, max_retries FROM jobs WHERE id=?1 AND state='running'",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| PhpxError::Runtime(format!("sqlite fail select failed: {e}")))?;
        match row {
            None => Err(PhpxError::Runtime(format!(
                "queue fail: unknown running job {id}"
            ))),
            Some((attempts, max_retries)) => {
                let next = (attempts + 1).max(0) as u32;
                let maxr = max_retries.max(0) as u32;
                if next > maxr {
                    conn.execute(
                        "UPDATE jobs SET attempts=?1, state='dead', lease_until=0 WHERE id=?2",
                        rusqlite::params![next as i64, id],
                    )
                    .map_err(|e| PhpxError::Runtime(format!("sqlite dead-letter failed: {e}")))?;
                    Ok(FailOutcome::Dead { attempts: next })
                } else {
                    let nb = now_unix() + backoff_secs(next);
                    conn.execute(
                        "UPDATE jobs SET attempts=?1, not_before=?2, state='queued', lease_until=0 WHERE id=?3",
                        rusqlite::params![next as i64, nb as i64, id],
                    )
                    .map_err(|e| PhpxError::Runtime(format!("sqlite retry failed: {e}")))?;
                    Ok(FailOutcome::Retrying {
                        attempts: next,
                        not_before: nb,
                    })
                }
            }
        }
    }

    fn status(&self) -> Result<QueueStatus> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("sqlite lock poisoned: {e}")))?;
        let now = now_unix() as i64;
        conn.execute(
            "UPDATE jobs SET state='queued', lease_until=0 WHERE state='running' AND lease_until <= ?1",
            rusqlite::params![now],
        )
        .map_err(|e| PhpxError::Runtime(format!("sqlite lease sweep failed: {e}")))?;
        let depth: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE state IN ('queued','running')",
                [],
                |r| r.get(0),
            )
            .map_err(|e| PhpxError::Runtime(format!("sqlite count failed: {e}")))?;
        let delayed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE state='queued' AND not_before > ?1",
                rusqlite::params![now],
                |r| r.get(0),
            )
            .map_err(|e| PhpxError::Runtime(format!("sqlite delayed count failed: {e}")))?;
        let running: i64 = conn
            .query_row("SELECT COUNT(*) FROM jobs WHERE state='running'", [], |r| {
                r.get(0)
            })
            .map_err(|e| PhpxError::Runtime(format!("sqlite running count failed: {e}")))?;
        let dead: i64 = conn
            .query_row("SELECT COUNT(*) FROM jobs WHERE state='dead'", [], |r| {
                r.get(0)
            })
            .map_err(|e| PhpxError::Runtime(format!("sqlite dead count failed: {e}")))?;
        Ok(QueueStatus {
            driver: self.driver_name.clone(),
            depth: depth.max(0) as usize,
            delayed: delayed.max(0) as usize,
            running: running.max(0) as usize,
            dead_letter: dead.max(0) as usize,
            max_depth: self.max_depth,
        })
    }
}

use rusqlite::OptionalExtension;

pub fn open_queue(
    driver: &str,
    database: Option<&str>,
    max_depth: usize,
    redis_url: Option<&str>,
) -> Result<Box<dyn QueueDriver>> {
    match driver.to_ascii_lowercase().as_str() {
        "memory" => Ok(Box::new(MemoryQueue::new(max_depth))),
        "sqlite" => {
            let db = database.filter(|s| !s.trim().is_empty()).ok_or_else(|| {
                PhpxError::Config(
                    "queue.driver=sqlite requires [queue] database path (e.g. ./data/queue.db)"
                        .to_string(),
                )
            })?;
            Ok(Box::new(SqliteQueue::open(Path::new(db), max_depth)?))
        }
        "redis" => {
            let url = redis_url
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .or_else(|| {
                    std::env::var("PHPX_REDIS_URL")
                        .ok()
                        .filter(|s| !s.trim().is_empty())
                })
                .ok_or_else(|| {
                    PhpxError::Config(
                        "queue.driver=redis requires [queue] url (e.g. redis://127.0.0.1:6379) or PHPX_REDIS_URL env"
                            .to_string(),
                    )
                })?;
            Ok(Box::new(RedisQueue::open(&url, "default", max_depth)?))
        }
        other => Err(PhpxError::Config(format!(
            "queue.driver must be memory, sqlite or redis, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_secs(0), 1);
        assert!(backoff_secs(1) >= 2);
        assert!(backoff_secs(10) <= 305);
    }

    #[test]
    fn memory_rejects_empty_and_huge() {
        let q = MemoryQueue::new(10);
        assert!(q.enqueue("", 3, 0).is_err());
        let big = "x".repeat(MAX_PAYLOAD_BYTES + 1);
        assert!(q.enqueue(&big, 3, 0).is_err());
    }

    #[test]
    fn memory_bounded() {
        let q = MemoryQueue::new(2);
        q.enqueue("a", 1, 0).unwrap();
        q.enqueue("b", 1, 0).unwrap();
        assert!(q.enqueue("c", 1, 0).is_err());
    }
}
