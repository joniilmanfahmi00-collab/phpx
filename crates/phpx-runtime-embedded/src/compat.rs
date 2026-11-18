//! M6 Point 9: Laravel plus WordPress compatibility subsets for embedded.
//!
//! Subsets mirror the M4 process fixtures. Execution through embedded
//! stays deferred until the native link lands; the stub validates the
//! subset id plus envelope and returns an actionable unlinked error.

use phpx_core::{PhpxError, Result};

/// Subset descriptor.
#[derive(Debug, Clone)]
pub struct Subset {
    pub id: String,
    pub app: String,
    pub entry: String,
    pub note: String,
}

pub fn subsets() -> Vec<Subset> {
    vec![
        Subset {
            id: "vanilla".to_string(),
            app: "vanilla".to_string(),
            entry: "index.php".to_string(),
            note: "M4 vanilla fixture shape".to_string(),
        },
        Subset {
            id: "laravel-minimal".to_string(),
            app: "laravel".to_string(),
            entry: "public/index.php".to_string(),
            note: "M4 laravel-minimal sim shape: routing plus JSON plus middleware".to_string(),
        },
        Subset {
            id: "wordpress-minimal".to_string(),
            app: "wordpress".to_string(),
            entry: "index.php".to_string(),
            note: "M4 wordpress-minimal sim shape: query plus shortcode".to_string(),
        },
        Subset {
            id: "woocommerce-minimal".to_string(),
            app: "woocommerce".to_string(),
            entry: "index.php".to_string(),
            note: "M4 woocommerce-minimal sim shape: cart plus checkout".to_string(),
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
    fn unknown_subset_rejected() {
        let r = phpx_runtime::PhpRequest::minimal("rid-9", "app", "index.php");
        let e = run_subset_stub("drupal", &r).unwrap_err().to_string();
        assert!(e.contains("unknown subset"));
    }
}
