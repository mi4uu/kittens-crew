//! T79 — per-agent behavioural-health score updated after every action.
//!
//! `Karma` is a lightweight, purely in-memory scoreboard: it accumulates
//! `good` credit for successful actions and `bad` debit for failures and
//! stubborn repetition. Both counters decay after every call so stale signal
//! naturally fades. The module is self-contained (pure std, no deps, no IO)
//! and is wired into the harness by callers that already own the driver context.
//!
//! ## Scoring rule (canonical, deterministic)
//!
//! On `record(action, ok)`:
//!
//! 1. `!ok` → `bad += 0.3`
//! 2. Consecutive-repeat penalty (r = run-length including this call):
//!    - r == 2 → `bad += 0.3`
//!    - r >= 3 → `bad += 0.5`
//!    - r == 1  → no repeat penalty
//! 3. `ok` → `good += 0.2`
//! 4. Decay both: `good = (good − 0.1).max(0.0)`, `bad = (bad − 0.1).max(0.0)`
//!
//! KEY INVARIANT: a call that both errors AND is the 2nd repeat contributes
//! 0.3 (fail) + 0.3 (r == 2) = 0.6 to `bad` before the −0.1 decay → net +0.5.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Behavioural-health score for a single agent.
///
/// Both `good` and `bad` are non-negative floats that decay after every action
/// so that stale signal fades automatically. Use [`health`](Karma::health) for
/// a signed summary.
#[derive(Debug, Clone)]
pub struct Karma {
    good: f64,
    bad: f64,
    /// Hash of the last action string (for repeat detection).
    last: Option<u64>,
    /// How many consecutive identical calls have been seen so far, ≥ 1.
    run: usize,
}

impl Karma {
    /// Create a fresh scoreboard — both counters at 0.0, no history.
    pub fn new() -> Self {
        Karma { good: 0.0, bad: 0.0, last: None, run: 0 }
    }

    /// Record one action outcome and update the score.
    ///
    /// `action` is the name/label of the action; `ok` is `true` on success,
    /// `false` on failure / bad-params / error.
    pub fn record(&mut self, action: &str, ok: bool) {
        let h = hash_action(action);

        // Update run-length counter.
        if self.last == Some(h) {
            self.run += 1;
        } else {
            self.run = 1;
        }
        self.last = Some(h);

        // Step 1 — failure penalty.
        if !ok {
            self.bad += 0.3;
        }

        // Step 2 — repeat penalty.
        match self.run {
            2 => self.bad += 0.3,
            r if r >= 3 => self.bad += 0.5,
            _ => {}
        }

        // Step 3 — success credit.
        if ok {
            self.good += 0.2;
        }

        // Step 4 — decay both (floors at 0.0).
        self.good = (self.good - 0.1).max(0.0);
        self.bad  = (self.bad  - 0.1).max(0.0);
    }

    /// Accumulated good-action credit (≥ 0.0).
    pub fn good(&self) -> f64 {
        self.good
    }

    /// Accumulated bad-action debit (≥ 0.0).
    pub fn bad(&self) -> f64 {
        self.bad
    }

    /// Signed health: positive when the agent is behaving well, negative when
    /// it is stuck or error-prone.
    pub fn health(&self) -> f64 {
        self.good - self.bad
    }
}

impl Default for Karma {
    fn default() -> Self {
        Self::new()
    }
}

/// Deterministic hash of an action label using `DefaultHasher`.
fn hash_action(action: &str) -> u64 {
    let mut h = DefaultHasher::new();
    action.hash(&mut h);
    h.finish()
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Single successful non-repeat: good = 0.2 − 0.1 = 0.1, bad = 0.0.
    #[test]
    fn single_success() {
        let mut k = Karma::new();
        k.record("fetch", true);
        assert!((k.good() - 0.1).abs() < 1e-9, "good={}", k.good());
        assert!((k.bad()        ).abs() < 1e-9, "bad={}",  k.bad());
    }

    /// Single failed non-repeat: bad = 0.3 − 0.1 = 0.2, good = 0.0.
    #[test]
    fn single_failure() {
        let mut k = Karma::new();
        k.record("fetch", false);
        assert!((k.bad()  - 0.2).abs() < 1e-9, "bad={}",  k.bad());
        assert!((k.good()      ).abs() < 1e-9, "good={}", k.good());
    }

    /// KEY INVARIANT: 2nd call is both a failure AND a repeat (r == 2).
    ///
    /// Call 1 ("fetch", ok):
    ///   good += 0.2; decay → good = 0.1, bad = 0.0
    /// Call 2 ("fetch", !ok), r == 2:
    ///   bad  += 0.3 (fail) + 0.3 (r==2) = 0.6; decay → bad = 0.5
    ///   good: no credit; decay → good = max(0.1-0.1, 0) = 0.0
    #[test]
    fn key_invariant_repeat_and_fail() {
        let mut k = Karma::new();
        k.record("fetch", true);          // call 1 — ok, r=1
        // after call 1: good=0.1, bad=0.0
        k.record("fetch", false);         // call 2 — fail+repeat
        // after call 2: bad = 0.3+0.3-0.1 = 0.5, good = max(0.1-0.1, 0) = 0.0
        assert!((k.bad()  - 0.5).abs() < 1e-9, "bad={}  (want 0.5)", k.bad());
        assert!((k.good()      ).abs() < 1e-9, "good={} (want 0.0)", k.good());
    }

    /// 3rd identical call uses the r >= 3 penalty (0.5).
    #[test]
    fn triple_repeat_uses_heavy_penalty() {
        let mut k = Karma::new();
        k.record("loop", true);   // r=1, good +=0.2; decay: good=0.1, bad=0.0
        k.record("loop", true);   // r=2, bad +=0.3; good +=0.2; decay: good=0.2, bad=0.2
        k.record("loop", true);   // r=3, bad +=0.5; good +=0.2; decay: good=0.3, bad=0.6
        assert!((k.bad() - 0.6).abs() < 1e-9, "bad={} (want 0.6)", k.bad());
    }

    /// Floors hold: neither counter goes negative after decay.
    #[test]
    fn floors_hold() {
        let mut k = Karma::new();
        k.record("ping", true);
        // After one success: good=0.1, bad=0.0 — next success will push good to 0.1
        // then decay → 0.1 again. Apply many times; floor must never go negative.
        for _ in 0..20 {
            k.record("other", true);
        }
        assert!(k.good() >= 0.0, "good went negative: {}", k.good());
        assert!(k.bad()  >= 0.0, "bad went negative: {}",  k.bad());
    }

    /// health() = good − bad.
    #[test]
    fn health_is_good_minus_bad() {
        let mut k = Karma::new();
        k.record("x", true);
        k.record("x", false); // repeat + fail
        let expected = k.good() - k.bad();
        assert!((k.health() - expected).abs() < 1e-9);
    }

    /// A run resets when a different action arrives.
    #[test]
    fn run_resets_on_different_action() {
        let mut k = Karma::new();
        k.record("a", true);
        k.record("a", true); // r=2 → bad penalty applies
        let bad_after_2 = k.bad();
        // switch action — run resets to 1, no repeat penalty
        k.record("b", true);
        // bad should have decayed without new penalty
        assert!(k.bad() <= bad_after_2, "bad unexpectedly rose after action switch");
    }
}
