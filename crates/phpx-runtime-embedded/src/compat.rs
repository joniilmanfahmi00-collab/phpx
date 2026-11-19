//! M6 Point 9: Laravel plus WordPress compatibility subsets for embedded.
//!
//! Subsets mirror the M4 process fixtures. Execution through embedded
//! stays deferred until the native link lands; the stub validates the
//! subset id plus envelope and returns an actionable unlinked error.

use phpx_core::{PhpxError, Result};

/// Subset descriptor.
///
/// `process_fixture` is the M4 fixture proven green through the process
/// runtime. `embedded_blocker` names the exact M6 gate keeping THIS subset
/// out of the embedded engine (explicit per-subset deferral, not vague).
#[derive(Debug, Clone)]
pub struct Subset {
    pub id: String,
    pub app: String,
    pub entry: String,
    pub note: String,
    pub process_fixture: String,
    pub embedded_blocker: String,
}

pub fn subsets() -> Vec<Subset> {
    vec![
        Subset {
            id: "vanilla".to_string(),
            app: "vanilla".to_string(),
            entry: "index.php".to_string(),
            note: "M4 vanilla fixture shape".to_string(),
            process_fixture: "examples/vanilla-php (m4_compat: isolation + crash-recovery green)".to_string(),
            embedded_blocker: "engine entry closed: isolated Zend request lifecycle not wired (M6 isolation gate)".to_string(),
        },
        Subset {
            id: "laravel-minimal".to_string(),
            app: "laravel".to_string(),
            entry: "public/index.php".to_string(),
            note: "M4 laravel-minimal sim shape: routing plus JSON plus middleware".to_string(),
            process_fixture: "examples/laravel-minimal (m4_compat: routing + JSON green)".to_string(),
            embedded_blocker: "engine entry closed: framework bootstrap needs persistent opcache + env lifecycle (M6 stability gate)".to_string(),
        },
        Subset {
            id: "wordpress-minimal".to_string(),
            app: "wordpress".to_string(),
            entry: "index.php".to_string(),
            note: "M4 wordpress-minimal sim shape: query plus shortcode".to_string(),
            process_fixture: "examples/wordpress-minimal (m4_compat: loop + search + shortcode green)".to_string(),
            embedded_blocker: "engine entry closed: WP_SIM global-state lifecycle per request not wired (M6 isolation gate)".to_string(),
        },
        Subset {
            id: "woocommerce-minimal".to_string(),
            app: "woocommerce".to_string(),
            entry: "index.php".to_string(),
            note: "M4 woocommerce-minimal sim shape: cart plus checkout".to_string(),
            process_fixture: "examples/woocommerce-minimal (m4_compat: cart + checkout JSON green)".to_string(),
            embedded_blocker: "engine entry closed: after WP baseline; session/cart state lifecycle not wired (M6 isolation gate)".to_string(),
        },
    ]
}

pub fn run_subset_stub(
    id: &str,
    req: &phpx_runtime::PhpRequest,
) -> Result<phpx_runtime::PhpResponse> {
    if subsets().iter().all(|s| s.id != id) {
        return Err(PhpxError::Runtime(format!(
            "embedded compat rejects unknown subset {i}: want one of vanilla, laravel-minimal, wordpress-minimal, woocommerce-minimal",
            i = id
        )));
    }
    req.validate()?;
    // Link-aware deferral: linked=true still defers (no live engine), but the
    // reason differs. Both messages keep "deferred" for contract stability.
    if crate::is_build_linked() {
        return Err(PhpxError::Runtime(format!(
            "embedded compat deferred: engine entry closed until M6 gates pass (linked=true). Same subset stays green through process runtime ({f})",
            f = subsets().iter().find(|s| s.id == id).map(|s| s.process_fixture.as_str()).unwrap_or("process")
        )));
    }
    Err(PhpxError::Runtime(
        "embedded compat deferred in this build: native php-embed not linked. Same subset stays green through process runtime".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsets_mirror_m4_fixtures() {
        let ids: Vec<String> = subsets().iter().map(|s| s.id.clone()).collect();
        assert_eq!(
            ids,
            vec![
                "vanilla",
                "laravel-minimal",
                "wordpress-minimal",
                "woocommerce-minimal"
            ]
        );
    }

    #[test]
    fn known_subset_reports_deferred() {
        let r = phpx_runtime::PhpRequest::minimal("rid-9", "app", "index.php");
        let e = run_subset_stub("laravel-minimal", &r)
            .unwrap_err()
            .to_string();
        assert!(e.contains("deferred"));
    }

    #[test]
    fn every_subset_names_fixture_and_blocker() {
        // No vague deferral: each subset must point at its green process
        // fixture AND its exact embedded blocker.
        for s in subsets() {
            assert!(!s.process_fixture.is_empty(), "{} must name process fixture", s.id);
            assert!(!s.embedded_blocker.is_empty(), "{} must name embedded blocker", s.id);
            assert!(s.embedded_blocker.contains("M6"), "{} blocker must cite the gate: {}", s.id, s.embedded_blocker);
        }
    }

    #[test]
    fn unknown_subset_rejected() {
        let r = phpx_runtime::PhpRequest::minimal("rid-9", "app", "index.php");
        let e = run_subset_stub("drupal", &r).unwrap_err().to_string();
        assert!(e.contains("unknown subset"));
    }
}
