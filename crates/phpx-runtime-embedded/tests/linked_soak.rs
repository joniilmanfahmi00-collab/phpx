//! P2 promotion: linked-engine soak (stability evidence).
//!
//! Runs ONLY under `PHPX_EMBED_LINK=1` on Linux NTS. Workload: hundreds of
//! mixed evals with periodic request cycles, contained bailouts with
//! immediate reuse, and a final parity include — asserting zero unexpected
//! failures, consecutive-clean accounting, and a bounded RSS trend. Archives
//! a JSON summary + CSV beside the process soak-long convention; the
//! stability gate reads it back (see `gates::stability`).
//!
//! Archive layout (gitignored `data/bench/`, never committed):
//!   data/bench/soak-embed-<UTC>/SUMMARY.json   (machine-readable record)
//!   data/bench/soak-embed-<UTC>/rss.csv        (t,start/end samples)
//!   data/bench/soak-embed-latest/SUMMARY.json  (copy the gate reads)

#![cfg(phpx_embed_linked)]

use phpx_runtime_embedded as emb;
use std::time::Instant;

const ITERS: usize = 300;
/// Hard RSS growth bound for the whole soak, measured steady-state (after
/// warmup): WSL runs show <1 MB over 300 mixed evals, so 10 MB is generous
/// headroom that still catches a real per-request leak. Never an SLA.
const RSS_GROWTH_CAP_BYTES: u64 = 10 * 1024 * 1024;

fn rss_bytes() -> Option<u64> {
    // /proc/self/statm field 2 = resident pages.
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    let page = 4096u64;
    Some(pages.saturating_mul(page))
}

fn archive_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("PHPX_EMBED_SOAK_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../data/bench/soak-embed-{ts}"))
}

#[test]
fn linked_engine_soak() {
    let t0 = Instant::now();
    // Warm up first: engine init plus first-request allocations are one-time
    // costs, not a trend. The growth bound below measures steady state only.
    emb::live::next_request().expect("soak must cycle");
    for i in 0..20 {
        if i % 2 == 0 {
            emb::live::eval_expr(&format!("{i}*3")).expect("warmup eval");
        } else {
            emb::live::execute_once(&format!("echo 'w{i}';")).expect("warmup eval");
        }
    }
    emb::live::next_request().expect("soak must cycle");
    let rss_start = rss_bytes();
    let mut clean = 0usize;
    let mut bails = 0usize;
    for i in 0..ITERS {
        match i % 10 {
            // Periodic request cycle (fresh executor every 10 evals).
            9 => {
                emb::live::next_request().expect("soak cycle");
                clean += 1;
            }
            // Contained fatal every 50th: must come back as status 1 and
            // the very next eval must serve (bailout + immediate reuse).
            5 if i % 50 == 5 => {
                let bad = emb::live::execute_once("throw new Exception('soak');")
                    .expect("soak eval runs");
                assert_eq!(bad.status, 1, "soak bailout must be contained");
                bails += 1;
                let ok = emb::live::eval_expr("7*8").expect("reuse after bailout");
                assert_eq!(ok.status, 0);
                assert_eq!(ok.ret_long, Some(56));
                clean += 2;
            }
            // Arithmetic with return value.
            n if n % 2 == 0 => {
                let out = emb::live::eval_expr(&format!("{i}*2")).expect("soak eval");
                assert_eq!(out.status, 0, "soak eval {i} must succeed");
                assert_eq!(out.ret_long, Some((i as i64) * 2));
                clean += 1;
            }
            // Echo output capture.
            _ => {
                let out = emb::live::execute_once(&format!("echo 's{i}';")).expect("soak eval");
                assert_eq!(out.status, 0, "soak eval {i} must succeed");
                assert_eq!(out.output, format!("s{i}").into_bytes());
                clean += 1;
            }
        }
    }
    // Final parity include on the soaked engine.
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/vanilla-php/public/index.php")
        .to_string_lossy()
        .to_string();
    emb::live::next_request().expect("fresh request for parity");
    let out = emb::live::execute_once(&format!("include {root:?};")).expect("parity include");
    assert_eq!(out.status, 0);
    assert!(String::from_utf8_lossy(&out.output).contains("hello from phpx vanilla"));
    clean += 1;

    let elapsed = t0.elapsed();
    let rss_end = rss_bytes();
    let rss_growth = match (rss_start, rss_end) {
        (Some(a), Some(b)) => b.saturating_sub(a),
        _ => 0,
    };
    assert!(
        rss_growth <= RSS_GROWTH_CAP_BYTES,
        "soak RSS grew {rss_growth} bytes (cap {RSS_GROWTH_CAP_BYTES})"
    );
    let date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Archive the record (gitignored data/bench, like the process soak-long).
    let dir = archive_dir();
    let _ = std::fs::create_dir_all(&dir);
    let summary = format!(
        "{{\n  \"schema\": \"phpx-embed-soak/1\",\n  \"date\": {date},\n  \"iters\": {ITERS},\n  \"clean\": {clean},\n  \"bailouts_contained\": {bails},\n  \"failures\": 0,\n  \"elapsed_ms\": {},\n  \"rss_start_bytes\": {},\n  \"rss_end_bytes\": {},\n  \"rss_growth_bytes\": {rss_growth},\n  \"rss_cap_bytes\": {RSS_GROWTH_CAP_BYTES}\n}}\n",
        elapsed.as_millis(),
        rss_start.unwrap_or(0),
        rss_end.unwrap_or(0),
    );
    let _ = std::fs::write(dir.join("SUMMARY.json"), &summary);
    let _ = std::fs::write(
        dir.join("rss.csv"),
        format!(
            "phase,rss_bytes\nstart,{}\nend,{}\n",
            rss_start.unwrap_or(0),
            rss_end.unwrap_or(0)
        ),
    );
    // Copy the gate reads (best effort).
    let latest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/bench/soak-embed-latest");
    let _ = std::fs::create_dir_all(&latest);
    let _ = std::fs::write(latest.join("SUMMARY.json"), &summary);
    // The gate flip, end to end: the archive this test just wrote must make
    // the stability gate pass in this same linked process.
    let gates = emb::gates::assess();
    let stability = gates.iter().find(|g| g.id == "stability").expect("gate");
    assert!(
        stability.pass,
        "soak archive must flip the stability gate: {}",
        stability.evidence
    );
    eprintln!(
        "embed soak: {clean} clean, {bails} bailouts contained, 0 failures, rss +{rss_growth}B in {:?} -> {}",
        elapsed,
        dir.display()
    );
}
