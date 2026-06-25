//! T66 — steer + follow-up injection queues.
//!
//! Two DISTINCT operator-control injection points into the drive loop:
//!
//! - **Steer** (`push_steer` / `drain_steer`): mid-run. Text is applied BEFORE
//!   the next turn, redirecting the active run without restarting it.
//! - **Follow-up** (`push_followup` / `drain_followup`): post-stop. Text that
//!   re-arms the loop after it has halted, letting an operator pick up where
//!   the run left off.
//!
//! Both queues are pure in-memory `VecDeque<String>` — no IO, no deps beyond
//! `std`. The [`DrainMode`] enum controls how callers consume messages:
//!
//! - [`DrainMode::OneAtATime`]: pops exactly one item (FIFO); caller decides
//!   when to ask for the next.
//! - [`DrainMode::DrainAll`]: takes the entire queue in order; all items are
//!   returned joined, and the queue is left empty.
//!
//! # Example
//!
//! ```rust
//! use kittenscrew::driver::queue::{DrainMode, Queues};
//!
//! let mut q = Queues::new();
//! q.push_steer("focus on module A");
//! q.push_steer("also check invariant V3");
//!
//! // Mid-run: apply the first redirect only.
//! let next = q.drain_steer(DrainMode::OneAtATime);
//! assert_eq!(next, vec!["focus on module A"]);
//! assert!(q.has_steer(), "second message still pending");
//!
//! // Post-stop re-arm.
//! q.push_followup("re-run §T.4 with tighter scope");
//! let re = q.drain_followup(DrainMode::DrainAll);
//! assert_eq!(re, vec!["re-run §T.4 with tighter scope"]);
//! ```

use std::collections::VecDeque;

// ── drain mode ────────────────────────────────────────────────────────────────

/// Controls how many messages a single `drain_*` call consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainMode {
    /// Pop exactly one message (FIFO). Returns an empty `Vec` if the queue is
    /// empty; returns a single-element `Vec` otherwise.
    OneAtATime,
    /// Drain the entire queue and return all messages in insertion order.
    /// The queue is empty after this call.
    DrainAll,
}

// ── queues ────────────────────────────────────────────────────────────────────

/// Paired injection queues: one for mid-run steering, one for post-stop
/// re-arming. Both are independent `VecDeque<String>` — no shared state.
#[derive(Debug, Default)]
pub struct Queues {
    steer: VecDeque<String>,
    followup: VecDeque<String>,
}

impl Queues {
    /// Construct empty queues.
    pub fn new() -> Self {
        Self::default()
    }

    // ── steer ─────────────────────────────────────────────────────────────────

    /// Enqueue a steer message (applied BEFORE the next turn).
    pub fn push_steer(&mut self, msg: impl Into<String>) {
        self.steer.push_back(msg.into());
    }

    /// Consume steer messages according to `mode`. Returns `Vec<String>` in
    /// insertion order. Draining an empty queue always returns `vec![]`.
    pub fn drain_steer(&mut self, mode: DrainMode) -> Vec<String> {
        drain(&mut self.steer, mode)
    }

    /// `true` if at least one steer message is waiting.
    pub fn has_steer(&self) -> bool {
        !self.steer.is_empty()
    }

    // ── follow-up ─────────────────────────────────────────────────────────────

    /// Enqueue a follow-up message (re-arms the loop after it has halted).
    pub fn push_followup(&mut self, msg: impl Into<String>) {
        self.followup.push_back(msg.into());
    }

    /// Consume follow-up messages according to `mode`. Returns `Vec<String>` in
    /// insertion order. Draining an empty queue always returns `vec![]`.
    pub fn drain_followup(&mut self, mode: DrainMode) -> Vec<String> {
        drain(&mut self.followup, mode)
    }

    /// `true` if at least one follow-up message is waiting.
    pub fn has_followup(&self) -> bool {
        !self.followup.is_empty()
    }
}

// ── internal helper ───────────────────────────────────────────────────────────

/// Generic drain over any `VecDeque<String>`, respecting `DrainMode`.
fn drain(q: &mut VecDeque<String>, mode: DrainMode) -> Vec<String> {
    match mode {
        DrainMode::OneAtATime => q.pop_front().into_iter().collect(),
        DrainMode::DrainAll => q.drain(..).collect(),
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── independence ──────────────────────────────────────────────────────────

    /// Steer and follow-up are completely independent: touching one must not
    /// affect the other.
    #[test]
    fn queues_are_independent() {
        let mut q = Queues::new();
        q.push_steer("s1");
        q.push_followup("f1");
        q.push_followup("f2");

        // Drain steer; follow-up intact.
        let s = q.drain_steer(DrainMode::DrainAll);
        assert_eq!(s, vec!["s1"]);
        assert!(!q.has_steer());
        assert!(q.has_followup(), "followup must not be touched");

        // Drain followup; steer still empty.
        let f = q.drain_followup(DrainMode::DrainAll);
        assert_eq!(f, vec!["f1", "f2"]);
        assert!(!q.has_followup());
        assert!(!q.has_steer());
    }

    // ── OneAtATime — steer ────────────────────────────────────────────────────

    /// OneAtATime pops exactly one item in FIFO order.
    #[test]
    fn steer_one_at_a_time_is_fifo() {
        let mut q = Queues::new();
        q.push_steer("first");
        q.push_steer("second");
        q.push_steer("third");

        assert_eq!(q.drain_steer(DrainMode::OneAtATime), vec!["first"]);
        assert_eq!(q.drain_steer(DrainMode::OneAtATime), vec!["second"]);
        assert_eq!(q.drain_steer(DrainMode::OneAtATime), vec!["third"]);
        // Now empty.
        assert!(q.drain_steer(DrainMode::OneAtATime).is_empty());
    }

    // ── DrainAll — steer ──────────────────────────────────────────────────────

    /// DrainAll empties the steer queue and returns all messages in order.
    #[test]
    fn steer_drain_all_returns_all_in_order() {
        let mut q = Queues::new();
        q.push_steer("a");
        q.push_steer("b");
        q.push_steer("c");

        let out = q.drain_steer(DrainMode::DrainAll);
        assert_eq!(out, vec!["a", "b", "c"]);
        assert!(!q.has_steer(), "queue must be empty after DrainAll");
    }

    // ── OneAtATime — followup ─────────────────────────────────────────────────

    /// OneAtATime pops exactly one follow-up in FIFO order.
    #[test]
    fn followup_one_at_a_time_is_fifo() {
        let mut q = Queues::new();
        q.push_followup("re-arm-1");
        q.push_followup("re-arm-2");

        assert_eq!(q.drain_followup(DrainMode::OneAtATime), vec!["re-arm-1"]);
        assert_eq!(q.drain_followup(DrainMode::OneAtATime), vec!["re-arm-2"]);
        assert!(q.drain_followup(DrainMode::OneAtATime).is_empty());
    }

    // ── DrainAll — followup ───────────────────────────────────────────────────

    /// DrainAll empties the follow-up queue and returns all messages in order.
    #[test]
    fn followup_drain_all_returns_all_in_order() {
        let mut q = Queues::new();
        q.push_followup("x");
        q.push_followup("y");

        let out = q.drain_followup(DrainMode::DrainAll);
        assert_eq!(out, vec!["x", "y"]);
        assert!(!q.has_followup());
    }

    // ── empty drain ───────────────────────────────────────────────────────────

    /// Draining an empty queue with either mode returns an empty Vec.
    #[test]
    fn drain_empty_returns_empty() {
        let mut q = Queues::new();

        assert!(q.drain_steer(DrainMode::OneAtATime).is_empty());
        assert!(q.drain_steer(DrainMode::DrainAll).is_empty());
        assert!(q.drain_followup(DrainMode::OneAtATime).is_empty());
        assert!(q.drain_followup(DrainMode::DrainAll).is_empty());
    }

    // ── has_steer / has_followup ──────────────────────────────────────────────

    /// has_steer reflects queue occupancy accurately.
    #[test]
    fn has_steer_tracks_occupancy() {
        let mut q = Queues::new();
        assert!(!q.has_steer());
        q.push_steer("x");
        assert!(q.has_steer());
        q.drain_steer(DrainMode::DrainAll);
        assert!(!q.has_steer());
    }

    /// has_followup reflects queue occupancy accurately.
    #[test]
    fn has_followup_tracks_occupancy() {
        let mut q = Queues::new();
        assert!(!q.has_followup());
        q.push_followup("y");
        assert!(q.has_followup());
        q.drain_followup(DrainMode::OneAtATime);
        assert!(!q.has_followup());
    }
}
