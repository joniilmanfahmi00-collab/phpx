//! PHPX embedded runtime (M6 experimental): capability reporting + single-entry guard.
//!
//! SPEC 3.1, M6: opt-in, never default. No native PHP linking in v0.1
//! (needs Linux php-embed dev libs + ABI pin). Process Runtime stays active;
//! embedded returns actionable errors until linked.

use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, TryLockError};

pub mod abi;
pub mod bridge;
pub mod compat;
pub mod containment;
pub mod link;
pub mod ownership;

/// Capability report for runtime list|inspect and docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedCapabilities {
    pub available: bool,
    pub enabled: bool,
    pub single_threaded: bool,
    pub max_concurrent_entries: usize,
    pub nts_only: bool,
    pub note: String,
}

impl EmbeddedCapabilities {
    pub fn report(enabled: bool) -> Self {
        Self {
            available: false,
            enabled,
            single_threaded: true,
            max_concurrent_entries: 1,
            nts_only: true,
            note: if enabled {
                "enabled=true but native php-embed lib not linked in this build (M6.1 stub)"
                    .to_string()
            } else {
                "disabled by default; enable with [runtime.embedded] enabled=true + runtime.mode=embedded (experimental)".to_string()
            },
        }
    }
}

/// Single execution domain: at most one PHP engine entry at a time.
/// M6 exit: no arbitrary concurrent PHP engine entry.
#[derive(Debug, Default)]
pub struct EmbeddedDomain {
    inner: Mutex<()>,
}

impl EmbeddedDomain {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(()),
        }
    }

    /// Try to enter the domain. Concurrent entry is rejected (backpressure).
    pub fn try_enter(&self) -> Result<EmbeddedGuard<'_>> {
        match self.inner.try_lock() {
            Ok(g) => Ok(EmbeddedGuard { _guard: g }),
            Err(TryLockError::WouldBlock) => Err(PhpxError::Runtime(
                "embedded runtime busy: concurrent PHP engine entry rejected (single-threaded domain)".to_string(),
            )),
            Err(TryLockError::Poisoned(_)) => Err(PhpxError::Runtime(
                "embedded runtime poisoned after panic".to_string(),
            )),
        }
    }
}

pub struct EmbeddedGuard<'a> {
    _guard: std::sync::MutexGuard<'a, ()>,
}

/// Stub execution: always actionable error until native linking lands.
/// Never panics; validates envelope first so contract tests stay meaningful.
pub fn execute_stub(req: &phpx_runtime::PhpRequest) -> Result<phpx_runtime::PhpResponse> {
    req.validate()?;
    Err(PhpxError::Runtime(
        "embedded runtime unavailable in this build: native php-embed not linked (M6 experimental). Use runtime.mode=process or build on Linux with php-embed dev libs".to_string(),
    ))
}

/// Status line for CLI human output.
pub fn status_line(enabled: bool) -> String {
    let c = EmbeddedCapabilities::report(enabled);
    format!(
        "embedded - experimental-{} (available={}, single-threaded, max_entries=1)",
        if enabled { "enabled" } else { "disabled" },
        c.available
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_default_disabled() {
        let c = EmbeddedCapabilities::report(false);
        assert!(!c.available);
        assert!(!c.enabled);
        assert!(c.single_threaded);
        assert_eq!(c.max_concurrent_entries, 1);
    }

    #[test]
    fn domain_rejects_concurrent_entry() {
        let d = EmbeddedDomain::new();
        let _g = d.try_enter().expect("first entry ok");
        assert!(d.try_enter().is_err());
    }

    #[test]
    fn stub_rejects_bad_envelope_first() {
        let mut r = phpx_runtime::PhpRequest::minimal("", "app", "index.php");
        r.request_id.clear();
        assert!(execute_stub(&r).is_err());
    }

    #[test]
    fn stub_unavailable_with_valid_envelope() {
        let r = phpx_runtime::PhpRequest::minimal("rid-1", "app", "index.php");
        let e = execute_stub(&r).unwrap_err().to_string();
        assert!(e.contains("php-embed not linked"));
    }
}
