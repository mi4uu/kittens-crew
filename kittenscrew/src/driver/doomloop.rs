//! T67 — doom-loop guard: deterministic, offline, model-state-independent.
//!
//! Two complementary mechanisms prevent a runaway agent from spinning forever:
//!
//! 1. **Repeat detector** — a sliding window of hashed action signatures.
//!    When the same hash appears `repeat_threshold` or more times within the
//!    window, the loop is classified as a doom-loop and blocked. Identical
//!    consecutive repeats (e.g. the agent issuing the same tool call in a row)
//!    are the canonical signal; the window also catches near-cycles that skip a
//!    step or two between repetitions.
//!
//! 2. **Hard iteration cap** — `record` increments an unconditional counter and
//!    returns `Block` the moment `max_iters` is reached, regardless of whether
//!    the actions look varied. This ensures termination even if the agent varies
//!    its actions just enough to dodge the repeat detector.
//!
//! Both checks are pure, deterministic, and offline — no model, no IO. The hash
//! is computed with `std::hash::DefaultHasher` (stable within a single run;
//! sufficient because the guard is per-session, not persisted).
//!
//! Ported conceptually from `opencrabs::is_stuck_in_intent_loop` (semantic
//! doom-loop + iteration cap). The implementation here is simpler: we hash
//! the full action string rather than a semantic embedding, which is cheaper
//! and still sufficient for identical or near-identical tool dispatches.

use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

// ── Public types ─────────────────────────────────────────────────────────────

/// Decision returned by [`DoomGuard::record`].
#[derive(Debug, PartialEq, Eq)]
pub enum LoopVerdict {
    /// Action accepted — no doom-loop detected yet.
    Allow,
    /// Action blocked — doom-loop detected. `reason` is always non-empty.
    Block { reason: String },
}

/// Stateful doom-loop detector. Create once per driver session and call
/// [`record`][DoomGuard::record] on every action before dispatching it.
///
/// # Example
/// ```
/// use kittenscrew::driver::doomloop::{DoomGuard, LoopVerdict};
///
/// let mut guard = DoomGuard::new(3, 100);
/// assert_eq!(guard.record("tool:read file=foo.rs"), LoopVerdict::Allow);
/// assert_eq!(guard.record("tool:read file=foo.rs"), LoopVerdict::Allow);
/// // third identical hit → Block
/// assert!(matches!(guard.record("tool:read file=foo.rs"), LoopVerdict::Block { .. }));
/// ```
pub struct DoomGuard {
    /// Sliding window of hashed action signatures (most-recent at back).
    window: VecDeque<u64>,
    /// How many times the same hash must appear in `window` to trigger Block.
    repeat_threshold: usize,
    /// Hard ceiling on total recorded actions.
    max_iters: usize,
    /// Unconditional counter of all `record` calls so far.
    iters: usize,
}

impl DoomGuard {
    /// Create a guard with the given thresholds.
    ///
    /// - `repeat_threshold` — minimum occurrences of one hash in the window to
    ///   declare a doom-loop (must be ≥ 1; values < 2 are clamped to 2 so that
    ///   a single occurrence is never blocked).
    /// - `max_iters` — hard cap on total iterations; once hit, every subsequent
    ///   call returns [`LoopVerdict::Block`].
    pub fn new(repeat_threshold: usize, max_iters: usize) -> Self {
        Self {
            window: VecDeque::new(),
            // Clamp so that threshold=0 or 1 doesn't block the very first action.
            repeat_threshold: repeat_threshold.max(2),
            max_iters,
            iters: 0,
        }
    }

    /// Record an action and decide whether to allow or block it.
    ///
    /// The action string is hashed deterministically (same input → same hash
    /// within a run). The sliding window is bounded to `repeat_threshold * 2`
    /// entries so memory stays constant regardless of session length.
    ///
    /// Precedence (safest-first):
    /// 1. Hard iteration cap (`iters >= max_iters`) → Block.
    /// 2. Same hash seen `>= repeat_threshold` times in window → Block.
    /// 3. Otherwise → Allow, push hash into window.
    pub fn record(&mut self, action: &str) -> LoopVerdict {
        // 1. Hard cap — checked BEFORE incrementing so the cap is exact.
        if self.iters >= self.max_iters {
            return LoopVerdict::Block {
                reason: format!(
                    "hard iteration cap reached ({}/{})",
                    self.iters, self.max_iters
                ),
            };
        }
        self.iters += 1;

        let h = hash_action(action);

        // 2. Repeat check against the current window (before pushing new hash).
        let occurrences = self.window.iter().filter(|&&x| x == h).count();
        // +1 because we're about to add this occurrence.
        if occurrences + 1 >= self.repeat_threshold {
            // Still push so the window reflects reality for future calls.
            self.push_window(h);
            return LoopVerdict::Block {
                reason: format!(
                    "doom-loop: action repeated {} time(s) within window (threshold={}): {:?}",
                    occurrences + 1,
                    self.repeat_threshold,
                    // Truncate long actions in the reason string.
                    truncate(action, 80),
                ),
            };
        }

        self.push_window(h);
        LoopVerdict::Allow
    }

    /// Sliding-window push. Drops the oldest entry when the window is full.
    fn push_window(&mut self, h: u64) {
        let cap = (self.repeat_threshold * 2).max(4);
        if self.window.len() >= cap {
            self.window.pop_front();
        }
        self.window.push_back(h);
    }

    /// Number of actions recorded so far (regardless of verdict).
    pub fn iters(&self) -> usize {
        self.iters
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn hash_action(action: &str) -> u64 {
    let mut h = DefaultHasher::new();
    action.hash(&mut h);
    h.finish()
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Same action repeated `repeat_threshold` times → Block, with non-empty reason.
    #[test]
    fn identical_repeats_block_at_threshold() {
        let mut g = DoomGuard::new(3, 100);
        assert_eq!(g.record("tool:read file=foo.rs"), LoopVerdict::Allow);
        assert_eq!(g.record("tool:read file=foo.rs"), LoopVerdict::Allow);
        let v = g.record("tool:read file=foo.rs");
        match v {
            LoopVerdict::Block { reason } => {
                assert!(!reason.is_empty(), "reason must be non-empty");
                assert!(reason.contains("doom-loop"), "reason should mention doom-loop");
            }
            LoopVerdict::Allow => panic!("expected Block on 3rd identical action"),
        }
    }

    /// Alternating / varied actions never trigger the repeat detector.
    #[test]
    fn varied_actions_stay_allow() {
        let mut g = DoomGuard::new(3, 200);
        let actions = ["read:a", "write:b", "read:c", "write:d", "list:e", "run:f"];
        for (i, &a) in actions.iter().enumerate() {
            assert_eq!(
                g.record(a),
                LoopVerdict::Allow,
                "action #{i} ({a:?}) should be Allow"
            );
        }
    }

    /// Hitting `max_iters` blocks even when every action is distinct.
    #[test]
    fn max_iters_blocks_regardless_of_variety() {
        let mut g = DoomGuard::new(5, 4); // cap at 4 iters
        assert_eq!(g.record("a"), LoopVerdict::Allow);
        assert_eq!(g.record("b"), LoopVerdict::Allow);
        assert_eq!(g.record("c"), LoopVerdict::Allow);
        assert_eq!(g.record("d"), LoopVerdict::Allow);
        // 5th call — iters already == max_iters → Block
        let v = g.record("e");
        match v {
            LoopVerdict::Block { reason } => {
                assert!(!reason.is_empty(), "reason must be non-empty");
                assert!(reason.contains("cap"), "reason should mention cap");
            }
            LoopVerdict::Allow => panic!("expected Block at max_iters"),
        }
    }

    /// Reason string is always non-empty on any Block.
    #[test]
    fn block_reason_always_non_empty() {
        // Via repeat detector.
        let mut g = DoomGuard::new(2, 100);
        g.record("x");
        if let LoopVerdict::Block { reason } = g.record("x") {
            assert!(!reason.is_empty());
        } else {
            panic!("expected Block");
        }

        // Via hard cap.
        let mut g2 = DoomGuard::new(10, 1);
        g2.record("y");
        if let LoopVerdict::Block { reason } = g2.record("z") {
            assert!(!reason.is_empty());
        } else {
            panic!("expected Block at cap");
        }
    }

    /// iters() counter tracks every record() call including blocked ones.
    #[test]
    fn iters_counter_tracks_all_calls() {
        let mut g = DoomGuard::new(3, 100);
        g.record("p");
        g.record("p");
        g.record("p"); // blocks
        g.record("p"); // also blocks
        assert_eq!(g.iters(), 4);
    }

    /// Two different action strings hash differently (sanity).
    #[test]
    fn distinct_actions_have_distinct_hashes() {
        assert_ne!(hash_action("tool:read file=a.rs"), hash_action("tool:read file=b.rs"));
    }
}
