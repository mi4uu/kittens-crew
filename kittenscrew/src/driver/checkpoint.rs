//! T69 — coupled checkpoint + rollback for the DAG loop.
//!
//! Each checkpoint captures conversation/loop state (`state`) AND a code snapshot
//! reference (`code_ref`) together, so `rollback(n)` reverts both at once — a bad
//! turn becomes fully undoable. Checkpoints are keyed by `(node_id, run_id)`:
//! snapshotting the same `run_id` twice is a no-op (idempotent memoization), which
//! makes the log crash-resumable — re-running a node that already checkpointed just
//! returns the cached entry.
//!
//! `code_ref` today is a plain string (e.g. a serialised state blob or a stash label
//! like `"stash@{0}"`). All state lives in memory; `to_json` / `from_json` provide
//! optional durable serialisation via `serde_json`.
//!
//! # ponytail: real upgrade path
//! Replace `code_ref: String` with an actual shadow-git stash ref created by
//! `git stash create` in a bare clone of the workspace.  `rollback` would then run
//! `git checkout <code_ref>` in that clone to restore files on disk.  The rest of
//! the API is unchanged.

use serde::{Deserialize, Serialize};

/// A single coupled checkpoint: loop state + code snapshot reference.
///
/// `code_ref` is an opaque string standing in for a shadow-git stash ref.
/// In the real upgrade path (see module-level ponytail comment) it would be
/// the SHA returned by `git stash create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Identifier of the DAG node this checkpoint belongs to.
    pub node_id: String,
    /// Stable run identifier — used as the idempotency key.  Two snapshots
    /// with the same `run_id` represent the same execution attempt.
    pub run_id: String,
    /// Serialised conversation / loop state blob (opaque to this module).
    pub state: String,
    /// Code snapshot reference.  Opaque string; real impl = shadow-git stash ref.
    /// ponytail: replace with `git stash create` SHA in a bare workspace clone.
    pub code_ref: String,
}

/// An ordered, append-only log of checkpoints with rollback support.
///
/// Invariant: no two entries share the same `run_id` — `snapshot` enforces this.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CheckpointLog {
    entries: Vec<Checkpoint>,
}

impl CheckpointLog {
    /// Create an empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a checkpoint for `(node_id, run_id)`.
    ///
    /// **Idempotent on `run_id`**: if the log already contains an entry with the
    /// same `run_id`, this is a no-op — the cached checkpoint is kept unchanged.
    /// This makes the log crash-resumable: re-running a node that already
    /// checkpointed will not create a duplicate.
    pub fn snapshot(&mut self, node_id: &str, run_id: &str, state: &str, code_ref: &str) {
        if self.entries.iter().any(|e| e.run_id == run_id) {
            return; // idempotent: already checkpointed this run
        }
        self.entries.push(Checkpoint {
            node_id: node_id.to_string(),
            run_id: run_id.to_string(),
            state: state.to_string(),
            code_ref: code_ref.to_string(),
        });
    }

    /// Revert to checkpoint index `n`: drop all entries after index `n`, then
    /// return a reference to the restored checkpoint at position `n`.
    ///
    /// Returns `None` if `n` is out of range (log is left unchanged in that case).
    pub fn rollback(&mut self, n: usize) -> Option<&Checkpoint> {
        if n >= self.entries.len() {
            return None;
        }
        self.entries.truncate(n + 1);
        self.entries.last()
    }

    /// Number of checkpoints currently in the log.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the log has no checkpoints.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read-only view of entry at index `i`.
    pub fn get(&self, i: usize) -> Option<&Checkpoint> {
        self.entries.get(i)
    }

    /// Serialise the whole log to a JSON string (optional persistence).
    /// ponytail: write this to `.kittenscrew/checkpoints.json` for durable
    /// crash-resume; load on startup with `from_json`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserialise from a JSON string produced by `to_json`.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ck(node: &str, run: &str, state: &str, code: &str) -> (String, String, String, String) {
        (node.into(), run.into(), state.into(), code.into())
    }

    /// `snapshot` appends entries; `len` tracks the count.
    #[test]
    fn snapshot_grows_log() {
        let mut log = CheckpointLog::new();
        assert_eq!(log.len(), 0);

        log.snapshot("T1", "run-1", "state-a", "stash@{0}");
        assert_eq!(log.len(), 1);

        log.snapshot("T2", "run-2", "state-b", "stash@{1}");
        assert_eq!(log.len(), 2);

        let c0 = log.get(0).unwrap();
        assert_eq!(c0.node_id, "T1");
        assert_eq!(c0.run_id, "run-1");
        assert_eq!(c0.state, "state-a");
        assert_eq!(c0.code_ref, "stash@{0}");
    }

    /// Snapshotting the same `run_id` twice must NOT duplicate the entry.
    #[test]
    fn snapshot_same_run_id_is_idempotent() {
        let mut log = CheckpointLog::new();
        log.snapshot("T1", "run-dup", "first-state", "ref-1");
        log.snapshot("T1", "run-dup", "second-state", "ref-2"); // same run_id
        log.snapshot("T2", "run-other", "other-state", "ref-3");

        // Still only two entries: the second call with "run-dup" was a no-op.
        assert_eq!(log.len(), 2, "duplicate run_id must not grow the log");

        // The original snapshot is preserved unchanged.
        let first = log.get(0).unwrap();
        assert_eq!(first.state, "first-state", "cached snapshot must not be overwritten");
        assert_eq!(first.code_ref, "ref-1");
    }

    /// `rollback(n)` restores checkpoint `n` and drops all later entries.
    #[test]
    fn rollback_drops_later_entries() {
        let mut log = CheckpointLog::new();
        log.snapshot("T1", "r1", "s1", "c1");
        log.snapshot("T2", "r2", "s2", "c2");
        log.snapshot("T3", "r3", "s3", "c3");
        assert_eq!(log.len(), 3);

        let restored = log.rollback(1).unwrap();
        assert_eq!(restored.node_id, "T2");
        assert_eq!(restored.state, "s2");
        assert_eq!(restored.code_ref, "c2");

        // Entries after index 1 are gone.
        assert_eq!(log.len(), 2, "entries after rollback point must be dropped");
        assert!(log.get(2).is_none());
    }

    /// `rollback(0)` restores the very first checkpoint.
    #[test]
    fn rollback_to_zero_restores_first() {
        let mut log = CheckpointLog::new();
        log.snapshot("T1", "r1", "initial", "base-ref");
        log.snapshot("T2", "r2", "later", "later-ref");
        log.snapshot("T3", "r3", "latest", "latest-ref");

        let restored = log.rollback(0).unwrap();
        assert_eq!(restored.node_id, "T1");
        assert_eq!(restored.state, "initial");
        assert_eq!(log.len(), 1);
    }

    /// Out-of-range rollback returns `None` and leaves the log intact.
    #[test]
    fn rollback_out_of_range_is_none() {
        let mut log = CheckpointLog::new();
        log.snapshot("T1", "r1", "s1", "c1");

        assert!(log.rollback(5).is_none(), "out-of-range rollback must return None");
        assert_eq!(log.len(), 1, "log must be unchanged after failed rollback");
    }

    /// Round-trip through JSON serialisation preserves all fields.
    #[test]
    fn json_roundtrip() {
        let mut log = CheckpointLog::new();
        log.snapshot("T1", "r1", "state-x", "ref-x");
        log.snapshot("T2", "r2", "state-y", "ref-y");

        let json = log.to_json();
        let restored = CheckpointLog::from_json(&json).unwrap();

        assert_eq!(restored.len(), 2);
        assert_eq!(restored.get(0).unwrap().state, "state-x");
        assert_eq!(restored.get(1).unwrap().code_ref, "ref-y");
    }
}
