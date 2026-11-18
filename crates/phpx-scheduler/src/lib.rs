//! PHPX scheduler (M5 phase 2): cron-compatible, timezone-aware,
//! single-node job dispatch.
//!
//! SPEC 7.2: the scheduler only **dispatches** jobs by enqueueing them
//! into the queue. It never executes PHP work and must not run on gateway
//! async threads. Each invocation performs a single dispatch pass; run it
//! every minute (systemd timer or cron) for continuous operation.
//! Duplicate-dispatch protection is enforced through the execution history,
//! so overlapping or retried invocations stay safe.

use chrono::{DateTime, Datelike, Timelike};
use phpx_core::{PhpxError, Result};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// Maximum number of schedules in one configuration (bounded, never unbounded).
pub const MAX_SCHEDULES: usize = 100;
/// Payload bound, mirrors the queue bound (SPEC 7.1).
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
/// Per-job retry bound, mirrors the queue bound (0-25).
pub const MAX_RETRIES: u32 = 25;
/// Brute-force minute scan cap for next-match search (~366 days, leap-safe).
const SCAN_LIMIT_MINUTES: u64 = 366 * 24 * 60;
/// Defensive cap on due slots examined per schedule per tick.
const MAX_DUES_PER_TICK: usize = 10_000;

// ---------------------------------------------------------------------------
// Cron expressions (5-field `minute hour dom month dow`)
// ---------------------------------------------------------------------------

const MONTH_NAMES: &[(&str, u32)] = &[
    ("jan", 1),
    ("january", 1),
    ("feb", 2),
    ("february", 2),
    ("mar", 3),
    ("march", 3),
    ("apr", 4),
    ("april", 4),
    ("may", 5),
    ("jun", 6),
    ("june", 6),
    ("jul", 7),
    ("july", 7),
    ("aug", 8),
    ("august", 8),
    ("sep", 9),
    ("sept", 9),
    ("september", 9),
    ("oct", 10),
    ("october", 10),
    ("nov", 11),
    ("november", 11),
    ("dec", 12),
    ("december", 12),
];

const DOW_NAMES: &[(&str, u32)] = &[
    ("sun", 0),
    ("sunday", 0),
    ("mon", 1),
    ("monday", 1),
    ("tue", 2),
    ("tues", 2),
    ("tuesday", 2),
    ("wed", 3),
    ("wednesday", 3),
    ("thu", 4),
    ("thur", 4),
    ("thurs", 4),
    ("thursday", 4),
    ("fri", 5),
    ("friday", 5),
    ("sat", 6),
    ("saturday", 6),
];

/// Parsed 5-field cron expression. See [`CronSpec::parse`].
#[derive(Debug, Clone)]
pub struct CronSpec {
    minute: [bool; 60],
    hour: [bool; 24],
    dom: [bool; 32],
    month: [bool; 13],
    dow: [bool; 7],
    dom_star: bool,
    dow_star: bool,
}

fn resolve_value(
    token: &str,
    names: Option<&[(&str, u32)]>,
    min: u32,
    max: u32,
    field: &str,
) -> Result<u32> {
    if let Some(table) = names {
        let low = token.to_ascii_lowercase();
        for (name, value) in table {
            if *name == low {
                return Ok(*value);
            }
        }
    }
    let value: u32 = token.parse().map_err(|_| {
        PhpxError::Config(format!(
            "cron value {token:?} is not a number (field {field:?}, expected {min}-{max})"
        ))
    })?;
    if value < min || value > max {
        return Err(PhpxError::Config(format!(
            "cron value {value} out of range {min}-{max} (field {field:?})"
        )));
    }
    Ok(value)
}

/// Parse one cron field into `set` (indexed by value). Returns true when the
/// field is unrestricted (`*` or `?`); that flag drives dom/dow OR-semantics.
fn parse_field(
    set: &mut [bool],
    min: u32,
    max: u32,
    raw: &str,
    names: Option<&[(&str, u32)]>,
) -> Result<bool> {
    let field = raw.trim();
    if field == "*" || field == "?" {
        for value in min..=max {
            set[value as usize] = true;
        }
        return Ok(true);
    }
    let mut matched_any = false;
    for part in field.split(',') {
        let item = part.trim();
        if item.is_empty() {
            return Err(PhpxError::Config(format!(
                "cron field {field:?} has an empty item (hint: `1,2,3` with no trailing comma)"
            )));
        }
        let (base, step) = match item.split_once('/') {
            Some((before, after)) => {
                let n: u32 = after.trim().parse().map_err(|_| {
                    PhpxError::Config(format!(
                        "cron step {:?} is not a positive number (hint: `*/15`)",
                        after.trim()
                    ))
                })?;
                if n == 0 || n > max - min + 1 {
                    return Err(PhpxError::Config(format!(
                        "cron step {n} out of range 1-{} (field {field:?})",
                        max - min + 1
                    )));
                }
                (before.trim(), n)
            }
            None => (item, 1),
        };
        if base.is_empty() {
            return Err(PhpxError::Config(format!(
                "cron item {item:?} is missing its range (hint: `*/15` or `1-30/5`)"
            )));
        }
        let (lo, hi) = if base == "*" || base == "?" {
            (min, max)
        } else if let Some((start, end)) = base.split_once('-') {
            let lo = resolve_value(start.trim(), names, min, max, field)?;
            let hi = resolve_value(end.trim(), names, min, max, field)?;
            (lo, hi)
        } else {
            let value = resolve_value(base, names, min, max, field)?;
            (value, value)
        };
        if lo > hi {
            return Err(PhpxError::Config(format!(
                "cron range {lo}-{hi} is reversed (field {field:?})"
            )));
        }
        let mut value = lo;
        while value <= hi {
            set[value as usize] = true;
            matched_any = true;
            value += step;
        }
    }
    if !matched_any {
        return Err(PhpxError::Config(format!(
            "cron field {field:?} matched nothing (expected values {min}-{max})"
        )));
    }
    Ok(false)
}

impl CronSpec {
    /// Parse `minute hour dom month dow` (e.g. `*/15 9-17 * * MON-FRI`).
    /// Supports `*`, `?` (dom/dow alias for `*`), `*/step`, `a-b`,
    /// `a-b/step`, comma lists, and English month/day names.
    pub fn parse(expr: &str) -> Result<Self> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(PhpxError::Config(format!(
                "cron {expr:?} must have 5 fields `minute hour dom month dow` (e.g. `0 2 * * *`), got {}",
                parts.len()
            )));
        }
        let mut minute = [false; 60];
        let mut hour = [false; 24];
        let mut dom = [false; 32];
        let mut month = [false; 13];
        let mut dow_raw = [false; 8];
        parse_field(&mut minute, 0, 59, parts[0], None)?;
        parse_field(&mut hour, 0, 23, parts[1], None)?;
        let dom_star = parse_field(&mut dom, 1, 31, parts[2], None)?;
        parse_field(&mut month, 1, 12, parts[3], Some(MONTH_NAMES))?;
        let dow_star = parse_field(&mut dow_raw, 0, 7, parts[4], Some(DOW_NAMES))?;
        // Cron allows both 0 and 7 for Sunday; fold 7 into 0.
        let mut dow = [false; 7];
        dow.copy_from_slice(&dow_raw[..7]);
        dow[0] |= dow_raw[7];
        Ok(Self {
            minute,
            hour,
            dom,
            month,
            dow,
            dom_star,
            dow_star,
        })
    }

    /// True when a local wall-clock time matches this expression.
    /// `dow0sun`: 0 = Sunday .. 6 = Saturday.
    pub fn matches_local(
        &self,
        minute: u32,
        hour: u32,
        dom: u32,
        month: u32,
        dow0sun: u32,
    ) -> bool {
        if minute > 59
            || hour > 23
            || dom == 0
            || dom > 31
            || month == 0
            || month > 12
            || dow0sun > 6
        {
            return false;
        }
        if !self.minute[minute as usize] || !self.hour[hour as usize] || !self.month[month as usize]
        {
            return false;
        }
        let dom_ok = self.dom[dom as usize];
        let dow_ok = self.dow[dow0sun as usize];
        // Standard cron: dom and dow are OR-ed when both are restricted.
        if self.dom_star && self.dow_star {
            true
        } else if self.dom_star {
            dow_ok
        } else if self.dow_star {
            dom_ok
        } else {
            dom_ok || dow_ok
        }
    }
}

// ---------------------------------------------------------------------------
// Timezones
// ---------------------------------------------------------------------------

/// Schedule timezone: UTC, the host local zone, a fixed offset, or an IANA
/// zone such as `Asia/Jakarta` (DST-safe via chrono-tz).
#[derive(Debug, Clone)]
pub enum ScheduleTz {
    Utc,
    Local,
    Fixed(chrono::FixedOffset),
    Iana(chrono_tz::Tz),
}

impl ScheduleTz {
    /// Short label for status output (e.g. `UTC`, `local`, `+07:00`, `Asia/Jakarta`).
    pub fn describe(&self) -> String {
        match self {
            ScheduleTz::Utc => "UTC".to_string(),
            ScheduleTz::Local => "local".to_string(),
            ScheduleTz::Fixed(off) => {
                let secs = off.local_minus_utc();
                let sign = if secs < 0 { '-' } else { '+' };
                let abs = secs.abs();
                format!("{sign}{:02}:{:02}", abs / 3600, (abs % 3600) / 60)
            }
            ScheduleTz::Iana(tz) => tz.name().to_string(),
        }
    }
}

fn parse_fixed_offset(raw: &str) -> Result<Option<chrono::FixedOffset>> {
    let text = raw.trim();
    let low = text.to_ascii_lowercase();
    let rest = low
        .strip_prefix("utc")
        .or_else(|| low.strip_prefix("gmt"))
        .unwrap_or(&low);
    if rest.is_empty() {
        // Input was exactly `utc`/`gmt` in some casing.
        return Ok(Some(
            chrono::FixedOffset::east_opt(0).expect("zero offset is valid"),
        ));
    }
    let bytes = rest.as_bytes();
    if bytes[0] != b'+' && bytes[0] != b'-' {
        if rest.len() != low.len() {
            return Err(PhpxError::Config(format!(
                "timezone {raw:?} has a UTC/GMT prefix but no offset (hint: `UTC+07:00` or plain `UTC`)"
            )));
        }
        return Ok(None);
    }
    let sign: i32 = if bytes[0] == b'+' { 1 } else { -1 };
    let digits: String = rest[1..].chars().filter(|c| *c != ':').collect();
    let (hh, mm) = match digits.len() {
        1 => (digits.parse().unwrap_or(99), 0),
        2 => (digits[0..2].parse().unwrap_or(99), 0),
        4 => (
            digits[0..2].parse().unwrap_or(99),
            digits[2..4].parse().unwrap_or(99),
        ),
        _ => {
            return Err(PhpxError::Config(format!(
                "timezone offset {raw:?} must look like `+07:00`, `+0700`, `+07`, or `+7`"
            )));
        }
    };
    if hh > 23 || mm > 59 {
        return Err(PhpxError::Config(format!(
            "timezone offset {raw:?} out of range (HH 00-23, MM 00-59)"
        )));
    }
    match chrono::FixedOffset::east_opt(sign * (hh * 3600 + mm * 60)) {
        Some(off) => Ok(Some(off)),
        None => Err(PhpxError::Config(format!(
            "timezone offset {raw:?} out of range (max +-23:59)"
        ))),
    }
}

/// Parse a schedule timezone: `UTC`/`Z`/`GMT`, `local`, a fixed offset such
/// as `+07:00` (optional `UTC`/`GMT` prefix), or an IANA name such as
/// `Asia/Jakarta`. Empty means UTC.
pub fn parse_timezone(raw: &str) -> Result<ScheduleTz> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(ScheduleTz::Utc);
    }
    let low = text.to_ascii_lowercase();
    if low == "utc" || low == "z" || low == "gmt" {
        return Ok(ScheduleTz::Utc);
    }
    if low == "local" {
        return Ok(ScheduleTz::Local);
    }
    if let Ok(tz) = text.parse::<chrono_tz::Tz>() {
        return Ok(ScheduleTz::Iana(tz));
    }
    if let Some(off) = parse_fixed_offset(text)? {
        // `utc`/`gmt` exact spellings were handled above; this covers
        // prefixed offsets like `UTC+07:00`.
        if off.local_minus_utc() == 0 && (low == "utc" || low == "gmt") {
            return Ok(ScheduleTz::Utc);
        }
        return Ok(ScheduleTz::Fixed(off));
    }
    Err(PhpxError::Config(format!(
        "timezone {text:?} unknown (hint: `UTC`, `local`, `+07:00`, or an IANA name like `Asia/Jakarta`)"
    )))
}

fn parts_of<Tz: chrono::TimeZone>(dt: DateTime<Tz>) -> (u32, u32, u32, u32, u32) {
    (
        dt.minute(),
        dt.hour(),
        dt.day(),
        dt.month(),
        dt.weekday().num_days_from_sunday(),
    )
}

impl ScheduleTz {
    /// Break a UTC unix timestamp into local wall-clock parts
    /// `(minute, hour, dom, month, dow0sun)` for cron matching.
    pub fn local_parts(&self, utc_unix: u64) -> Result<(u32, u32, u32, u32, u32)> {
        let utc = DateTime::from_timestamp(utc_unix as i64, 0)
            .ok_or_else(|| PhpxError::Config(format!("timestamp {utc_unix} out of range")))?;
        Ok(match self {
            ScheduleTz::Utc => parts_of(utc),
            ScheduleTz::Fixed(off) => parts_of(utc.with_timezone(off)),
            ScheduleTz::Iana(tz) => parts_of(utc.with_timezone(tz)),
            ScheduleTz::Local => parts_of(utc.with_timezone(&chrono::Local)),
        })
    }
}

// ---------------------------------------------------------------------------
// Schedule definitions
// ---------------------------------------------------------------------------

/// Unwrap one `configuration error: ` display prefix so layered validation
/// (cron -> schedule -> config) reads as a single actionable message.
fn detail(e: PhpxError) -> String {
    let text = e.to_string();
    text.strip_prefix("configuration error: ")
        .unwrap_or(&text)
        .to_string()
}

/// What to do when a run was missed (scheduler was not invoked in time).
/// Must be explicit per schedule (SPEC 7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedPolicy {
    /// Skip missed slots; wait for the next due time.
    Skip,
    /// Dispatch one catchup job (newest missed slot) on the next pass.
    DispatchNow,
}

impl MissedPolicy {
    /// Parse `skip` (default) or `dispatch-now` (`catchup` accepted as alias).
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "skip" => Ok(MissedPolicy::Skip),
            "dispatch-now" | "dispatch_now" | "catchup" => Ok(MissedPolicy::DispatchNow),
            other => Err(PhpxError::Config(format!(
                "missed_policy must be `skip` or `dispatch-now`, got {other:?}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MissedPolicy::Skip => "skip",
            MissedPolicy::DispatchNow => "dispatch-now",
        }
    }
}

fn valid_schedule_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// One validated schedule: parsed cron + timezone plus dispatch parameters.
#[derive(Debug, Clone)]
pub struct ScheduleDef {
    pub id: String,
    pub cron_expr: String,
    pub cron: CronSpec,
    pub timezone_expr: String,
    pub tz: ScheduleTz,
    pub payload: String,
    pub enabled: bool,
    pub missed_policy: MissedPolicy,
    pub max_retries: u32,
}

impl ScheduleDef {
    /// Validate and build a schedule. Empty timezone defaults to UTC and
    /// empty missed policy defaults to `skip`.
    pub fn new(
        id: &str,
        cron_expr: &str,
        timezone_expr: &str,
        payload: &str,
        enabled: bool,
        missed_policy_expr: &str,
        max_retries: u32,
    ) -> Result<Self> {
        let id = id.trim().to_string();
        if !valid_schedule_id(&id) {
            return Err(PhpxError::Config(format!(
                "schedule id {id:?} invalid (hint: 1-64 chars of A-Z a-z 0-9 `-` `_` `.`)"
            )));
        }
        if cron_expr.trim().is_empty() {
            return Err(PhpxError::Config(format!(
                "schedule {id:?}: cron must not be empty (e.g. `0 2 * * *`)"
            )));
        }
        let cron = CronSpec::parse(cron_expr)
            .map_err(|e| PhpxError::Config(format!("schedule {id:?}: {}", detail(e))))?;
        let tz = parse_timezone(timezone_expr)
            .map_err(|e| PhpxError::Config(format!("schedule {id:?}: {}", detail(e))))?;
        if payload.trim().is_empty() {
            return Err(PhpxError::Config(format!(
                "schedule {id:?}: payload must not be empty (it becomes the queue job payload)"
            )));
        }
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(PhpxError::Config(format!(
                "schedule {id:?}: payload too large ({} bytes, max {MAX_PAYLOAD_BYTES})",
                payload.len()
            )));
        }
        if max_retries > MAX_RETRIES {
            return Err(PhpxError::Config(format!(
                "schedule {id:?}: max_retries must be 0-{MAX_RETRIES}, got {max_retries}"
            )));
        }
        let missed_policy = MissedPolicy::parse(missed_policy_expr)
            .map_err(|e| PhpxError::Config(format!("schedule {id:?}: {}", detail(e))))?;
        let timezone_expr = {
            let t = timezone_expr.trim();
            if t.is_empty() {
                "UTC".to_string()
            } else {
                t.to_string()
            }
        };
        Ok(Self {
            id,
            cron_expr: cron_expr.trim().to_string(),
            cron,
            timezone_expr,
            tz,
            payload: payload.to_string(),
            enabled,
            missed_policy,
            max_retries,
        })
    }

    /// Truncate a unix timestamp down to its minute.
    pub fn due_minute(utc_unix: u64) -> u64 {
        utc_unix / 60 * 60
    }

    /// True when `utc_unix` falls inside a due minute for this schedule.
    pub fn is_due(&self, utc_unix: u64) -> Result<bool> {
        let (minute, hour, dom, month, dow) = self.tz.local_parts(utc_unix)?;
        Ok(self.cron.matches_local(minute, hour, dom, month, dow))
    }

    /// First due minute strictly after `after_unix`, or None when nothing
    /// matches within ~366 days (misconfigured far-future February 30ths).
    pub fn next_after(&self, after_unix: u64) -> Result<Option<u64>> {
        let mut candidate = Self::due_minute(after_unix).saturating_add(60);
        for _ in 0..SCAN_LIMIT_MINUTES {
            if self.is_due(candidate)? {
                return Ok(Some(candidate));
            }
            candidate = candidate.saturating_add(60);
        }
        Ok(None)
    }
}
// ---------------------------------------------------------------------------
// Execution history (duplicate-dispatch protection, SPEC 7.2)
// ---------------------------------------------------------------------------

/// Execution history: remembers which `(schedule_id, due_at)` slots were
/// already dispatched. The check happens **before** enqueueing, so a retry
/// after a crash can never double-enqueue the same slot.
pub trait HistoryStore: Send {
    /// True when this slot was already dispatched.
    fn was_dispatched(&self, schedule_id: &str, due_at: u64) -> Result<bool>;
    /// Record a dispatch. Returns true when newly recorded, false when the
    /// slot was already present (duplicate).
    fn record_dispatch(
        &self,
        schedule_id: &str,
        due_at: u64,
        dispatched_at: u64,
        job_id: &str,
    ) -> Result<bool>;
}

/// Ephemeral in-process history (development/testing only, like the memory
/// queue driver: duplicates are only suppressed within one process).
pub struct MemoryHistory {
    inner: Mutex<HashSet<(String, u64)>>,
}

impl MemoryHistory {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashSet::new()),
        }
    }
}

impl Default for MemoryHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryStore for MemoryHistory {
    fn was_dispatched(&self, schedule_id: &str, due_at: u64) -> Result<bool> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("scheduler history lock poisoned: {e}")))?;
        Ok(inner.contains(&(schedule_id.to_string(), due_at)))
    }

    fn record_dispatch(
        &self,
        schedule_id: &str,
        due_at: u64,
        _dispatched_at: u64,
        _job_id: &str,
    ) -> Result<bool> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("scheduler history lock poisoned: {e}")))?;
        Ok(inner.insert((schedule_id.to_string(), due_at)))
    }
}

/// Persistent single-node history backed by SQLite. Reopening the same file
/// keeps every record, so a restarted scheduler never re-dispatches old slots.
pub struct SqliteHistory {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteHistory {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    PhpxError::Io(format!(
                        "cannot create scheduler history dir {}: {e}",
                        parent.display()
                    ))
                })?;
            }
        }
        let conn = rusqlite::Connection::open(path).map_err(|e| {
            PhpxError::Io(format!(
                "cannot open scheduler history {}: {e}",
                path.display()
            ))
        })?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scheduler_runs (
                schedule_id TEXT NOT NULL,
                due_at INTEGER NOT NULL,
                dispatched_at INTEGER NOT NULL,
                job_id TEXT NOT NULL,
                PRIMARY KEY (schedule_id, due_at)
            );",
        )
        .map_err(|e| PhpxError::Runtime(format!("scheduler history migrate failed: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

use rusqlite::OptionalExtension;

impl HistoryStore for SqliteHistory {
    fn was_dispatched(&self, schedule_id: &str, due_at: u64) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("scheduler history lock poisoned: {e}")))?;
        let row: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM scheduler_runs WHERE schedule_id=?1 AND due_at=?2",
                rusqlite::params![schedule_id, due_at as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| PhpxError::Runtime(format!("scheduler history lookup failed: {e}")))?;
        Ok(row.is_some())
    }

    fn record_dispatch(
        &self,
        schedule_id: &str,
        due_at: u64,
        dispatched_at: u64,
        job_id: &str,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| PhpxError::Runtime(format!("scheduler history lock poisoned: {e}")))?;
        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO scheduler_runs (schedule_id, due_at, dispatched_at, job_id) VALUES (?1,?2,?3,?4)",
                rusqlite::params![schedule_id, due_at as i64, dispatched_at as i64, job_id],
            )
            .map_err(|e| PhpxError::Runtime(format!("scheduler history record failed: {e}")))?;
        Ok(changed == 1)
    }
}

// ---------------------------------------------------------------------------
// Scheduler: single-pass dispatch into the queue
// ---------------------------------------------------------------------------

/// One slot selected for dispatch.
#[derive(Debug, Clone)]
pub struct PlannedDispatch {
    pub schedule_id: String,
    pub due_at: u64,
}

/// Read-only dispatch plan (used by `--dry-run`): what a tick *would* do.
#[derive(Debug, Default)]
pub struct PlanOutcome {
    pub to_dispatch: Vec<PlannedDispatch>,
    pub skipped_duplicate: usize,
    pub skipped_missed: usize,
    pub skipped_disabled: usize,
}

/// One job actually enqueued by a tick.
#[derive(Debug, Clone)]
pub struct Dispatched {
    pub schedule_id: String,
    pub due_at: u64,
    pub job_id: String,
}

/// Result of a dispatch tick.
#[derive(Debug, Default)]
pub struct TickOutcome {
    pub dispatched: Vec<Dispatched>,
    pub skipped_duplicate: usize,
    pub skipped_missed: usize,
    pub skipped_disabled: usize,
}

/// Single-node scheduler. The queue driver and history store are boxed
/// trait objects so tests can use the memory backends while production
/// uses SQLite, without generics leaking into the CLI.
pub struct Scheduler {
    schedules: Vec<ScheduleDef>,
    queue: Box<dyn phpx_queue::QueueDriver>,
    history: Box<dyn HistoryStore>,
}

impl Scheduler {
    /// Build a scheduler. Rejects oversized fleets and duplicate ids.
    /// Callers pass boxed drivers (e.g. `phpx_queue::open_queue(..)` and
    /// `SqliteHistory::open(..)`), so memory and SQLite backends mix freely.
    pub fn new(
        schedules: Vec<ScheduleDef>,
        queue: Box<dyn phpx_queue::QueueDriver>,
        history: Box<dyn HistoryStore>,
    ) -> Result<Self> {
        if schedules.len() > MAX_SCHEDULES {
            return Err(PhpxError::Config(format!(
                "too many schedules: {} (max {MAX_SCHEDULES}, keep the fleet bounded)",
                schedules.len()
            )));
        }
        let mut seen = HashSet::new();
        for sched in &schedules {
            if !seen.insert(sched.id.clone()) {
                return Err(PhpxError::Config(format!(
                    "duplicate schedule id {:?} (ids must be unique)",
                    sched.id
                )));
            }
        }
        Ok(Self {
            schedules,
            queue,
            history,
        })
    }

    pub fn schedules(&self) -> &[ScheduleDef] {
        &self.schedules
    }

    /// Read-only plan for a window: due slots in
    /// `(window_start_excl, window_end_incl]` after dedup. When
    /// `dispatch_missed` is false (or a schedule uses `skip`), past slots
    /// are only counted, never planned.
    pub fn plan_window(
        &self,
        window_start_excl: u64,
        window_end_incl: u64,
        dispatch_missed: bool,
    ) -> Result<PlanOutcome> {
        let mut out = PlanOutcome::default();
        let end_min = ScheduleDef::due_minute(window_end_incl);
        for sched in &self.schedules {
            if !sched.enabled {
                out.skipped_disabled += 1;
                continue;
            }
            // Collect due slots in (window_start_excl, end_min].
            let mut dues: Vec<u64> = Vec::new();
            let mut cursor = window_start_excl;
            for _ in 0..MAX_DUES_PER_TICK {
                let next = match sched.next_after(cursor)? {
                    Some(n) => n,
                    None => break,
                };
                if next > end_min {
                    break;
                }
                dues.push(next);
                cursor = next;
            }
            if dues.is_empty() {
                continue;
            }
            let current = dues.last().copied().filter(|d| *d == end_min);
            let missed: Vec<u64> = dues.into_iter().filter(|d| *d != end_min).collect();
            if let Some(due) = current {
                if self.history.was_dispatched(&sched.id, due)? {
                    out.skipped_duplicate += 1;
                } else {
                    out.to_dispatch.push(PlannedDispatch {
                        schedule_id: sched.id.clone(),
                        due_at: due,
                    });
                }
                // A current dispatch supersedes older missed slots.
                out.skipped_missed += missed.len();
                continue;
            }
            if missed.is_empty() {
                continue;
            }
            if !dispatch_missed || sched.missed_policy != MissedPolicy::DispatchNow {
                out.skipped_missed += missed.len();
                continue;
            }
            // Newest-first catchup, at most one per schedule per tick (bounded).
            let mut remaining: Vec<u64> = Vec::new();
            for due in missed {
                if self.history.was_dispatched(&sched.id, due)? {
                    out.skipped_duplicate += 1;
                } else {
                    remaining.push(due);
                }
            }
            let pending = remaining.len();
            if let Some(latest) = remaining.into_iter().max() {
                out.to_dispatch.push(PlannedDispatch {
                    schedule_id: sched.id.clone(),
                    due_at: latest,
                });
                out.skipped_missed += pending.saturating_sub(1);
            }
        }
        Ok(out)
    }

    /// Execute a dispatch window: plan, then enqueue each planned slot.
    /// History is recorded only after a successful enqueue, so a failed
    /// tick can safely retry without losing or duplicating jobs.
    pub fn tick_window(
        &self,
        window_start_excl: u64,
        window_end_incl: u64,
        dispatch_missed: bool,
    ) -> Result<TickOutcome> {
        let plan = self.plan_window(window_start_excl, window_end_incl, dispatch_missed)?;
        let mut out = TickOutcome {
            skipped_duplicate: plan.skipped_duplicate,
            skipped_missed: plan.skipped_missed,
            skipped_disabled: plan.skipped_disabled,
            dispatched: Vec::with_capacity(plan.to_dispatch.len()),
        };
        for item in plan.to_dispatch {
            let sched = self
                .schedules
                .iter()
                .find(|s| s.id == item.schedule_id)
                .expect("planned dispatch always refers to a known schedule");
            let job = self
                .queue
                .enqueue(&sched.payload, sched.max_retries, 0)
                .map_err(|e| {
                    PhpxError::Runtime(format!(
                        "schedule {:?} dispatch failed (enqueue): {e}",
                        sched.id
                    ))
                })?;
            let fresh =
                self.history
                    .record_dispatch(&sched.id, item.due_at, window_end_incl, &job.id)?;
            if fresh {
                out.dispatched.push(Dispatched {
                    schedule_id: item.schedule_id,
                    due_at: item.due_at,
                    job_id: job.id,
                });
            } else {
                out.skipped_duplicate += 1;
            }
        }
        Ok(out)
    }

    /// Dispatch jobs due in the minute containing `now_unix`
    /// (no missed-run scan; use `tick_window` for catchup).
    pub fn tick_at(&self, now_unix: u64) -> Result<TickOutcome> {
        let end = ScheduleDef::due_minute(now_unix);
        self.tick_window(end.saturating_sub(60), end, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_every_minute_matches_all() {
        let c = CronSpec::parse("* * * * *").expect("parse");
        assert!(c.matches_local(0, 0, 1, 1, 3));
        assert!(c.matches_local(59, 23, 31, 12, 6));
    }

    #[test]
    fn cron_steps_lists_ranges() {
        let c = CronSpec::parse("*/15 9-17 * * MON-FRI").expect("parse");
        assert!(c.matches_local(0, 9, 18, 11, 3));
        assert!(c.matches_local(30, 12, 18, 11, 3));
        assert!(!c.matches_local(7, 12, 18, 11, 3));
        assert!(!c.matches_local(0, 8, 18, 11, 3));
        assert!(!c.matches_local(0, 12, 18, 11, 6));
        assert!(!c.matches_local(0, 12, 18, 11, 0));
    }

    #[test]
    fn cron_dom_dow_or_semantics() {
        // 1st of month OR Sunday at midnight.
        let c = CronSpec::parse("0 0 1 * 0").expect("parse");
        assert!(c.matches_local(0, 0, 1, 6, 3));
        assert!(c.matches_local(0, 0, 8, 6, 0));
        assert!(!c.matches_local(0, 0, 8, 6, 3));
        // dom-only restriction: dow star means dom decides.
        let d = CronSpec::parse("0 0 15 * *").expect("parse");
        assert!(d.matches_local(0, 0, 15, 6, 3));
        assert!(!d.matches_local(0, 0, 16, 6, 3));
    }

    #[test]
    fn cron_sunday7_equals_sunday0() {
        let a = CronSpec::parse("0 0 * * 0").expect("parse");
        let b = CronSpec::parse("0 0 * * 7").expect("parse");
        for dow in 0..7 {
            assert_eq!(
                a.matches_local(0, 0, 18, 11, dow),
                b.matches_local(0, 0, 18, 11, dow),
                "dow {dow}"
            );
        }
    }

    #[test]
    fn cron_rejects_bad_shapes() {
        for bad in [
            "",
            "* * * *",
            "* * * * * *",
            "61 * * * *",
            "*/0 * * * *",
            "5-2 * * * *",
            "* * * * FUNDAY",
            "*, * * * *",
        ] {
            assert!(CronSpec::parse(bad).is_err(), "expected error for {bad:?}");
        }
    }

    #[test]
    fn timezone_shapes() {
        assert!(matches!(parse_timezone("").unwrap(), ScheduleTz::Utc));
        assert!(matches!(parse_timezone("UTC").unwrap(), ScheduleTz::Utc));
        assert!(matches!(parse_timezone("z").unwrap(), ScheduleTz::Utc));
        assert!(matches!(
            parse_timezone("local").unwrap(),
            ScheduleTz::Local
        ));
        assert!(matches!(
            parse_timezone("Asia/Jakarta").unwrap(),
            ScheduleTz::Iana(_)
        ));
        assert!(matches!(
            parse_timezone("+07:00").unwrap(),
            ScheduleTz::Fixed(_)
        ));
        assert!(parse_timezone("Bogus/Zone").is_err());
    }

    #[test]
    fn jakarta_midnight_is_1700z() {
        // 2026-11-17 17:00:00Z == 2026-11-18 00:00 WIB (no DST in WIB).
        let midnight_wib: u64 = 1_794_934_800;
        let tz = parse_timezone("Asia/Jakarta").expect("tz");
        let parts = tz.local_parts(midnight_wib).expect("parts");
        assert_eq!(parts, (0, 0, 18, 11, 3));
        let fixed = parse_timezone("+07:00").expect("offset");
        assert_eq!(fixed.local_parts(midnight_wib).expect("parts"), parts);
        assert_eq!(
            parse_timezone("UTC")
                .expect("utc")
                .local_parts(midnight_wib)
                .expect("parts"),
            (0, 17, 17, 11, 2)
        );
    }

    #[test]
    fn schedule_validation_is_actionable() {
        assert!(ScheduleDef::new("a", "* * * * *", "UTC", "{}", true, "skip", 3).is_ok());
        assert!(ScheduleDef::new("", "* * * * *", "UTC", "{}", true, "skip", 3).is_err());
        assert!(ScheduleDef::new("a", "nope", "UTC", "{}", true, "skip", 3).is_err());
        assert!(ScheduleDef::new("a", "* * * * *", "Mars/Olympus", "{}", true, "skip", 3).is_err());
        assert!(ScheduleDef::new("a", "* * * * *", "UTC", "", true, "skip", 3).is_err());
        assert!(ScheduleDef::new("a", "* * * * *", "UTC", "{}", true, "sometimes", 3).is_err());
        assert!(ScheduleDef::new("a", "* * * * *", "UTC", "{}", true, "skip", 26).is_err());
    }
}
