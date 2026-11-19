//! M6 Point 10: crash containment plus resource limits.
//!
//! Containment order: validate envelope, enter single-entry domain,
//! enforce size caps, then report unlinked. A crashing engine can never
//! take down the gateway because no engine is entered in this build.

use crate::EmbeddedDomain;
use phpx_core::{PhpxError, Result};

/// Resource limits for one embedded entry attempt.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_frame_bytes: usize,
    pub max_wall_ms: u64,
    pub single_entry: bool,
}

impl ResourceLimits {
    pub fn standard() -> Self {
        Self {
            max_frame_bytes: phpx_runtime::MAX_FRAME_BYTES,
            max_wall_ms: 30_000,
            single_entry: true,
        }
    }
}

/// Guarded entry: envelope first, then domain, then size cap.
pub fn execute_guarded(
    domain: &EmbeddedDomain,
    req: &phpx_runtime::PhpRequest,
) -> Result<phpx_runtime::PhpResponse> {
    req.validate()?;
    let _guard = domain.try_enter()?;
    if req.body.len() > phpx_runtime::MAX_FRAME_BYTES {
        return Err(PhpxError::Runtime(format!(
            "embedded containment rejects oversize body {n}: cap is {c}",
            n = req.body.len(),
            c = phpx_runtime::MAX_FRAME_BYTES
        )));
    }
    // Link-aware hold message: under PHPX_EMBED_LINK=1 libphp IS linked,
    // but no live engine is entered until M6 gates pass. Gateway stays up either way.
    if crate::is_build_linked() {
        return Err(PhpxError::Runtime(
            "embedded containment holds: engine entry closed until M6 compat/isolation/stability gates pass (linked=true, no live engine hosted; gateway stays up). Use runtime.mode=process".to_string(),
        ));
    }
    Err(PhpxError::Runtime(
        "embedded containment holds: native php-embed not linked, gateway stays up. Use process runtime or build on Linux with php-embed dev libs".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_limits_match_runtime_cap() {
        let l = ResourceLimits::standard();
        assert_eq!(l.max_frame_bytes, phpx_runtime::MAX_FRAME_BYTES);
        assert!(l.single_entry);
    }

    #[test]
    fn valid_envelope_holds_with_gateway_up() {
        let d = EmbeddedDomain::new();
        let r = phpx_runtime::PhpRequest::minimal("rid-c", "app", "index.php");
        let e = execute_guarded(&d, &r).unwrap_err().to_string();
        assert!(e.contains("gateway stays up"));
    }

    #[test]
    fn concurrent_entry_rejected() {
        let d = EmbeddedDomain::new();
        let _g = d.try_enter().expect("first entry ok");
        let r = phpx_runtime::PhpRequest::minimal("rid-c2", "app", "index.php");
        let e = execute_guarded(&d, &r).unwrap_err().to_string();
        assert!(e.contains("concurrent"));
    }

    #[test]
    fn boundary_body_at_cap_passes_size_check() {
        // Exactly MAX_FRAME_BYTES is NOT oversize: it must fall through to the
        // hold error (proves the cap is exact, no off-by-one), never a cap rejection.
        let d = EmbeddedDomain::new();
        let mut r = phpx_runtime::PhpRequest::minimal("rid-edge", "app", "index.php");
        r.body = vec![0u8; phpx_runtime::MAX_FRAME_BYTES];
        let e = execute_guarded(&d, &r).unwrap_err().to_string();
        assert!(
            !e.contains("oversize"),
            "exact-cap body must not be cap-rejected: {e}"
        );
        assert!(
            e.contains("gateway stays up"),
            "exact-cap body must reach hold: {e}"
        );
    }

    #[test]
    fn oversize_body_rejected() {
        let d = EmbeddedDomain::new();
        let mut r = phpx_runtime::PhpRequest::minimal("rid-big", "app", "index.php");
        r.body = vec![0u8; phpx_runtime::MAX_FRAME_BYTES + 1];
        let e = execute_guarded(&d, &r).unwrap_err().to_string();
        assert!(
            e.contains("MAX_FRAME_BYTES"),
            "cap rejection must name the cap: {e}"
        );
    }

    #[test]
    fn junk_envelope_rejected_before_domain_entry() {
        // Ordering: validation runs BEFORE try_enter, so junk never occupies
        // the single-entry domain (no exhaustion by malformed frames).
        let d = EmbeddedDomain::new();
        let _g = d.try_enter().expect("first entry ok");
        let mut r = phpx_runtime::PhpRequest::minimal("", "app", "index.php");
        r.request_id.clear();
        let e = execute_guarded(&d, &r).unwrap_err().to_string();
        assert!(
            !e.contains("concurrent"),
            "envelope error must precede domain error: {e}"
        );
    }

    #[test]
    fn guard_release_allows_reentry_no_deadlock() {
        let d = EmbeddedDomain::new();
        {
            let _g = d.try_enter().expect("enter ok");
        }
        d.try_enter()
            .expect("re-entry after clean release must succeed");
    }

    #[test]
    fn engine_panic_is_contained_gateway_survives() {
        // Crash containment: a panic while the domain is held poisons the
        // mutex, but the gateway thread survives and the NEXT try_enter
        // reports a graceful poisoned error instead of hanging or crashing.
        use std::sync::Arc;
        let d = Arc::new(EmbeddedDomain::new());
        let d2 = d.clone();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = d2.try_enter().expect("enter ok");
            panic!("simulated engine crash");
        }));
        assert!(r.is_err(), "the panic must have happened");
        let e = match d.try_enter() {
            Ok(_) => panic!("post-crash entry must fail with poisoned error"),
            Err(e) => e.to_string(),
        };
        assert!(
            e.contains("poisoned"),
            "post-crash entry must report poison, not hang: {e}"
        );
    }
}
