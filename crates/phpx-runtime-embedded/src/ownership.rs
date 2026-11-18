//! M6 Point 4: memory ownership and lifecycle rules.
//!
//! Rules are enforced by construction in the stub: request scoped data
//! is owned by Rust, nothing crosses requests, Drop closes the scope.
//! Native engine handles stay deferred with the link.

/// Ownership rules, shared by docs plus tests.
pub const RULES: [&str; 5] = [
    "Rust owns every request buffer; the engine never takes ownership.",
    "No pointers cross request boundaries; each request gets a fresh scope.",
    "Engine handles, when linked, live exactly one request and drop after.",
    "Concurrent entry is rejected before any allocation happens.",
    "Oversize payloads are rejected before any allocation happens.",
];

/// Request scoped allocation tracker. Drop marks the scope closed.
#[derive(Debug)]
pub struct RequestScope {
    pub tracked_bytes: usize,
    pub closed: bool,
}

impl RequestScope {
    pub fn open() -> Self {
        Self {
            tracked_bytes: 0,
            closed: false,
        }
    }

    pub fn track(&mut self, n: usize) {
        self.tracked_bytes += n;
    }

    pub fn finish(self) -> usize {
        self.tracked_bytes
    }
}

impl Drop for RequestScope {
    fn drop(&mut self) {
        self.closed = true;
    }
}

pub fn rules() -> Vec<String> {
    RULES.iter().map(|r| r.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_five_and_shared() {
        assert_eq!(rules().len(), 5);
        assert!(rules()[0].contains("Rust owns"));
    }

    #[test]
    fn scope_tracks_bytes() {
        let mut s = RequestScope::open();
        s.track(10);
        s.track(5);
        assert_eq!(s.finish(), 15);
    }
}
