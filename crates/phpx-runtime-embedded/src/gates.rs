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
        pass: false,
        evidence: "HOLD: IsolationScope lifecycle modeled and unit-proven (clean seal vs leak fail-closed); live Zend executor reset per request unproven without a hosted engine".to_string(),
    });
    gates.push(GateStatus {
        id: "compat_parity",
        name: "Parity on the compatibility suite for every claimed capability".to_string(),
        pass: false,
        evidence: "HOLD: subsets mirror M4 fixtures with per-subset blockers, but no subset executes through an engine yet (engine entry closed)".to_string(),
    });
    gates.push(GateStatus {
        id: "stability",
        name: "Stability evidence (no promotion on throughput alone)".to_string(),
        pass: false,
        evidence: "HOLD: no archived linked-engine soak (consecutive clean entries + RSS trend per the soak-long convention)".to_string(),
    });
    gates
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
        // Structural gates hold regardless of machine: isolation, parity and
        // stability need a live engine plus archived evidence.
        for id in ["isolation", "compat_parity", "stability"] {
            let g = assess().into_iter().find(|g| g.id == id).unwrap();
            assert!(!g.pass, "{id} must hold without a live engine");
            assert!(g.evidence.contains("HOLD"), "{id} must say HOLD");
        }
    }
}
