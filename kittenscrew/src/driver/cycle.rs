//! T80 — sequence-cycle detector.
//!
//! Catches "zombie agent walking in circles": the agent repeats the same
//! *sequence* of actions, e.g. `A→B→A→B` (period 2) or `A→B→C→A→B→C`
//! (period 3). This is distinct from the doom-loop guard (T67), which
//! catches a run of *identical* consecutive actions (period 1 is subsumed
//! here: three or more of the same action will also be caught).
//!
//! **Period-1 behaviour (design choice):** a run of identical consecutive
//! actions (e.g. `A A A A`) IS reported as period 1 by this detector.
//! Callers that want to defer period-1 detection to the doom-loop guard
//! should ignore `Some(1)`.
//!
//! # Algorithm
//!
//! For each candidate period `p` in `1..=max_period`, examine the last
//! `p * min_reps` actions. If they form exactly `min_reps` back-to-back
//! copies of the same length-`p` block, return `Some(p)`. We return the
//! *smallest* such period (most specific diagnosis first).
//!
//! The history ring never grows beyond `max_period * min_reps` entries, so
//! memory is O(`max_period * min_reps`).

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// ── public API ────────────────────────────────────────────────────────────────

/// Sliding-window cycle detector. Feed action labels with [`record`]; it
/// returns the smallest repeating period the moment one is confirmed.
///
/// [`record`]: CycleDetector::record
pub struct CycleDetector {
    history: VecDeque<u64>,
    max_period: usize,
    min_reps: usize,
}

impl CycleDetector {
    /// Create a detector.
    ///
    /// * `max_period` — longest sequence block to consider (e.g. `4`).
    /// * `min_reps`   — how many consecutive repetitions trigger a report
    ///                  (e.g. `2` means the block must appear at least twice).
    ///
    /// Panics if either argument is zero (a period or rep-count of zero is
    /// nonsensical and almost certainly a caller bug).
    pub fn new(max_period: usize, min_reps: usize) -> Self {
        assert!(max_period >= 1, "max_period must be >= 1");
        assert!(min_reps >= 2, "min_reps must be >= 2 (one repetition is not a cycle)");
        CycleDetector {
            history: VecDeque::with_capacity(max_period * min_reps),
            max_period,
            min_reps,
        }
    }

    /// Record one action (any `&str` label) and return `Some(period)` if the
    /// tail of the history now forms `min_reps` consecutive copies of a block
    /// of length `period`. Returns the smallest such period, or `None`.
    pub fn record(&mut self, action: &str) -> Option<usize> {
        let h = hash_action(action);
        self.history.push_back(h);
        // Trim to the window we actually need.
        let cap = self.max_period * self.min_reps;
        while self.history.len() > cap {
            self.history.pop_front();
        }
        detect_cycle(self.history.make_contiguous(), self.max_period, self.min_reps)
    }
}

// ── core free function ────────────────────────────────────────────────────────

/// Pure, allocation-free cycle check over a pre-hashed action slice.
///
/// Returns the smallest `p` in `1..=max_period` such that the last
/// `p * min_reps` elements of `history` are exactly `min_reps` copies of the
/// same length-`p` block. Returns `None` if no such period exists.
pub fn detect_cycle(history: &[u64], max_period: usize, min_reps: usize) -> Option<usize> {
    for p in 1..=max_period {
        let need = p * min_reps;
        if history.len() < need {
            continue;
        }
        let tail = &history[history.len() - need..];
        // `tail` is `min_reps` copies of `tail[0..p]` iff every position `i`
        // satisfies `tail[i] == tail[i % p]`.
        if tail.iter().enumerate().all(|(i, &v)| v == tail[i % p]) {
            return Some(p);
        }
    }
    None
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn hash_action(action: &str) -> u64 {
    let mut h = DefaultHasher::new();
    action.hash(&mut h);
    h.finish()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(actions: &[&str], max_period: usize, min_reps: usize) -> Option<usize> {
        let mut det = CycleDetector::new(max_period, min_reps);
        let mut last = None;
        for &a in actions {
            last = det.record(a);
        }
        last
    }

    // ── required spec tests ───────────────────────────────────────────────────

    /// Period-2 cycle: A B A B A B
    #[test]
    fn period_2_detected() {
        let result = feed(&["A", "B", "A", "B", "A", "B"], 4, 2);
        assert_eq!(result, Some(2), "A B A B A B must detect period 2");
    }

    /// Period-3 cycle: A B C A B C
    #[test]
    fn period_3_detected() {
        let result = feed(&["A", "B", "C", "A", "B", "C"], 4, 2);
        assert_eq!(result, Some(3), "A B C A B C must detect period 3");
    }

    /// Varied, non-repeating history → no detection.
    #[test]
    fn non_repeating_returns_none() {
        let result = feed(&["A", "B", "C", "D", "E", "F", "G"], 4, 2);
        assert!(result.is_none(), "varied history must not trigger detection");
    }

    /// Period-1: A A A — reported as Some(1) (period-1 is valid, see module doc).
    /// Callers that wish to defer this to the doom-loop guard should ignore Some(1).
    #[test]
    fn period_1_reported() {
        let result = feed(&["A", "A", "A"], 4, 2);
        assert_eq!(result, Some(1), "A A A must report period 1 (design choice: see module doc)");
    }

    /// Detection is deterministic: identical input always produces identical output.
    #[test]
    fn deterministic() {
        let actions = &["X", "Y", "X", "Y", "X", "Y"];
        let a = feed(actions, 4, 2);
        let b = feed(actions, 4, 2);
        assert_eq!(a, b, "same input must yield same result");
    }

    // ── additional robustness tests ───────────────────────────────────────────

    /// Not enough history yet — no false positive.
    #[test]
    fn too_short_returns_none() {
        let result = feed(&["A", "B"], 4, 2);
        assert!(result.is_none(), "only 2 elements, period-2 needs 4 to confirm");
    }

    /// Period detected mid-stream on the LAST action.
    #[test]
    fn detection_triggers_on_completing_action() {
        let mut det = CycleDetector::new(4, 2);
        assert_eq!(det.record("A"), None);
        assert_eq!(det.record("B"), None);
        assert_eq!(det.record("A"), None); // only 3 elements, need 4 for period-2
        assert_eq!(det.record("B"), Some(2)); // now 4 elements: A B A B → period 2
    }

    /// Smallest period wins when multiple fit (e.g. period-1 beats period-2).
    #[test]
    fn smallest_period_wins() {
        // A A A A A A — period 1 (and also period 2, period 3 …) but 1 is smallest.
        let result = feed(&["A", "A", "A", "A", "A", "A"], 4, 2);
        assert_eq!(result, Some(1));
    }

    /// Window eviction: only the last `max_period * min_reps` hashes are kept.
    #[test]
    fn history_is_bounded() {
        let max_period = 3;
        let min_reps = 2;
        let mut det = CycleDetector::new(max_period, min_reps);
        // Fill with noise well beyond the cap (cap = 6).
        for i in 0..100 {
            let label = format!("N{i}");
            det.record(&label);
        }
        // History must not exceed max_period * min_reps.
        assert!(
            det.history.len() <= max_period * min_reps,
            "history len {} exceeds cap {}",
            det.history.len(),
            max_period * min_reps
        );
    }

    /// A partial repeat (prefix matches but last element breaks it) → None.
    #[test]
    fn partial_repeat_is_not_a_cycle() {
        // A B C A B D  — almost A B C A B C but the last element differs.
        let result = feed(&["A", "B", "C", "A", "B", "D"], 4, 2);
        assert!(result.is_none());
    }
}
