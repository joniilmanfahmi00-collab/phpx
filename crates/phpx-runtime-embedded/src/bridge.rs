//! M6 Point 3: narrow C ABI bridge surface.
//!
//! The bridge allowlist is intentionally tiny: startup, execute_once,
//! shutdown. Everything else stays out. Calls validate the op name plus
//! the envelope first, then return an actionable unlinked error until
//! a Linux native lib is wired.

use phpx_core::{PhpxError, Result};

/// Bridge ABI version. Bump only with a matching native lib.
pub const BRIDGE_VERSION: u32 = 1;

/// Allowed ops. Narrow on purpose.
pub const ALLOWED_OPS: [&str; 3] = ["startup", "execute_once", "shutdown"];

/// C compatible request header passed across the bridge boundary.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BridgeRequest {
    pub version: u32,
    pub script_len: usize,
    pub input_len: usize,
}

/// C compatible response header passed back across the boundary.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BridgeResponse {
    pub version: u32,
    pub status: u16,
    pub body_len: usize,
}

pub fn is_allowed(op: &str) -> bool {
    ALLOWED_OPS.contains(&op)
}

/// Stub call: validates op plus envelope, then reports unlinked.
pub fn call_stub(op: &str, req: &phpx_runtime::PhpRequest) -> Result<phpx_runtime::PhpResponse> {
    if !is_allowed(op) {
        return Err(PhpxError::Runtime(format!(
            "embedded bridge rejects unknown op {o}: allowed ops are startup, execute_once, shutdown",
            o = op
        )));
    }
    req.validate()?;
    Err(PhpxError::Runtime(
        "embedded bridge unlinked in this build: native php-embed not linked. Use process runtime or build on Linux with php-embed dev libs".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_is_narrow() {
        assert!(is_allowed("startup"));
        assert!(is_allowed("execute_once"));
        assert!(is_allowed("shutdown"));
        assert!(!is_allowed("eval"));
        assert!(!is_allowed("execute_raw"));
    }

    #[test]
    fn unknown_op_rejected_before_envelope() {
        let r = phpx_runtime::PhpRequest::minimal("rid-1", "app", "index.php");
        let e = call_stub("eval", &r).unwrap_err().to_string();
        assert!(e.contains("unknown op"));
    }

    #[test]
    fn known_op_with_valid_envelope_reports_unlinked() {
        let r = phpx_runtime::PhpRequest::minimal("rid-2", "app", "index.php");
        let e = call_stub("execute_once", &r).unwrap_err().to_string();
        assert!(e.contains("unlinked"));
    }

    #[test]
    fn known_op_with_bad_envelope_fails_validation_first() {
        let mut r = phpx_runtime::PhpRequest::minimal("", "app", "index.php");
        r.request_id.clear();
        assert!(call_stub("execute_once", &r).is_err());
    }
}
