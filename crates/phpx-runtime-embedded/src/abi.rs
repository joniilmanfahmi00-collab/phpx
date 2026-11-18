//! M6 Point 2: pinned PHP headers, libraries, and ABI expectations.
//!
//! No native linking in v0.1. This module pins what a future Linux
//! php-embed build must satisfy, so docs, CLI, and tests share one source.
//! Process Runtime stays active; embedded stays experimental.

use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};

/// PHP versions PHPX will claim for embedded.
pub const SUPPORTED_PHP_VERSIONS: [&str; 3] = ["8.3", "8.4", "8.5"];
/// Minimum PHP_VERSION_ID accepted for embed (8.3.0).
pub const MIN_PHP_VERSION_ID: u32 = 80300;
/// Expected SAPI name for embedded builds.
pub const EXPECTED_SAPI: &str = "embed";
/// Required headers for a php-embed build.
pub const EXPECTED_HEADERS: [&str; 3] = ["sapi/embed/php_embed.h", "main/php.h", "Zend/zend_API.h"];
/// Linux link targets.
pub const LINUX_LIBS: [&str; 2] = ["libphp.so", "libphp8.so"];
/// Pinned pkg-config name on Debian and Ubuntu php-dev installs.
pub const LINUX_PKG: &str = "php-embed";

/// Serializable ABI pin for CLI JSON and docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiPin {
    pub supported_php_versions: Vec<String>,
    pub sapi: String,
    pub nts_only: bool,
    pub debug_disabled: bool,
    pub min_version_id: u32,
    pub headers: Vec<String>,
    pub linux_libs: Vec<String>,
    pub linux_pkg: String,
    pub note: String,
}

impl AbiPin {
    pub fn pinned() -> Self {
        Self {
            supported_php_versions: SUPPORTED_PHP_VERSIONS.iter().map(|s| s.to_string()).collect(),
            sapi: EXPECTED_SAPI.to_string(),
            nts_only: true,
            debug_disabled: true,
            min_version_id: MIN_PHP_VERSION_ID,
            headers: EXPECTED_HEADERS.iter().map(|s| s.to_string()).collect(),
            linux_libs: LINUX_LIBS.iter().map(|s| s.to_string()).collect(),
            linux_pkg: LINUX_PKG.to_string(),
            note: "M6.2 pin: Linux NTS php-embed only; ZTS and Debug builds rejected; native link deferred".to_string(),
        }
    }

    /// Prefix match: 8.4.12 matches 8.4.
    pub fn supports_version(version: &str) -> bool {
        let v = version.trim();
        SUPPORTED_PHP_VERSIONS
            .iter()
            .any(|base| v == *base || v.starts_with(&format!("{base}.")))
    }

    /// Validate a candidate php-embed build with actionable errors.
    pub fn check(version: &str, is_zts: bool, is_debug: bool) -> Result<()> {
        if !Self::supports_version(version) {
            return Err(PhpxError::Runtime(format!(
                "embedded ABI unsupported PHP version {v}: want one of 8.3/8.4/8.5 NTS non-debug; process runtime stays active",
                v = version
            )));
        }
        if is_zts {
            return Err(PhpxError::Runtime(format!(
                "embedded ABI rejects ZTS build for PHP {v}: want NTS only; use process runtime or rebuild PHP NTS",
                v = version
            )));
        }
        if is_debug {
            return Err(PhpxError::Runtime(format!(
                "embedded ABI rejects Debug build for PHP {v}: want PHP_DEBUG=0 release build",
                v = version
            )));
        }
        Ok(())
    }

    pub fn describe() -> String {
        format!(
            "php-embed ABI pin: PHP {} NTS non-debug, SAPI={}, headers={} + linux libs={} (pkg {})",
            SUPPORTED_PHP_VERSIONS.join("/"),
            EXPECTED_SAPI,
            EXPECTED_HEADERS.join(", "),
            LINUX_LIBS.join(", "),
            LINUX_PKG,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_lists_supported_versions() {
        let p = AbiPin::pinned();
        assert_eq!(p.supported_php_versions, vec!["8.3", "8.4", "8.5"]);
        assert_eq!(p.sapi, "embed");
        assert!(p.nts_only);
        assert_eq!(p.min_version_id, 80300);
    }

    #[test]
    fn supports_prefix_versions() {
        assert!(AbiPin::supports_version("8.3"));
        assert!(AbiPin::supports_version("8.3.30"));
        assert!(AbiPin::supports_version("8.4.1"));
        assert!(AbiPin::supports_version("8.5.0"));
        assert!(!AbiPin::supports_version("8.2.1"));
        assert!(!AbiPin::supports_version("7.4.0"));
    }

    #[test]
    fn check_rejects_zts_and_debug() {
        assert!(AbiPin::check("8.4.1", false, false).is_ok());
        assert!(AbiPin::check("8.3.30", true, false).is_err());
        assert!(AbiPin::check("8.4.0", false, true).is_err());
        assert!(AbiPin::check("8.2.0", false, false).is_err());
    }

    #[test]
    fn windows_dev_php_is_zts_so_rejected() {
        let err = AbiPin::check("8.3.30", true, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("ZTS"));
    }
}
