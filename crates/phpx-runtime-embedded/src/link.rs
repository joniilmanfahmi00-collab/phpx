//! M6 Point 1: prototype PHP Embed SAPI build and linking probe.
//!
//! No native linking in v0.1. probe() inspects the dev machine for
//! php-config plus php binaries and reports what a Linux embed build
//! would need. It never fails the build. Native link stays deferred.

use serde::{Deserialize, Serialize};

/// Binaries the probe looks for, in order.
pub const PROBE_TARGETS: [&str; 2] = ["php-config", "php"];

/// Result of a link probe. found stays false until a Linux NTS
/// php-embed toolchain is wired into the build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkProbe {
    pub found: bool,
    pub php_config_version: String,
    pub php_version_line: String,
    pub tried: Vec<String>,
    pub how_to_enable: String,
}

impl LinkProbe {
    pub fn probe() -> Self {
        let mut tried = Vec::new();
        let php_config_version = Self::run_version("php-config", "--version", &mut tried);
        let php_version_line = Self::run_version("php", "--version", &mut tried);
        Self {
            found: false,
            php_config_version,
            php_version_line,
            tried,
            how_to_enable: "Linux only: install php-dev with embed SAPI plus NTS non-debug build, pin headers per AbiPin, then wire php-embed libs in build.rs. Windows ZTS dev PHP is rejected by design.".to_string(),
        }
    }

    fn run_version(bin: &str, arg: &str, tried: &mut Vec<String>) -> String {
        tried.push(format!("{bin} {arg}"));
        match std::process::Command::new(bin).arg(arg).output() {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.lines().next().unwrap_or("empty output").to_string()
            }
            Err(e) => format!("missing: {e}"),
        }
    }

    pub fn build_plan() -> String {
        "embed link plan: 1) Linux x86_64 with php-dev NTS non-debug 8.3-8.5, 2) php-config --includes plus --libs for php-embed, 3) build.rs emits cargo:rustc-link-lib for the embed lib, 4) narrow bridge calls startup plus execute_once plus shutdown, 5) contract tests gate every claim.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_reports_not_found_with_tried_list() {
        let p = LinkProbe::probe();
        assert!(!p.found);
        assert_eq!(p.tried, vec!["php-config --version", "php --version"]);
        assert!(p.how_to_enable.contains("Linux only"));
    }

    #[test]
    fn plan_mentions_linux_and_bridge() {
        let plan = LinkProbe::build_plan();
        assert!(plan.contains("Linux"));
        assert!(plan.contains("execute_once"));
    }
}
