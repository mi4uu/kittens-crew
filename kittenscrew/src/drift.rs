//! T29 — drift reconcile (V16).
//!
//! An agent edited SPEC.md directly (drift from the store, which is truth, V9).
//! We re-import that edited SPEC.md (`incoming`) and diff it against the store
//! projection (`current`):
//!
//! - §T task changes (add / remove / status / deps / cites / prose) are
//!   STRUCTURAL → auto-reconciled into the store.
//! - §G/§C/§I/§V/§B prose changes are AMBIGUOUS → reported so the caller (LLM)
//!   reviews them; we still adopt them (SPEC.md is the human's authored edit),
//!   but never silently — the report is the escalation.
//!
//! `priority` and `scope` are toml-only (don't round-trip through SPEC.md), so
//! reconcile carries them over from the current store by task id.

use crate::store::Store;
use serde::Serialize;

/// What changed between the store projection and the edited SPEC.md.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Drift {
    pub task_added: Vec<String>,
    pub task_removed: Vec<String>,
    pub task_changed: Vec<TaskChange>,
    /// Prose sections that differ — escalated to the caller (V16).
    pub prose_changed: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct TaskChange {
    pub id: String,
    pub fields: Vec<String>,
}

impl Drift {
    pub fn is_empty(&self) -> bool {
        self.task_added.is_empty()
            && self.task_removed.is_empty()
            && self.task_changed.is_empty()
            && self.prose_changed.is_empty()
    }
}

/// Diff `incoming` (edited SPEC.md, re-imported) against `current` (store).
pub fn diff(current: &Store, incoming: &Store) -> Drift {
    let mut d = Drift::default();

    for inc in &incoming.tasks {
        match current.tasks.iter().find(|c| c.id == inc.id) {
            None => d.task_added.push(inc.id.clone()),
            Some(cur) => {
                let mut fields = Vec::new();
                if cur.status != inc.status {
                    fields.push("status".into());
                }
                if cur.task != inc.task {
                    fields.push("task".into());
                }
                if cur.deps != inc.deps {
                    fields.push("deps".into());
                }
                if cur.cites != inc.cites {
                    fields.push("cites".into());
                }
                if cur.note != inc.note {
                    fields.push("note".into());
                }
                if !fields.is_empty() {
                    d.task_changed.push(TaskChange {
                        id: inc.id.clone(),
                        fields,
                    });
                }
            }
        }
    }
    for cur in &current.tasks {
        if !incoming.tasks.iter().any(|i| i.id == cur.id) {
            d.task_removed.push(cur.id.clone());
        }
    }

    if current.goal != incoming.goal {
        d.prose_changed.push("§G".into());
    }
    if current.constraints != incoming.constraints {
        d.prose_changed.push("§C".into());
    }
    if current.interfaces != incoming.interfaces {
        d.prose_changed.push("§I".into());
    }
    if current.invariants != incoming.invariants {
        d.prose_changed.push("§V".into());
    }
    if current.bugs != incoming.bugs {
        d.prose_changed.push("§B".into());
    }

    d
}

/// Adopt `incoming` as the new store, carrying over toml-only fields
/// (`priority`, `scope`) from `current` for tasks that already existed.
pub fn reconcile(current: &Store, incoming: &Store) -> Store {
    let mut merged = incoming.clone();
    for t in merged.tasks.iter_mut() {
        if let Some(old) = current.tasks.iter().find(|o| o.id == t.id) {
            t.priority = old.priority;
            t.scope = old.scope.clone();
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Status, Task};

    fn task(id: &str, status: Status, text: &str) -> Task {
        Task {
            id: id.into(),
            status,
            task: text.into(),
            deps: vec![],
            priority: 7,
            cites: vec![],
            scope: vec!["src/x.rs".into()],
            note: String::new(),
            ..Default::default()
        }
    }

    fn store(tasks: Vec<Task>) -> Store {
        Store {
            tasks,
            ..Default::default()
        }
    }

    #[test]
    fn no_drift_when_identical() {
        let s = store(vec![task("T1", Status::Done, "a")]);
        assert!(diff(&s, &s.clone()).is_empty());
    }

    #[test]
    fn detects_added_removed_and_status_change() {
        let cur = store(vec![
            task("T1", Status::Todo, "a"),
            task("T2", Status::Todo, "b"),
        ]);
        let inc = store(vec![
            task("T1", Status::Done, "a"),
            task("T3", Status::Todo, "c"),
        ]);
        let d = diff(&cur, &inc);
        assert_eq!(d.task_added, vec!["T3".to_string()]);
        assert_eq!(d.task_removed, vec!["T2".to_string()]);
        assert_eq!(d.task_changed.len(), 1);
        assert_eq!(d.task_changed[0].id, "T1");
        assert_eq!(d.task_changed[0].fields, vec!["status".to_string()]);
    }

    #[test]
    fn detects_prose_change() {
        let cur = store(vec![]);
        let mut inc = store(vec![]);
        inc.goal = "new goal".into();
        assert_eq!(diff(&cur, &inc).prose_changed, vec!["§G".to_string()]);
    }

    #[test]
    fn reconcile_preserves_toml_only_fields() {
        // incoming (from SPEC.md) lost priority+scope; reconcile restores them by id.
        let cur = store(vec![task("T1", Status::Todo, "a")]); // priority 7, scope src/x.rs
        let mut imported = task("T1", Status::Done, "a");
        imported.priority = 0;
        imported.scope = vec![];
        let inc = store(vec![imported]);
        let merged = reconcile(&cur, &inc);
        let t = &merged.tasks[0];
        assert_eq!(t.status, Status::Done); // structural edit adopted
        assert_eq!(t.priority, 7); // toml-only carried over
        assert_eq!(t.scope, vec!["src/x.rs".to_string()]);
    }

    #[test]
    fn reconcile_keeps_new_task_defaults() {
        let cur = store(vec![]);
        let inc = store(vec![task("T9", Status::Todo, "fresh")]);
        let merged = reconcile(&cur, &inc);
        assert_eq!(merged.tasks.len(), 1);
        assert_eq!(merged.tasks[0].id, "T9");
    }
}
