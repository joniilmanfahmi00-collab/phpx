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
    fn oversize_body_rejected() {
        let d = EmbeddedDomain::new();
        let mut r = phpx_runtime::PhpRequest::minimal("rid-big", "app", "index.php");
        r.body = vec![0u8; phpx_runtime::MAX_FRAME_BYTES + 1];
        assert!(execute_guarded(&d, &r).is_err());
    }
}
