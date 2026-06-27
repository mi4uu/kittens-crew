//! T77 — delegation / parallelization SAFE gate + a parallel drive loop.
//!
//! The deterministic core of "what can run concurrently": among the ready frontier,
//! the largest batch of tasks that are pairwise (a) dependency-independent and
//! (b) scope-disjoint ("don't block each other" = no write-conflict). Among *ready*
//! tasks (deps already satisfied) the dep check is automatically true, so the real
//! gate is scope-disjointness — overlapping write targets MUST serialize. This is
//! pure graph+set math over the store (deps + scope already there): no model, no IO.
//!
//! Parallel execution is safe BY CONSTRUCTION: a batch's tasks write disjoint scope
//! files, and the store is only mutated (mark-done) serially after the batch joins —
//! so there is no write race. Conflict-handling for *overlapping* parallel work
//! stays an open edge (we simply never batch overlapping scopes).

use super::api::{Driver, Turn};
use super::drive::{extract_code, mark_done, scoped_prompt, DriveOpts, Outcome};
use super::verify;
use crate::plan;
use crate::store::{Store, Task};
use std::path::{Path, PathBuf};

/// A safe-to-parallelize batch from the current ready frontier: pairwise
/// dep-independent AND scope-disjoint. Greedy-maximal in store order (deterministic).
pub fn parallelizable_batch(store: &Store) -> Vec<&Task> {
    let mut batch: Vec<&Task> = Vec::new();
    for t in plan::ready(store) {
        if batch.iter().all(|b| compatible(b, t)) {
            batch.push(t);
        }
    }
    batch
}

/// Two ready tasks may run together iff neither depends on the other AND their scopes
/// are disjoint (no shared write target). The dep clause is defensive — among ready
/// tasks it already holds — but keeps the rule correct if `ready` semantics change.
fn compatible(a: &Task, b: &Task) -> bool {
    !a.deps.iter().any(|d| d == &b.id)
        && !b.deps.iter().any(|d| d == &a.id)
        && scope_disjoint(a, b)
}

fn scope_disjoint(a: &Task, b: &Task) -> bool {
    !a.scope.iter().any(|s| b.scope.contains(s))
}

/// Parallel drive: each round runs the safe batch concurrently (one thread per node),
/// then marks the green ones done serially (no store race). Subsumes serial drive — a
/// frontier with no parallelism just yields batches of one. Halts on a failed verify
/// in a batch (parallel replan is an open edge; serial `drive` already does retry).
pub fn drive_parallel<D: Driver + Sync>(
    driver: &D,
    opts: &DriveOpts,
    mut progress: impl FnMut(&str, &str),
) -> Result<Outcome, String> {
    let mut done = 0u32;
    for _ in 0..opts.max_iters {
        let store = Store::load(&opts.store_path).map_err(|e| e.to_string())?;

        let batch: Vec<(String, String, PathBuf)> = parallelizable_batch(&store)
            .iter()
            .filter_map(|t| {
                t.scope
                    .first()
                    .map(|s| (t.id.clone(), t.task.clone(), PathBuf::from(s)))
            })
            .collect();

        if batch.is_empty() {
            return match plan::next(&store) {
                None => Ok(Outcome::Converged { done }),
                Some(t) => Ok(Outcome::Halted {
                    node: t.id.clone(),
                    reason: "node has no scope file to write".into(),
                    done,
                }),
            };
        }

        // Run the disjoint-scope batch concurrently. Each thread fills + verifies one
        // node and writes only its own scope file (disjoint → no clash).
        let results: Vec<(String, bool, String)> = std::thread::scope(|s| {
            let handles: Vec<_> = batch
                .iter()
                .map(|(id, task, target)| {
                    s.spawn(move || run_one(driver, id.as_str(), task.as_str(), target.as_path()))
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        // Advance: mark the green nodes done (serial — the only store mutation point).
        for (id, ok, model) in &results {
            if *ok {
                mark_done(&opts.store_path, id)?;
                progress(id, model);
                done += 1;
            }
        }
        if let Some((id, _, _)) = results.iter().find(|(_, ok, _)| !*ok) {
            return Ok(Outcome::Halted {
                node: id.clone(),
                reason: "verify failed in parallel batch".into(),
                done,
            });
        }
    }
    Ok(Outcome::CapReached { done })
}

/// Fill + verify one leaf. Pure per-node work, safe to run on its own thread.
fn run_one<D: Driver>(driver: &D, id: &str, task: &str, target: &Path) -> (String, bool, String) {
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match driver.dispatch(&Turn {
        prompt: scoped_prompt(task, target),
    }) {
        Ok(res) => {
            let code = extract_code(&res.text);
            if std::fs::write(target, &code).is_err() {
                return (id.to_string(), false, res.model);
            }
            (id.to_string(), verify::rustc_compiles(target).ok, res.model)
        }
        Err(_) => (id.to_string(), false, String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::sim::SimDriver;

    fn store_at(tag: &str, spec: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ks_deleg_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("spec.toml");
        std::fs::write(&p, spec.replace("{DIR}", d.to_str().unwrap())).unwrap();
        p
    }

    /// Disjoint-scope ready tasks batch together; tasks sharing a scope file do not.
    #[test]
    fn batch_excludes_scope_conflicts() {
        let sp = store_at(
            "batch",
            "schema=1\n\
             [[task]]\nid=\"T1\"\nstatus=\"todo\"\ntask=\"a\"\ndeps=[]\npriority=1\nscope=[\"a.rs\"]\n\
             [[task]]\nid=\"T2\"\nstatus=\"todo\"\ntask=\"b\"\ndeps=[]\npriority=1\nscope=[\"b.rs\"]\n\
             [[task]]\nid=\"T3\"\nstatus=\"todo\"\ntask=\"a-again\"\ndeps=[]\npriority=1\nscope=[\"a.rs\"]\n",
        );
        let store = Store::load(&sp).unwrap();
        let batch: Vec<&str> = parallelizable_batch(&store).iter().map(|t| t.id.as_str()).collect();
        // T1 and T2 are disjoint -> both in; T3 shares a.rs with T1 -> excluded this round.
        assert_eq!(batch, vec!["T1", "T2"], "scope-conflicting T3 must not batch with T1");
    }

    /// Parallel drive converges a mixed DAG (two independent leaves + one dependent)
    /// with no model — deterministic, offline.
    #[test]
    fn parallel_drive_converges_offline() {
        let sp = store_at(
            "drive",
            "schema=1\n\
             [[task]]\nid=\"T1\"\nstatus=\"todo\"\ntask=\"alpha\"\ndeps=[]\npriority=1\nscope=[\"{DIR}/a.rs\"]\n\
             [[task]]\nid=\"T2\"\nstatus=\"todo\"\ntask=\"beta\"\ndeps=[]\npriority=1\nscope=[\"{DIR}/b.rs\"]\n\
             [[task]]\nid=\"T3\"\nstatus=\"todo\"\ntask=\"gamma\"\ndeps=[\"T1\",\"T2\"]\npriority=1\nscope=[\"{DIR}/c.rs\"]\n",
        );
        let sim = SimDriver::new();
        let mut done_ids = Vec::new();
        let out = drive_parallel(
            &sim,
            &DriveOpts { max_iters: 10, max_retries: 0, store_path: sp, workspace_root: None, escalation: None },
            |id, _| done_ids.push(id.to_string()),
        )
        .unwrap();
        assert!(matches!(out, Outcome::Converged { done: 3 }), "got {out:?}");
        // T3 depends on T1+T2, so it must come after both.
        let pos = |id: &str| done_ids.iter().position(|x| x == id).unwrap();
        assert!(pos("T3") > pos("T1") && pos("T3") > pos("T2"), "order: {done_ids:?}");
    }
}
