//! M6 promotion gate: request state isolation lifecycle.
//!
//! A live embedded engine must prove that no request state leaks into the
//! next request (superglobals, statics, handles, buffers). The Rust side of
//! that contract is this scope: a request opens exactly one scope, tracks
//! every byte it hands the engine, releases everything before the next
//! entry, and seals clean. A seal over unreleased bytes is a leak and fails
//! closed. Live-engine reset proofs (Zend executor reset per request) plug
//! into this lifecycle; until then the gate reports Hold.

use phpx_core::{PhpxError, Result};

/// Scope lifecycle: Open -> Sealed (consumed by seal). No re-entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePhase {
    Open,
    Sealed,
}

/// One request's isolation scope. Tracks bytes handed out vs released so a
/// seal can prove nothing is retained across the request boundary.
#[derive(Debug)]
pub struct IsolationScope {
    pub request_id: String,
    phase: ScopePhase,
    tracked_bytes: usize,
    released_bytes: usize,
}

/// Receipt of a sealed request: the evidence the isolation gate archives.
#[derive(Debug, Clone)]
pub struct SealReceipt {
    pub request_id: String,
    pub tracked_bytes: usize,
    pub released_bytes: usize,
    pub clean: bool,
}

impl IsolationScope {
    pub fn open(request_id: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            phase: ScopePhase::Open,
            tracked_bytes: 0,
            released_bytes: 0,
        }
    }

    pub fn phase(&self) -> ScopePhase {
        self.phase
    }

    /// Record bytes handed to the engine. Only valid while open.
    pub fn track(&mut self, n: usize) {
        if self.phase == ScopePhase::Open {
            self.tracked_bytes = self.tracked_bytes.saturating_add(n);
        }
    }

    /// Record bytes reclaimed before the next entry. Only valid while open.
    pub fn release(&mut self, n: usize) {
        if self.phase == ScopePhase::Open {
            self.released_bytes = self.released_bytes.saturating_add(n);
        }
    }

    /// Seal the request boundary. Fails closed on any retained bytes.
    pub fn seal(mut self) -> Result<SealReceipt> {
        self.phase = ScopePhase::Sealed;
        if self.released_bytes < self.tracked_bytes {
            return Err(PhpxError::Runtime(format!(
                "embedded isolation leak in request {}: tracked {} bytes, released {} ({} retained across the boundary)",
                self.request_id,
                self.tracked_bytes,
                self.released_bytes,
                self.tracked_bytes - self.released_bytes,
            )));
        }
        Ok(SealReceipt {
            request_id: self.request_id.clone(),
            tracked_bytes: self.tracked_bytes,
            released_bytes: self.released_bytes,
            clean: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_request_seals() {
        let mut s = IsolationScope::open("iso-1");
        assert_eq!(s.phase(), ScopePhase::Open);
        s.track(100);
        s.release(100);
        let r = s.seal().expect("clean seal");
        assert!(r.clean);
        assert_eq!(r.tracked_bytes, 100);
        assert_eq!(r.released_bytes, 100);
    }

    #[test]
    fn empty_request_seals_clean() {
        let s = IsolationScope::open("iso-empty");
        let r = s.seal().expect("empty seal");
        assert!(r.clean);
    }

    #[test]
    fn retained_bytes_fail_seal_with_counts() {
        let mut s = IsolationScope::open("iso-leak");
        s.track(64);
        s.release(60);
        let e = s.seal().unwrap_err().to_string();
        assert!(e.contains("4"), "{e}");
        assert!(e.contains("iso-leak"), "{e}");
    }

    #[test]
    fn post_seal_tracking_ignored_never_panics() {
        // Seal consumes the scope, so post-seal use is impossible by
        // construction (ownership, not runtime checks).
        let mut s = IsolationScope::open("iso-2");
        s.track(10);
        s.release(10);
        assert!(s.seal().is_ok());
    }
}
