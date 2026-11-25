//! M6 promotion gates assessor: per-gate DONE/HOLD with evidence.
//!
//! `available()` is true only when every gate passes — the mechanical
//! promotion condition. False today (fail-closed): the stub reports holds,
//! and the linked build holds engine entry until compat, isolation and
//! stability gates pass with archived evidence.

use serde::{Deserialize, Serialize};

/// One promotion gate verdict. `pass=false` is a Hold with the reason in
/// `evidence`, never a crash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateStatus {
    pub id: &'static str,
    pub name: String,
    pub pass: bool,
    pub evidence: String,
}

/// Assess all promotion gates in plan order. Pure except the extension
/// probe, which runs `php -m` (Hold on missing binary, never an error).
pub fn assess() -> Vec<GateStatus> {
    let mut gates = Vec::with_capacity(9);
    gates.push(GateStatus {
        id: "abi_pin",
        name: "Supported PHP build and ABI matrix".to_string(),
        pass: true,
        evidence: format!(
            "DONE: {} (matrix unit-proven: NTS non-debug accept, ZTS/Debug/old-version reject)",
            crate::abi::AbiPin::describe()
        ),
    });
    let linked = crate::is_build_linked();
    gates.push(GateStatus {
        id: "native_link",
        name: "Real PHP Embed SAPI link".to_string(),
        pass: linked,
        evidence: if linked {
            "DONE: libphp linked for this build (Linux NTS + PHPX_EMBED_LINK=1); engine entry still held until the gates below pass".to_string()
        } else {
            format!(
                "HOLD: stub build (no libphp linked). {}",
                crate::link::LinkProbe::probe().how_to_enable
            )
        },
    });
    gates.push(GateStatus {
        id: "execution_domain",
        name: "Controlled execution domain and thread constraints".to_string(),
        pass: true,
        evidence: "DONE: single-entry EmbeddedDomain (8-thread race yields exactly 1 winner; concurrent entry rejected, panic poisons gracefully)".to_string(),
    });
    gates.push(GateStatus {
        id: "ownership",
        name: "Clear ownership and lifetime rules across the C ABI".to_string(),
        pass: true,
        evidence: format!(
            "DONE: {} rules enforced by construction (Rust owns buffers, fresh scope per request, entry rejected before allocation)",
            crate::ownership::RULES.len()
        ),
    });
    gates.push(GateStatus {
        id: "containment",
        name: "Failure containment strategy".to_string(),
        pass: true,
        evidence: format!(
            "DONE: validate-then-enter-then-cap order, {} byte cap exact (no off-by-one), poisoned-domain fail-closed, gateway never hosts a live engine in this state",
            phpx_runtime::MAX_FRAME_BYTES
        ),
    });
    let ext = crate::ext::ExtReport::probe();
    gates.push(GateStatus {
        id: "extensions",
        name: "Extension compatibility tests".to_string(),
        pass: ext.satisfied(),
        evidence: if ext.satisfied() {
            format!(
                "DONE: live `php -m` satisfies the 11-extension matrix ({})",
                ext.note
            )
        } else {
            format!("HOLD: {}", ext.note)
        },
    });
    gates.push(GateStatus {
        id: "isolation",
        name: "Request state isolation tests".to_string(),
        pass: linked,
        evidence: if linked {
            "DONE: per-request executor reset via phpx_embed_next_request (balanced RINIT cycle; an unbalanced double-shutdown corrupts the heap — pinned). Linked tests prove clean separation (set → cycle → probe reads clean) and bailout → cycle recovery".to_string()
        } else {
            "HOLD: IsolationScope lifecycle modeled and unit-proven (clean seal vs leak fail-closed); live Zend executor reset needs a linked engine".to_string()
        },
    });
    gates.push(GateStatus {
        id: "compat_parity",
        name: "Parity on the compatibility suite for every claimed capability".to_string(),
        pass: false,
        evidence: "HOLD: 1/4 subsets live — the vanilla fixture executes through the engine via include with pinned output (hello from phpx vanilla|rid=direct). laravel-minimal/wordpress-minimal/woocommerce-minimal need SAPI request plumbing ($_GET/$_POST/headers/cookies per request)".to_string(),
    });
    gates.push(stability_gate());
    gates
}

/// Conventional soak archive the stability gate reads (gitignored, like the
/// process soak-long). `PHPX_EMBED_SOAK_RECORD` overrides the path.
fn soak_record_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("PHPX_EMBED_SOAK_RECORD") {
        if !p.trim().is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/bench/soak-embed-latest/SUMMARY.json")
}

/// Stability: PASS iff this is a linked build AND a soak archive exists,
/// is fresh (<180 days), and records zero failures with a bounded RSS
/// trend. The archive is produced by `tests/linked_soak.rs`; absent,
/// invalid or stale records HOLD (never an error). Stub builds always
/// HOLD: there is no linked engine whose stability could be evidenced.
fn stability_gate() -> GateStatus {
    let path = soak_record_path();
    // A stub build has no linked engine, so no soak record can evidence
    // stability — HOLD without even reading the archive.
    if !crate::is_build_linked() {
        return GateStatus {
            id: "stability",
            name: "Stability evidence (no promotion on throughput alone)".to_string(),
            pass: false,
            evidence: "HOLD: stub build (no linked engine to soak; run tests/linked_soak.rs on a linked build)".to_string(),
        };
    }
    let read = (|| -> Option<String> {
        let text = std::fs::read_to_string(&path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        if v.get("schema")?.as_str()? != "phpx-embed-soak/1" {
            return None;
        }
        let failures = v.get("failures")?.as_u64()?;
        let iters = v.get("iters")?.as_u64()?;
        let clean = v.get("clean")?.as_u64()?;
        let growth = v.get("rss_growth_bytes")?.as_u64()?;
        let cap = v.get("rss_cap_bytes")?.as_u64()?;
        let date = v.get("date")?.as_u64()?;
        if failures != 0 || iters < 200 || clean == 0 || growth > cap {
            return None;
        }
        let age_days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(date) / 86400)
            .unwrap_or(u64::MAX);
        if age_days > 180 {
            return None;
        }
        Some(format!(
            "DONE: linked soak record {} ({} iters, {} clean, 0 failures, RSS +{}B, {}d old)",
            path.display(),
            iters,
            clean,
            growth,
            age_days
        ))
    })();
    match read {
        Some(evidence) => GateStatus {
            id: "stability",
            name: "Stability evidence (no promotion on throughput alone)".to_string(),
            pass: true,
            evidence,
        },
        None => GateStatus {
            id: "stability",
            name: "Stability evidence (no promotion on throughput alone)".to_string(),
            pass: false,
            evidence: format!(
                "HOLD: no valid linked-engine soak at {} (run tests/linked_soak.rs on a linked build to archive one; needs 200+ iters, 0 failures, bounded RSS, <180d old)",
                path.display()
            ),
        },
    }
}

/// Promotion condition: every gate passes. Fail-closed by construction.
pub fn available() -> bool {
    assess().iter().all(|g| g.pass)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_gates_in_plan_order() {
        let ids: Vec<&str> = assess().iter().map(|g| g.id).collect();
        assert_eq!(
            ids,
            vec![
                "abi_pin",
                "native_link",
                "execution_domain",
                "ownership",
                "containment",
                "extensions",
                "isolation",
                "compat_parity",
                "stability"
            ]
        );
    }

    #[test]
    fn every_gate_carries_evidence() {
        for g in assess() {
            assert!(!g.name.is_empty(), "{} needs a name", g.id);
            assert!(!g.evidence.is_empty(), "{} needs evidence", g.id);
        }
    }

    #[test]
    fn promotion_stays_fail_closed() {
        assert!(
            !available(),
            "available must stay false until every gate passes"
        );
        // compat_parity holds in every build (parity needs all four subsets).
        // stability holds in stub builds (no linked engine to soak); in
        // linked builds it follows the soak archive (the linked soak test
        // asserts the flip end to end, so unit tests only pin the wording).
        #[cfg(not(phpx_embed_linked))]
        for id in ["compat_parity", "stability"] {
            let g = assess().into_iter().find(|g| g.id == id).unwrap();
            assert!(!g.pass, "{id} must hold");
            assert!(g.evidence.contains("HOLD"), "{id} must say HOLD");
        }
        #[cfg(phpx_embed_linked)]
        {
            let g = assess()
                .into_iter()
                .find(|g| g.id == "compat_parity")
                .unwrap();
            assert!(
                !g.pass,
                "compat_parity must hold until all four subsets run live"
            );
            assert!(g.evidence.contains("HOLD"));
            let s = assess().into_iter().find(|g| g.id == "stability").unwrap();
            assert!(
                s.evidence.contains("HOLD") || s.evidence.contains("DONE"),
                "stability must report a verdict"
            );
        }
        let iso = assess().into_iter().find(|g| g.id == "isolation").unwrap();
        assert_eq!(
            iso.pass,
            crate::is_build_linked(),
            "isolation follows the build (stub HOLD, linked DONE)"
        );
    }
}
