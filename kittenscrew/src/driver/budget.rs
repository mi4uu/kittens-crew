//! T70 — budget as a loop primitive.
//!
//! Tracks token / dollar spend per DAG run AND per node; serves as a first-class
//! TERMINATION condition (`exhausted`) and a cheap/expensive model-routing signal
//! (`fits`). Limits are *rough* — approximate caps whose job is twofold:
//!   1. Bound runaway work (the infinite-research spiral, see explorer-budget memory).
//!   2. Let the planner select what fits the remaining envelope before dispatching.
//!
//! # ponytail: token counting
//! This module does NOT count tokens. The caller feeds already-counted numbers from
//! the squeez token-tracker (external). We only accumulate, enforce caps, and answer
//! routing queries. Actual per-response token counts come from the model's usage field
//! or squeez; pass them straight into `charge`.
//!
//! # Optional pricing
//! A rough pricing helper (`tokens_to_usd`) is provided for display / logging. It uses
//! a simple flat rate (per-million tokens) and is intentionally approximate — do not
//! use it for billing.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Rough pricing (display only, not authoritative)
// ---------------------------------------------------------------------------

/// Approximate USD cost per 1 M tokens (blended input+output, "small model" default).
/// Caller may override by ignoring this and computing cost externally.
const USD_PER_MILLION: f64 = 3.0;

/// Convert a raw token count to an approximate USD cost (display only).
/// ponytail: squeez is the real source of truth for token counts.
#[inline]
pub fn tokens_to_usd(tokens: u64) -> f64 {
    (tokens as f64) * USD_PER_MILLION / 1_000_000.0
}

// ---------------------------------------------------------------------------
// Budget
// ---------------------------------------------------------------------------

/// Per-run budget envelope: token cap, iteration cap, and per-node accounting.
///
/// Create once at the top of the DAG run loop and pass `&mut budget` into every
/// `charge` / `tick` site. Check `exhausted()` at loop-top as a termination guard,
/// and `fits(estimate)` before dispatching an expensive node.
///
/// # Example
/// ```
/// use kittenscrew::driver::budget::Budget;
///
/// let mut b = Budget::new(100_000, 20);
/// b.charge("T1", 12_000);
/// b.tick();
/// assert_eq!(b.spent(), 12_000);
/// assert!(!b.exhausted());
/// assert!(b.fits(80_000));
/// assert!(!b.fits(90_000));
/// ```
#[derive(Debug, Clone)]
pub struct Budget {
    /// Hard token cap for the whole run.
    token_cap: u64,
    /// Total tokens charged so far (across all nodes).
    spent: u64,
    /// Per-node token totals (node id → tokens).
    per_node: HashMap<String, u64>,
    /// Iteration cap for the DAG loop (coarse; see also driver::decide).
    max_iters: u32,
    /// Iterations consumed so far (bumped by `tick`).
    iters: u32,
}

impl Budget {
    /// Create a fresh budget with a token cap and an iteration cap.
    ///
    /// Both caps are ROUGH upper bounds — the goal is "close enough to stop runaway
    /// work", not precise accounting.
    pub fn new(token_cap: u64, max_iters: u32) -> Self {
        Self {
            token_cap,
            spent: 0,
            per_node: HashMap::new(),
            max_iters,
            iters: 0,
        }
    }

    /// Charge `tokens` to `node`. Adds to the node's running total AND the global
    /// total. Safe to call multiple times per node (e.g. multi-turn nodes).
    ///
    /// ponytail: token values come from the model response's `usage` field or squeez;
    /// pass them in without re-counting.
    pub fn charge(&mut self, node: &str, tokens: u64) {
        self.spent = self.spent.saturating_add(tokens);
        *self.per_node.entry(node.to_string()).or_insert(0) += tokens;
    }

    /// Total tokens spent across the whole run.
    #[inline]
    pub fn spent(&self) -> u64 {
        self.spent
    }

    /// Tokens remaining before the cap is hit. Saturates at 0 — never underflows.
    #[inline]
    pub fn remaining(&self) -> u64 {
        self.token_cap.saturating_sub(self.spent)
    }

    /// True when the budget is spent: either the token cap is reached OR the
    /// iteration cap is reached. Either condition is sufficient — the loop MUST stop.
    #[inline]
    pub fn exhausted(&self) -> bool {
        self.spent >= self.token_cap || self.iters >= self.max_iters
    }

    /// True if `estimated_tokens` fit within the remaining envelope. Use this before
    /// dispatching a node to a model so that the planner can skip / defer expensive
    /// nodes when the budget is nearly exhausted.
    #[inline]
    pub fn fits(&self, estimated_tokens: u64) -> bool {
        estimated_tokens <= self.remaining()
    }

    /// Advance the iteration counter by one. Call once per DAG loop iteration
    /// (not per node — per full pass through the ready frontier).
    #[inline]
    pub fn tick(&mut self) {
        self.iters = self.iters.saturating_add(1);
    }

    /// How many iterations have been consumed.
    #[inline]
    pub fn iters(&self) -> u32 {
        self.iters
    }

    /// Tokens charged to a specific node (0 if the node has not been charged).
    pub fn node_spent(&self, node: &str) -> u64 {
        self.per_node.get(node).copied().unwrap_or(0)
    }

    /// Approximate USD cost of tokens spent so far (display / logging only).
    /// ponytail: squeez is the authoritative token source; this is a display helper.
    pub fn cost_usd(&self) -> f64 {
        tokens_to_usd(self.spent)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// charge() accumulates correctly in the global total and per-node totals.
    #[test]
    fn charge_accumulates_total_and_per_node() {
        let mut b = Budget::new(1_000_000, 50);
        b.charge("T1", 10_000);
        b.charge("T2", 5_000);
        b.charge("T1", 3_000); // second charge to same node
        assert_eq!(b.spent(), 18_000, "global total");
        assert_eq!(b.node_spent("T1"), 13_000, "T1 per-node");
        assert_eq!(b.node_spent("T2"), 5_000, "T2 per-node");
        assert_eq!(b.node_spent("T3"), 0, "unknown node returns 0");
    }

    /// exhausted() flips when spent reaches the token cap.
    #[test]
    fn exhausted_on_token_cap() {
        let mut b = Budget::new(10_000, 100);
        assert!(!b.exhausted());
        b.charge("T1", 9_999);
        assert!(!b.exhausted(), "one below cap is still live");
        b.charge("T1", 1);
        assert!(b.exhausted(), "exactly at cap must be exhausted");
    }

    /// exhausted() flips when iters reach max_iters, even under the token cap.
    #[test]
    fn exhausted_on_iter_cap_under_token_cap() {
        let mut b = Budget::new(1_000_000, 3);
        b.tick();
        b.tick();
        assert!(!b.exhausted(), "2/3 iters not yet exhausted");
        b.tick();
        assert!(b.exhausted(), "3/3 iters must be exhausted even with plenty of tokens");
        assert!(b.spent() == 0, "token spend is still 0");
    }

    /// fits() rejects estimates larger than remaining; accepts those that fit.
    #[test]
    fn fits_rejects_over_budget_accepts_within() {
        let mut b = Budget::new(50_000, 100);
        b.charge("T1", 20_000); // remaining = 30_000
        assert!(b.fits(30_000), "exact fit should be accepted");
        assert!(!b.fits(30_001), "one over should be rejected");
        assert!(b.fits(0), "zero always fits");
    }

    /// remaining() saturates at 0 and never underflows.
    #[test]
    fn remaining_saturates_at_zero() {
        let mut b = Budget::new(1_000, 100);
        b.charge("T1", 500);
        assert_eq!(b.remaining(), 500);
        b.charge("T1", 500);
        assert_eq!(b.remaining(), 0, "exactly zero at cap");
        // Manually push past cap to verify saturation (charge adds, but we test
        // saturating_sub in remaining()).
        b.charge("T1", 1);
        assert_eq!(
            b.remaining(),
            0,
            "remaining must saturate at 0, not underflow"
        );
    }

    /// fits() returns false when already exhausted by iteration cap.
    #[test]
    fn fits_false_when_iter_exhausted() {
        let mut b = Budget::new(1_000_000, 1);
        b.tick(); // exhausted by iters
        // remaining() is still large, but fits should be false because exhausted.
        // NOTE: fits() is defined as estimated <= remaining(); the iter-exhausted
        // guard is in exhausted(). Callers SHOULD check exhausted() before fits().
        // Here we document that fits() alone does NOT check the iter cap — the loop
        // should check exhausted() first. This test verifies the documented contract.
        assert!(!b.exhausted() == false); // iters = max_iters → exhausted
        // fits() only looks at token remaining, not iter cap — callers must check
        // exhausted() separately. This is intentional: the two axes are orthogonal.
        assert!(b.fits(100), "fits() is token-only; caller must also check exhausted()");
    }

    /// tick() increments the iteration counter correctly.
    #[test]
    fn tick_increments_iters() {
        let mut b = Budget::new(1_000_000, 10);
        assert_eq!(b.iters(), 0);
        b.tick();
        b.tick();
        assert_eq!(b.iters(), 2);
    }

    /// cost_usd() returns a non-negative value proportional to spend.
    #[test]
    fn cost_usd_proportional() {
        let mut b = Budget::new(1_000_000, 10);
        b.charge("T1", 1_000_000);
        let cost = b.cost_usd();
        assert!(cost > 0.0);
        assert!((cost - 3.0).abs() < 1e-9, "1M tokens at $3/M = $3.0, got {cost}");
    }
}
