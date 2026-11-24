//! M6 promotion gate: engine lifecycle state machine.
//!
//! Models the future live engine so orchestration (domain entry, bridge
//! calls, shutdown) has one deterministic lifecycle to drive. Pure
//! transition table, unit-tested. Hosting a live engine stays closed until
//! the promotion gates pass; the machine itself is the contract the live
//! engine will obey.

/// Engine lifecycle phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnginePhase {
    Cold,
    Initializing,
    Ready,
    Serving,
    Draining,
    Shutdown,
    Failed,
}

/// Lifecycle events driving the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineEvent {
    InitStart,
    InitOk,
    InitFailed,
    Enter,
    ExitOk,
    ExitFailed,
    DrainRequested,
    Shutdown,
}

impl EnginePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            EnginePhase::Cold => "cold",
            EnginePhase::Initializing => "initializing",
            EnginePhase::Ready => "ready",
            EnginePhase::Serving => "serving",
            EnginePhase::Draining => "draining",
            EnginePhase::Shutdown => "shutdown",
            EnginePhase::Failed => "failed",
        }
    }

    /// Pure transition table. `None` = illegal transition (never applied).
    /// Failure escalation is a supervisor decision, not a transition.
    pub fn transition(self, event: EngineEvent) -> Option<EnginePhase> {
        use EnginePhase::*;
        match (self, event) {
            (Cold, EngineEvent::InitStart) => Some(Initializing),
            (Initializing, EngineEvent::InitOk) => Some(Ready),
            (Initializing, EngineEvent::InitFailed) => Some(Failed),
            (Ready, EngineEvent::Enter) => Some(Serving),
            (Serving, EngineEvent::ExitOk) => Some(Ready),
            (Serving, EngineEvent::ExitFailed) => Some(Failed),
            (Ready, EngineEvent::DrainRequested) => Some(Draining),
            (Serving, EngineEvent::DrainRequested) => Some(Draining),
            (Draining, EngineEvent::ExitOk) => Some(Shutdown),
            (Draining, EngineEvent::ExitFailed) => Some(Failed),
            (Ready, EngineEvent::Shutdown) => Some(Shutdown),
            (Draining, EngineEvent::Shutdown) => Some(Shutdown),
            (Failed, EngineEvent::Shutdown) => Some(Shutdown),
            _ => None,
        }
    }

    /// Only Ready serves; only Serving exits. Everything else is a bug.
    pub fn can_enter(self) -> bool {
        self == EnginePhase::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::{EngineEvent as E, EnginePhase as P};

    #[test]
    fn happy_path_cold_to_serving_and_back() {
        assert_eq!(P::Cold.transition(E::InitStart), Some(P::Initializing));
        assert_eq!(P::Initializing.transition(E::InitOk), Some(P::Ready));
        assert!(P::Ready.can_enter());
        assert_eq!(P::Ready.transition(E::Enter), Some(P::Serving));
        assert_eq!(P::Serving.transition(E::ExitOk), Some(P::Ready));
    }

    #[test]
    fn failures_are_terminal_except_shutdown() {
        assert_eq!(P::Initializing.transition(E::InitFailed), Some(P::Failed));
        assert_eq!(P::Serving.transition(E::ExitFailed), Some(P::Failed));
        assert_eq!(P::Cold.transition(E::Enter), None);
        assert_eq!(P::Serving.transition(E::Enter), None);
        assert_eq!(P::Failed.transition(E::Enter), None);
        assert_eq!(P::Failed.transition(E::Shutdown), Some(P::Shutdown));
        assert!(!P::Cold.can_enter());
        assert!(!P::Serving.can_enter());
    }

    #[test]
    fn drain_path() {
        assert_eq!(P::Ready.transition(E::DrainRequested), Some(P::Draining));
        assert_eq!(P::Serving.transition(E::DrainRequested), Some(P::Draining));
        assert_eq!(P::Draining.transition(E::ExitOk), Some(P::Shutdown));
        assert_eq!(P::Ready.transition(E::Shutdown), Some(P::Shutdown));
    }
}
