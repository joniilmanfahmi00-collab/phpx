use chrono::{TimeZone, Utc};
use phpx_queue::MemoryQueue;
use phpx_scheduler::{
    HistoryStore, MemoryHistory, MissedPolicy, ScheduleDef, Scheduler, SqliteHistory,
};

fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> u64 {
    Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
        .single()
        .expect("valid test time")
        .timestamp() as u64
}

fn every_minute(id: &str) -> ScheduleDef {
    ScheduleDef::new(id, "* * * * *", "UTC", r#"{"job":"tick"}"#, true, "skip", 3)
        .expect("valid schedule")
}

fn nightly_jakarta() -> ScheduleDef {
    ScheduleDef::new(
        "nightly",
        "0 2 * * *",
        "Asia/Jakarta",
        r#"{"job":"nightly"}"#,
        true,
        "skip",
        3,
    )
    .expect("valid schedule")
}

#[test]
fn tick_dispatches_due_job_to_queue() {
    let now = utc(2026, 11, 18, 10, 0);
    let queue = MemoryQueue::new(100);
    let history = MemoryHistory::new();
    let sched = Scheduler::new(
        vec![every_minute("tick")],
        Box::new(queue),
        Box::new(history),
    )
    .expect("scheduler");
    // Scheduler borrows queue/history by value; use a fresh pair via plan+tick on refs:
    // (tick consumes through &self, so re-check through status below needs access;
    // instead assert on the returned outcome.)
    let out = sched.tick_at(now).expect("tick");
    assert_eq!(out.dispatched.len(), 1);
    assert_eq!(out.dispatched[0].schedule_id, "tick");
    assert_eq!(out.dispatched[0].due_at, now / 60 * 60);
    assert!(!out.dispatched[0].job_id.is_empty());
}

#[test]
fn duplicate_dispatch_suppressed_within_same_minute() {
    let now = utc(2026, 11, 18, 10, 0);
    let queue = MemoryQueue::new(100);
    let history = MemoryHistory::new();
    let sched = Scheduler::new(
        vec![every_minute("tick")],
        Box::new(queue),
        Box::new(history),
    )
    .expect("scheduler");
    let first = sched.tick_at(now).expect("tick1");
    assert_eq!(first.dispatched.len(), 1);
    let second = sched.tick_at(now + 20).expect("tick2");
    assert!(second.dispatched.is_empty());
    assert_eq!(second.skipped_duplicate, 1);
}

#[test]
fn non_due_schedule_dispatches_nothing() {
    // 02:00 Jakarta == 19:00Z previous day; at 10:00Z nothing is due.
    let now = utc(2026, 11, 18, 10, 0);
    let queue = MemoryQueue::new(100);
    let history = MemoryHistory::new();
    let sched = Scheduler::new(vec![nightly_jakarta()], Box::new(queue), Box::new(history))
        .expect("scheduler");
    let out = sched.tick_at(now).expect("tick");
    assert!(out.dispatched.is_empty());
    // ...but at the due minute it fires exactly once.
    let due = utc(2026, 11, 17, 19, 0);
    let out = sched.tick_at(due).expect("tick due");
    assert_eq!(out.dispatched.len(), 1);
    assert_eq!(out.dispatched[0].schedule_id, "nightly");
}

#[test]
fn missed_skip_counts_and_dispatch_now_catches_up_once() {
    // Hourly schedule; window covers 3 missed hours, current minute not due.
    let mk = |policy: &str| {
        ScheduleDef::new("hourly", "0 * * * *", "UTC", "{}", true, policy, 1).expect("schedule")
    };
    let end = utc(2026, 11, 18, 10, 30);
    let start = end - 3 * 3600;

    let q1 = MemoryQueue::new(100);
    let h1 = MemoryHistory::new();
    let skip = Scheduler::new(vec![mk("skip")], Box::new(q1), Box::new(h1)).expect("scheduler");
    let out = skip.tick_window(start, end, true).expect("tick");
    assert!(out.dispatched.is_empty());
    assert_eq!(out.skipped_missed, 3);

    let q2 = MemoryQueue::new(100);
    let h2 = MemoryHistory::new();
    let catchup =
        Scheduler::new(vec![mk("dispatch-now")], Box::new(q2), Box::new(h2)).expect("scheduler");
    let out = catchup.tick_window(start, end, true).expect("tick");
    // Bounded: exactly one catchup job (newest missed slot), never three.
    assert_eq!(out.dispatched.len(), 1);
    assert_eq!(out.dispatched[0].due_at, utc(2026, 11, 18, 10, 0));
    assert_eq!(out.skipped_missed, 2);
}

#[test]
fn catchup_disabled_without_window_flag() {
    let mk = ScheduleDef::new("hourly", "0 * * * *", "UTC", "{}", true, "dispatch-now", 1)
        .expect("schedule");
    let end = utc(2026, 11, 18, 10, 30);
    let queue = MemoryQueue::new(100);
    let history = MemoryHistory::new();
    let sched = Scheduler::new(vec![mk], Box::new(queue), Box::new(history)).expect("scheduler");
    let out = sched.tick_window(end - 3 * 3600, end, false).expect("tick");
    assert!(out.dispatched.is_empty());
    assert_eq!(out.skipped_missed, 3);
}

#[test]
fn disabled_schedules_are_counted_not_dispatched() {
    let mut s = every_minute("paused");
    s.enabled = false;
    let now = utc(2026, 11, 18, 10, 0);
    let queue = MemoryQueue::new(100);
    let history = MemoryHistory::new();
    let sched = Scheduler::new(vec![s], Box::new(queue), Box::new(history)).expect("scheduler");
    let out = sched.tick_at(now).expect("tick");
    assert!(out.dispatched.is_empty());
    assert_eq!(out.skipped_disabled, 1);
}

#[test]
fn duplicate_ids_rejected() {
    let queue = MemoryQueue::new(10);
    let history = MemoryHistory::new();
    let res = Scheduler::new(
        vec![every_minute("dup"), every_minute("dup")],
        Box::new(queue),
        Box::new(history),
    );
    match res {
        Ok(_) => panic!("duplicate ids must fail"),
        Err(err) => assert!(err.to_string().contains("duplicate"), "{err}"),
    }
}

#[test]
fn sqlite_history_survives_reopen() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("sched.db");
    let now = utc(2026, 11, 18, 10, 0);
    {
        let history = SqliteHistory::open(&path).expect("open");
        assert!(!history.was_dispatched("tick", now).expect("lookup"));
        assert!(history
            .record_dispatch("tick", now, now, "job-1")
            .expect("record"));
        assert!(history.was_dispatched("tick", now).expect("lookup2"));
        // Duplicate record returns false.
        assert!(!history
            .record_dispatch("tick", now, now, "job-2")
            .expect("record2"));
    }
    {
        let history = SqliteHistory::open(&path).expect("reopen");
        assert!(history
            .was_dispatched("tick", now)
            .expect("survives restart"));
        // A restarted scheduler with the same history dispatches nothing.
        let queue = MemoryQueue::new(100);
        let sched = Scheduler::new(
            vec![every_minute("tick")],
            Box::new(queue),
            Box::new(history),
        )
        .expect("scheduler");
        let out = sched.tick_at(now).expect("tick");
        assert!(out.dispatched.is_empty());
        assert_eq!(out.skipped_duplicate, 1);
    }
}

#[test]
fn missed_policy_parses_with_alias() {
    assert_eq!(MissedPolicy::parse("skip").unwrap(), MissedPolicy::Skip);
    assert_eq!(MissedPolicy::parse("").unwrap(), MissedPolicy::Skip);
    assert_eq!(
        MissedPolicy::parse("dispatch-now").unwrap(),
        MissedPolicy::DispatchNow
    );
    assert_eq!(
        MissedPolicy::parse("catchup").unwrap(),
        MissedPolicy::DispatchNow
    );
    assert!(MissedPolicy::parse("sometimes").is_err());
}

#[test]
fn dry_run_plan_matches_tick_then_empties() {
    // plan_window is read-only: same plan twice, then tick dispatches it,
    // then the plan is empty (slot now recorded).
    let now = utc(2026, 11, 18, 10, 0);
    let queue = MemoryQueue::new(100);
    let history = MemoryHistory::new();
    let sched = Scheduler::new(
        vec![every_minute("tick")],
        Box::new(queue),
        Box::new(history),
    )
    .expect("scheduler");
    let plan1 = sched
        .plan_window(now / 60 * 60 - 60, now, false)
        .expect("plan1");
    let plan2 = sched
        .plan_window(now / 60 * 60 - 60, now, false)
        .expect("plan2");
    assert_eq!(plan1.to_dispatch.len(), 1);
    assert_eq!(plan2.to_dispatch.len(), 1);
    let tick = sched.tick_at(now).expect("tick");
    assert_eq!(tick.dispatched.len(), 1);
    let plan3 = sched
        .plan_window(now / 60 * 60 - 60, now, false)
        .expect("plan3");
    assert!(plan3.to_dispatch.is_empty());
    assert_eq!(plan3.skipped_duplicate, 1);
}
