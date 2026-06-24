//! T48 — graded conformance score (V31). Quality as a %, not a binary gate.
//!
//! `spec check` answers "valid? yes/no". `score` answers "how close to the
//! ideal, 0-100%?" — so a weird spec≠code case dents the number instead of
//! flipping a boolean, and we watch the % converge over commits rather than
//! expecting one big fix. Every dimension is deterministic (⊥ LLM).

use crate::check;
use crate::store::{Status, Store};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Serialize)]
pub struct Dim {
    pub name: String,
    pub pct: f64,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Score {
    pub overall: f64,
    pub dims: Vec<Dim>,
}

/// §I `cmd:` interface lines → declared command paths (`"spec apply"`).
/// Keeps only leading bareword segments (drops `<args>`, `[opts]`, `--flags`).
pub fn declared_cmds(store: &Store) -> Vec<String> {
    store
        .interfaces
        .iter()
        .filter_map(|line| {
            let l = line.strip_prefix("cmd:")?.trim();
            let inner = l.split('`').nth(1)?; // `kittenscrew spec apply …`
            let rest = inner.strip_prefix("kittenscrew")?.trim();
            let path: Vec<&str> = rest
                .split_whitespace()
                .take_while(|w| w.chars().all(|c| c.is_ascii_lowercase()))
                .collect();
            (!path.is_empty()).then(|| path.join(" "))
        })
        .collect()
}

/// Aggregate conformance. `binary_cmds` = the actual CLI subcommand paths
/// (clap introspection, supplied by `main`); `synced` = SPEC.md ≡ store.
pub fn conformance(store: &Store, binary_cmds: &HashSet<String>, synced: bool) -> Score {
    let dims = vec![
        interface_dim(store, binary_cmds),
        check_done_dim(store),
        dep_coverage_dim(store),
        value_coverage_dim(store),
        Dim {
            name: "sync".into(),
            pct: if synced { 100.0 } else { 0.0 },
            detail: if synced {
                "SPEC.md ≡ store".into()
            } else {
                "drift pending".into()
            },
        },
    ];
    let overall = round1(dims.iter().map(|d| d.pct).sum::<f64>() / dims.len() as f64);
    Score { overall, dims }
}

/// % of §I-declared commands that actually exist in the binary (V28).
fn interface_dim(store: &Store, binary_cmds: &HashSet<String>) -> Dim {
    let declared = declared_cmds(store);
    let missing: Vec<String> = declared
        .iter()
        .filter(|c| !binary_cmds.contains(*c))
        .cloned()
        .collect();
    let pct = pct(declared.len() - missing.len(), declared.len());
    Dim {
        name: "interface_completeness".into(),
        pct,
        detail: if missing.is_empty() {
            format!("{}/{} §I cmds built", declared.len(), declared.len())
        } else {
            format!("declared-but-unbuilt: {}", missing.join(", "))
        },
    }
}

/// % of `done` tasks that pass `check done` (no fake delivery, cites intact).
fn check_done_dim(store: &Store) -> Dim {
    let reports = check::check_done(store);
    let ok = reports.iter().filter(|r| r.ok).count();
    Dim {
        name: "check_done_pass".into(),
        pct: pct(ok, reports.len()),
        detail: format!("{ok}/{} done tasks clean", reports.len()),
    }
}

/// % of live tasks wired into the DAG (have a dep or a dependent) — the bug we
/// found: an edgeless graph makes every plan query degenerate.
fn dep_coverage_dim(store: &Store) -> Dim {
    let live: Vec<&str> = store
        .tasks
        .iter()
        .filter(|t| t.status != Status::Killed)
        .map(|t| t.id.as_str())
        .collect();
    let depended: HashSet<&str> = store
        .tasks
        .iter()
        .flat_map(|t| t.deps.iter().map(|d| d.as_str()))
        .collect();
    let wired = store
        .tasks
        .iter()
        .filter(|t| t.status != Status::Killed)
        .filter(|t| !t.deps.is_empty() || depended.contains(t.id.as_str()))
        .count();
    Dim {
        name: "dep_coverage".into(),
        pct: pct(wired, live.len()),
        detail: format!("{wired}/{} live tasks in the DAG", live.len()),
    }
}

/// % of live tasks carrying an authored `value` (the worth model is only as
/// good as the values fed it).
fn value_coverage_dim(store: &Store) -> Dim {
    let live: Vec<&_> = store
        .tasks
        .iter()
        .filter(|t| t.status != Status::Killed)
        .collect();
    let scored = live.iter().filter(|t| t.value > 0).count();
    Dim {
        name: "value_coverage".into(),
        pct: pct(scored, live.len()),
        detail: format!("{scored}/{} live tasks scored", live.len()),
    }
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        100.0 // nothing to fail → vacuously complete
    } else {
        round1(n as f64 / total as f64 * 100.0)
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Invariant, Task};

    fn store_with(tasks: Vec<Task>, interfaces: Vec<&str>) -> Store {
        Store {
            tasks,
            interfaces: interfaces.iter().map(|s| s.to_string()).collect(),
            invariants: vec![Invariant {
                id: "V1".into(),
                text: "x".into(),
            }],
            ..Default::default()
        }
    }

    fn t(id: &str, status: Status, deps: &[&str], value: i64) -> Task {
        Task {
            id: id.into(),
            status,
            deps: deps.iter().map(|s| s.to_string()).collect(),
            value,
            ..Default::default()
        }
    }

    #[test]
    fn declared_cmds_strips_args_and_flags() {
        let s = store_with(
            vec![],
            vec![
                "cmd: `kittenscrew spec apply` → …",
                "cmd: `kittenscrew kitty says <kitty> <message>` → …",
                "cmd: `kittenscrew --version` → …", // flag, not a subcommand
            ],
        );
        assert_eq!(declared_cmds(&s), vec!["spec apply", "kitty says"]);
    }

    #[test]
    fn interface_dim_flags_unbuilt_cmds() {
        let s = store_with(
            vec![],
            vec![
                "cmd: `kittenscrew spec apply` → …",
                "cmd: `kittenscrew docs task <id>` → …", // declared, not built
            ],
        );
        let binary: HashSet<String> = ["spec apply".to_string()].into_iter().collect();
        let d = interface_dim(&s, &binary);
        assert_eq!(d.pct, 50.0);
        assert!(d.detail.contains("docs task"));
    }

    #[test]
    fn dep_and_value_coverage_grade_gradually() {
        // T1 wired (dep T2 depends on it), T2 wired (has dep), T3 lone+unscored.
        let s = store_with(
            vec![
                t("T1", Status::Todo, &[], 5),
                t("T2", Status::Todo, &["T1"], 0),
                t("T3", Status::Todo, &[], 0),
            ],
            vec![],
        );
        let binary = HashSet::new();
        let score = conformance(&s, &binary, true);
        let dep = score
            .dims
            .iter()
            .find(|d| d.name == "dep_coverage")
            .unwrap();
        let val = score
            .dims
            .iter()
            .find(|d| d.name == "value_coverage")
            .unwrap();
        assert_eq!(dep.pct, 66.7); // T1,T2 wired / 3
        assert_eq!(val.pct, 33.3); // only T1 scored / 3
    }

    #[test]
    fn killed_tasks_excluded_from_coverage() {
        let s = store_with(
            vec![
                t("T1", Status::Done, &[], 4),
                t("T2", Status::Killed, &[], 0),
            ],
            vec![],
        );
        let val = value_coverage_dim(&s);
        assert_eq!(val.pct, 100.0); // T2 killed → ignored; T1 scored
    }
}
