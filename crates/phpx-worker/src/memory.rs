/// P0-C: OS-level resident-set sampling + soft/hard policy classification.
///
/// RSS (resident set size) is sampled from the OS per supervised worker PID.
/// It is NOT the same as the PHP heap: the Zend allocator, native
/// extensions, shared libraries and fragmentation all affect observed RSS.
/// A stable RSS does not prove there is no leak; a high RSS does not prove
/// one. These samples drive recycling/protection policy, never leak claims.
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// One OS RSS observation.
#[derive(Debug, Clone, Copy)]
pub struct MemorySample {
    pub rss_bytes: u64,
}

/// Sample current RSS for `pid`. Returns None when the process is gone or
/// the platform cannot report it (never panics, never blocks long).
pub fn sample_rss(pid: u32) -> Option<MemorySample> {
    let mut sys = System::new();
    let pid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    sys.process(pid).map(|p| MemorySample {
        rss_bytes: p.memory(),
    })
}

/// Policy classification for one RSS observation.
///
/// - `Below`: under both thresholds (or threshold disabled = 0).
/// - `Soft`: at/over soft but under hard -> graceful recycle.
/// - `Hard`: at/over hard -> protective termination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryClass {
    Below,
    Soft,
    Hard,
}

pub fn classify(rss_bytes: u64, soft_bytes: u64, hard_bytes: u64) -> MemoryClass {
    if hard_bytes > 0 && rss_bytes >= hard_bytes {
        MemoryClass::Hard
    } else if soft_bytes > 0 && rss_bytes >= soft_bytes {
        MemoryClass::Soft
    } else {
        MemoryClass::Below
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_boundaries_are_exact() {
        // Disabled thresholds never fire.
        assert_eq!(classify(0, 0, 0), MemoryClass::Below);
        assert_eq!(classify(u64::MAX, 0, 0), MemoryClass::Below);
        // Soft-only policy.
        assert_eq!(classify(7, 8, 0), MemoryClass::Below);
        assert_eq!(classify(8, 8, 0), MemoryClass::Soft);
        assert_eq!(classify(1_000_000, 8, 0), MemoryClass::Soft);
        // Soft + hard: hard wins at/over the hard line.
        assert_eq!(classify(8, 8, 48), MemoryClass::Soft);
        assert_eq!(classify(47, 8, 48), MemoryClass::Soft);
        assert_eq!(classify(48, 8, 48), MemoryClass::Hard);
        assert_eq!(classify(u64::MAX, 8, 48), MemoryClass::Hard);
        // Hard-only policy.
        assert_eq!(classify(15, 0, 16), MemoryClass::Below);
        assert_eq!(classify(16, 0, 16), MemoryClass::Hard);
    }

    #[test]
    fn sample_current_process_succeeds() {
        // Smoke: our own test process must have a nonzero RSS.
        let sample = sample_rss(std::process::id());
        let sample = sample.expect("must sample own RSS");
        assert!(sample.rss_bytes > 0, "RSS must be nonzero");
    }

    #[test]
    fn sample_gone_pid_returns_none() {
        // PID 0 / absurd PIDs must not panic and must not return data.
        // (u32::MAX is never a live PID on supported platforms.)
        assert!(sample_rss(u32::MAX).is_none());
    }
}
