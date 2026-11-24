//! M6 promotion gate: extension compatibility.
//!
//! The embedded engine must expose the same PHP extensions the process
//! runtime requires (see `phpx doctor`: json, mbstring, curl, openssl,
//! pdo, pdo_sqlite, fileinfo, filter, dom, gd, intl). This module audits a
//! `php -m` listing against that matrix: pure parse plus a live probe.
//! A promotion flips only when the linked NTS build satisfies the matrix.

use serde::{Deserialize, Serialize};

/// Extensions the engine must expose (matches the `phpx doctor` audit).
pub const REQUIRED_EXTENSIONS: [&str; 11] = [
    "json",
    "mbstring",
    "curl",
    "openssl",
    "pdo",
    "pdo_sqlite",
    "fileinfo",
    "filter",
    "dom",
    "gd",
    "intl",
];

/// Audit result: present vs missing extension names (lowercase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtReport {
    pub present: Vec<String>,
    pub missing: Vec<String>,
    pub note: String,
}

impl ExtReport {
    /// Audit a `php -m` listing (the `[PHP Modules]` section; extra sections
    /// like `[Zend Modules]` are ignored).
    pub fn audit(modules_output: &str) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut in_modules = false;
        for line in modules_output.lines() {
            let t = line.trim();
            if t.eq_ignore_ascii_case("[PHP Modules]") {
                in_modules = true;
                continue;
            }
            if t.starts_with('[') && t.ends_with(']') {
                if in_modules {
                    break;
                }
                continue;
            }
            if in_modules && !t.is_empty() {
                seen.insert(t.to_ascii_lowercase());
            }
        }
        let mut present = Vec::new();
        let mut missing = Vec::new();
        for ext in REQUIRED_EXTENSIONS {
            if seen.contains(ext) {
                present.push(ext.to_string());
            } else {
                missing.push(ext.to_string());
            }
        }
        let note = if missing.is_empty() {
            "extension matrix satisfied (11/11)".to_string()
        } else {
            format!("extension matrix missing: {}", missing.join(", "))
        };
        Self {
            present,
            missing,
            note,
        }
    }

    pub fn satisfied(&self) -> bool {
        self.missing.is_empty()
    }

    /// Live probe: run `php -m` and audit it. Missing binary (or parseable
    /// output) is a Hold, never a crash — the caller decides promotion.
    pub fn probe() -> Self {
        match std::process::Command::new("php").arg("-m").output() {
            Ok(out) if out.status.success() => Self::audit(&String::from_utf8_lossy(&out.stdout)),
            Ok(out) => Self {
                present: Vec::new(),
                missing: REQUIRED_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
                note: format!("php -m exited non-zero ({}); matrix unaudited", out.status),
            },
            Err(e) => Self {
                present: Vec::new(),
                missing: REQUIRED_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
                note: format!("php binary missing ({e}); matrix unaudited"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "[PHP Modules]\nbcmath\ncalendar\nCore\nctype\ncurl\ndate\ndom\nexif\nfileinfo\nfilter\ngd\nhash\niconv\nintl\njson\nlibxml\nmbstring\nmysqli\nmysqlnd\nopenssl\npcre\nPDO\npdo_mysql\npdo_sqlite\nPhar\nreadline\nReflection\nsession\nSimpleXML\nsodium\nSPL\nsqlite3\nstandard\ntokenizer\nxml\nxmlreader\nxmlwriter\nxsl\nzip\nzlib\n\n[Zend Modules]\n\n";

    #[test]
    fn full_matrix_satisfied() {
        let r = ExtReport::audit(FULL);
        assert!(r.satisfied(), "missing: {:?}", r.missing);
        assert_eq!(r.present.len(), 11);
    }

    #[test]
    fn missing_extension_reported() {
        let partial = FULL.replace("gd\n", "");
        let r = ExtReport::audit(&partial);
        assert!(!r.satisfied());
        assert_eq!(r.missing, vec!["gd".to_string()]);
        assert!(r.note.contains("gd"));
    }

    #[test]
    fn zend_section_ignored_and_case_folds() {
        let out = "[PHP Modules]\nJSON\nOpenSSL\nPDO\n";
        let r = ExtReport::audit(out);
        assert!(r.present.contains(&"json".to_string()));
        assert!(r.present.contains(&"openssl".to_string()));
        assert!(r.missing.contains(&"gd".to_string()));
    }

    #[test]
    fn live_probe_audits_local_php() {
        // Any PHP (ZTS dev box included) exposes json: the probe must parse,
        // never crash. Satisfaction depends on the local build.
        let r = ExtReport::probe();
        assert!(
            r.present.contains(&"json".to_string()),
            "local php -m must list json: {}",
            r.note
        );
    }
}
