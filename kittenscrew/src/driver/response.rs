//! T82 — intervention response strategy selector.
//!
//! Given an [`Level`] (how serious an agent misbehaviour is) and a [`Policy`]
//! (per-level strategy preferences), [`choose`] returns the [`Strategy`] that
//! the driver should apply — deterministically, with no IO and no model calls.
//!
//! The *actual* work — calling the agent, confronting a peer, halting the
//! subgraph — lives behind the Driver seam. This module is the pure decision
//! core; it only *selects*.
//!
//! # Design
//! - [`Level`] is defined locally. Translation to/from `intervene::Intervention`
//!   is wired in T84; importing sibling modules here would couple things too early.
//! - [`Strategy::SelfDecide`] carries `tighten: f64` (> 0). The fault is NOT
//!   erased when the agent chooses to continue; `tighten` shrinks the next
//!   trigger threshold so a continued bad run re-warns very fast.
//! - `f64` in `Strategy` means the type cannot derive `Eq` — that is intentional.

/// How serious the observed misbehaviour is.
///
/// Mirrors the actionable levels in `intervene::Intervention`; the canonical
/// translation lives in T84, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Minor drift — worth flagging but the agent may self-correct.
    Soft,
    /// Clear violation — needs active intervention before continuing.
    Hard,
    /// Unrecoverable or trust-breaking — must stop.
    DealBreaker,
}

/// What to do in response to an intervention at a given [`Level`].
#[derive(Debug, Clone, PartialEq)]
pub enum Strategy {
    /// Stop the subgraph and trigger a re-plan. Default for [`Level::DealBreaker`].
    HaltReplan,

    /// Ask the agent to explain what happened AND state — honestly — whether it
    /// genuinely believes it will solve the problem in the next few moves (not
    /// just repeat the same moves). A "yes-I'll-fix-it" without a credible plan
    /// still counts as a bad signal. Default for [`Level::Hard`].
    ExplainSelfAssess,

    /// Let the agent decide whether to continue, but it must provide an
    /// explanation. The fault is **not** erased: `tighten` lowers the next
    /// trigger threshold so a continued bad run re-warns very fast.
    ///
    /// Default for [`Level::Soft`].
    ///
    /// # Invariant
    /// `tighten` must be > 0.  A zero value would leave the threshold unchanged
    /// — equivalent to not tracking the fault at all — defeating the purpose.
    SelfDecide { tighten: f64 },

    /// Confront the offending agent with a peer: the offender explains its
    /// reasoning and the peer decides whether to let it continue. The peer's
    /// decision is binding — the driver applies it without further judgement.
    PeerConfront,
}

/// Per-level strategy configuration.
///
/// Construct with [`Policy::default()`] for the documented defaults, then
/// override individual fields as needed.
///
/// ```
/// use kittenscrew::driver::response::{Level, Policy, Strategy, choose};
///
/// let mut p = Policy::default();
/// p.on_soft = Strategy::ExplainSelfAssess;
/// assert!(matches!(choose(Level::Soft, &p), Strategy::ExplainSelfAssess));
/// ```
#[derive(Debug, Clone)]
pub struct Policy {
    /// Strategy applied when an intervention is [`Level::Soft`].
    pub on_soft: Strategy,
    /// Strategy applied when an intervention is [`Level::Hard`].
    pub on_hard: Strategy,
    /// Strategy applied when an intervention is [`Level::DealBreaker`].
    pub on_dealbreaker: Strategy,
}

impl Default for Policy {
    /// Soft → `SelfDecide { tighten: 0.25 }` — let agent continue with lower
    /// re-trigger threshold; Hard → `PeerConfront`; DealBreaker → `HaltReplan`.
    fn default() -> Self {
        Policy {
            on_soft: Strategy::SelfDecide { tighten: 0.25 },
            on_hard: Strategy::PeerConfront,
            on_dealbreaker: Strategy::HaltReplan,
        }
    }
}

/// Deterministically select a [`Strategy`] given an intervention [`Level`] and
/// a [`Policy`].
///
/// This is a pure, total function with no side-effects. The caller is
/// responsible for executing the returned strategy.
///
/// ```
/// use kittenscrew::driver::response::{Level, Policy, Strategy, choose};
///
/// let s = choose(Level::DealBreaker, &Policy::default());
/// assert!(matches!(s, Strategy::HaltReplan));
/// ```
pub fn choose(level: Level, policy: &Policy) -> Strategy {
    match level {
        Level::Soft => policy.on_soft.clone(),
        Level::Hard => policy.on_hard.clone(),
        Level::DealBreaker => policy.on_dealbreaker.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── default policy ────────────────────────────────────────────────────────

    #[test]
    fn default_soft_is_self_decide() {
        let s = choose(Level::Soft, &Policy::default());
        assert!(
            matches!(s, Strategy::SelfDecide { .. }),
            "Soft default must be SelfDecide, got {s:?}"
        );
    }

    #[test]
    fn default_hard_is_peer_confront() {
        let s = choose(Level::Hard, &Policy::default());
        assert!(
            matches!(s, Strategy::PeerConfront),
            "Hard default must be PeerConfront, got {s:?}"
        );
    }

    #[test]
    fn default_dealbreaker_is_halt_replan() {
        let s = choose(Level::DealBreaker, &Policy::default());
        assert!(
            matches!(s, Strategy::HaltReplan),
            "DealBreaker default must be HaltReplan, got {s:?}"
        );
    }

    // ── SelfDecide tighten invariant ──────────────────────────────────────────

    /// The default `tighten` on SelfDecide must be positive: a zero value would
    /// not lower the re-trigger threshold and would defeat the fast-re-warn semantics.
    #[test]
    fn default_self_decide_tighten_positive() {
        match Policy::default().on_soft {
            Strategy::SelfDecide { tighten } => {
                assert!(tighten > 0.0, "tighten must be > 0, got {tighten}");
            }
            other => panic!("expected SelfDecide for on_soft, got {other:?}"),
        }
    }

    // ── policy override ───────────────────────────────────────────────────────

    #[test]
    fn override_soft_returns_overridden_strategy() {
        let policy = Policy {
            on_soft: Strategy::ExplainSelfAssess,
            ..Policy::default()
        };
        let s = choose(Level::Soft, &policy);
        assert!(
            matches!(s, Strategy::ExplainSelfAssess),
            "overridden Soft must return ExplainSelfAssess, got {s:?}"
        );
    }

    #[test]
    fn override_hard_returns_overridden_strategy() {
        let policy = Policy {
            on_hard: Strategy::ExplainSelfAssess,
            ..Policy::default()
        };
        let s = choose(Level::Hard, &policy);
        assert!(
            matches!(s, Strategy::ExplainSelfAssess),
            "overridden Hard must return ExplainSelfAssess, got {s:?}"
        );
    }

    #[test]
    fn override_dealbreaker_returns_overridden_strategy() {
        let policy = Policy {
            on_dealbreaker: Strategy::PeerConfront,
            ..Policy::default()
        };
        let s = choose(Level::DealBreaker, &policy);
        assert!(
            matches!(s, Strategy::PeerConfront),
            "overridden DealBreaker must return PeerConfront, got {s:?}"
        );
    }

    // ── determinism ──────────────────────────────────────────────────────────

    /// Same inputs → same output on repeated calls (pure function, no mutable state).
    #[test]
    fn choose_is_deterministic() {
        let policy = Policy::default();
        for level in [Level::Soft, Level::Hard, Level::DealBreaker] {
            let a = choose(level, &policy);
            let b = choose(level, &policy);
            // PartialEq on Strategy (no Eq due to f64) — compare via Debug repr
            // to keep the assertion simple and format the diff on failure.
            assert_eq!(
                format!("{a:?}"),
                format!("{b:?}"),
                "choose({level:?}) returned different results on two calls"
            );
        }
    }

    // ── custom SelfDecide tighten ─────────────────────────────────────────────

    #[test]
    fn custom_self_decide_tighten_round_trips() {
        let policy = Policy {
            on_soft: Strategy::SelfDecide { tighten: 0.5 },
            ..Policy::default()
        };
        match choose(Level::Soft, &policy) {
            Strategy::SelfDecide { tighten } => {
                assert!((tighten - 0.5).abs() < f64::EPSILON, "tighten round-trip failed");
            }
            other => panic!("expected SelfDecide, got {other:?}"),
        }
    }
}
