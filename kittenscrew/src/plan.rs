//! T28/T34/T35/T36 — DAG plan engine. Pure, deterministic, O(V+E) (V20).
//!
//! Order is never stored (V13): every query derives it from `task.deps`.
//! A dep is *satisfied* when its task is Done or Killed (`∅` = dropped, never a
//! blocker). All queries are stable: same store → same result.

use crate::store::{Status, Store, Task};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Trailing-digit numeric id for stable ordering (`T10` → 10).
fn id_num(id: &str) -> i64 {
    let digits: String = id.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(i64::MAX)
}

fn satisfied(s: &Store, dep: &str) -> bool {
    matches!(
        s.task(dep).map(|t| t.status),
        Some(Status::Done) | Some(Status::Killed)
    )
}

fn is_open(t: &Task) -> bool {
    matches!(t.status, Status::Todo | Status::Wip)
}

/// READY frontier (V17): open tasks with every dep satisfied, sorted by
/// (priority, id) — the parallelizable batch.
pub fn ready(s: &Store) -> Vec<&Task> {
    let mut r: Vec<&Task> = s
        .tasks
        .iter()
        .filter(|t| is_open(t) && t.deps.iter().all(|d| satisfied(s, d)))
        .collect();
    r.sort_by_key(|t| (t.priority, id_num(&t.id)));
    r
}

pub fn next(s: &Store) -> Option<&Task> {
    ready(s).into_iter().next()
}

/// Direct dependents of `id` still open — who is waiting on it.
pub fn blocking(s: &Store, id: &str) -> Vec<String> {
    let mut out: Vec<String> = s
        .tasks
        .iter()
        .filter(|t| is_open(t) && t.deps.iter().any(|d| d == id))
        .map(|t| t.id.clone())
        .collect();
    out.sort_by_key(|i| id_num(i));
    out
}

/// Topological order over dep edges (prereqs first). Err = cycle node set.
pub fn topo(s: &Store) -> Result<Vec<String>, Vec<String>> {
    let ids: HashSet<&str> = s.tasks.iter().map(|t| t.id.as_str()).collect();
    // indegree = number of (existing) deps.
    let mut indeg: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in &s.tasks {
        indeg.entry(t.id.as_str()).or_insert(0);
        for d in &t.deps {
            if ids.contains(d.as_str()) {
                *indeg.entry(t.id.as_str()).or_insert(0) += 1;
                dependents
                    .entry(d.as_str())
                    .or_default()
                    .push(t.id.as_str());
            }
        }
    }
    // deterministic: process lowest-id first at every step.
    let mut roots: Vec<&str> = indeg
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&k, _)| k)
        .collect();
    roots.sort_by_key(|i| id_num(i));
    let mut queue: VecDeque<&str> = roots.into_iter().collect();
    let mut order = Vec::new();
    while let Some(n) = queue.pop_front() {
        order.push(n.to_string());
        let mut newly: Vec<&str> = Vec::new();
        if let Some(deps) = dependents.get(n) {
            for &dep in deps {
                let e = indeg.get_mut(dep).unwrap();
                *e -= 1;
                if *e == 0 {
                    newly.push(dep);
                }
            }
        }
        newly.sort_by_key(|i| id_num(i));
        for dep in newly {
            queue.push_back(dep);
        }
    }
    if order.len() == s.tasks.len() {
        Ok(order)
    } else {
        // remaining nodes are in / downstream of a cycle.
        let done: HashSet<&str> = order.iter().map(|s| s.as_str()).collect();
        let mut cyc: Vec<String> = s
            .tasks
            .iter()
            .map(|t| t.id.clone())
            .filter(|i| !done.contains(i.as_str()))
            .collect();
        cyc.sort_by_key(|i| id_num(i));
        Err(cyc)
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Impact {
    pub id: String,
    /// What this task delivers.
    pub scope: Vec<String>,
    /// Tasks that become READY the moment `id` flips to Done.
    pub unblocks: Vec<String>,
    /// All transitive dependents waiting (directly or via chain) on `id`.
    pub blocks: Vec<String>,
}

/// T35 — what choosing/doing `id` carries: scope delivered, edges it frees,
/// edges it gates.
pub fn impact(s: &Store, id: &str) -> Impact {
    let scope = s.task(id).map(|t| t.scope.clone()).unwrap_or_default();

    // Newly ready: direct dependents whose *other* deps are already satisfied.
    let mut unblocks: Vec<String> = s
        .tasks
        .iter()
        .filter(|t| {
            is_open(t)
                && t.deps.iter().any(|d| d == id)
                && t.deps
                    .iter()
                    .filter(|d| d.as_str() != id)
                    .all(|d| satisfied(s, d))
        })
        .map(|t| t.id.clone())
        .collect();
    unblocks.sort_by_key(|i| id_num(i));

    // Transitive dependents (everything downstream of id).
    let mut blocks: Vec<String> = transitive_dependents(s, id).into_iter().collect();
    blocks.sort_by_key(|i| id_num(i));

    Impact {
        id: id.to_string(),
        scope,
        unblocks,
        blocks,
    }
}

fn transitive_dependents(s: &Store, id: &str) -> HashSet<String> {
    let mut rev: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in &s.tasks {
        for d in &t.deps {
            rev.entry(d.as_str()).or_default().push(t.id.as_str());
        }
    }
    let mut seen = HashSet::new();
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        if let Some(deps) = rev.get(n) {
            for &dep in deps {
                if seen.insert(dep.to_string()) {
                    stack.push(dep);
                }
            }
        }
    }
    seen
}

/// T34 — critical path: longest prerequisite chain. With `goal`, the chain
/// ending at goal; otherwise the longest chain in the DAG. Returns ids in
/// build order (deepest prereq first → target last).
pub fn critical_path(s: &Store, goal: Option<&str>) -> Vec<String> {
    let by_id: HashMap<&str, &Task> = s.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    let mut visiting: HashSet<String> = HashSet::new();

    // longest chain ENDING at node (inclusive), prereqs first.
    fn chain(
        id: &str,
        by_id: &HashMap<&str, &Task>,
        memo: &mut HashMap<String, Vec<String>>,
        visiting: &mut HashSet<String>,
    ) -> Vec<String> {
        if let Some(c) = memo.get(id) {
            return c.clone();
        }
        if !visiting.insert(id.to_string()) {
            return vec![id.to_string()]; // cycle guard
        }
        let mut best: Vec<String> = Vec::new();
        if let Some(t) = by_id.get(id) {
            for d in &t.deps {
                if by_id.contains_key(d.as_str()) {
                    let c = chain(d, by_id, memo, visiting);
                    if c.len() > best.len() {
                        best = c;
                    }
                }
            }
        }
        best.push(id.to_string());
        visiting.remove(id);
        memo.insert(id.to_string(), best.clone());
        best
    }

    match goal {
        Some(g) => chain(g, &by_id, &mut memo, &mut visiting),
        None => {
            let mut longest: Vec<String> = Vec::new();
            let mut ids: Vec<&str> = s.tasks.iter().map(|t| t.id.as_str()).collect();
            ids.sort_by_key(|i| id_num(i));
            for id in ids {
                let c = chain(id, &by_id, &mut memo, &mut visiting);
                if c.len() > longest.len() {
                    longest = c;
                }
            }
            longest
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AltRoute {
    pub id: String,
    pub task: String,
    pub scope: Vec<String>,
    /// Count of tasks this choice immediately frees.
    pub unblocks: usize,
    /// Count of tasks downstream (eventually gated on it).
    pub blocks: usize,
}

/// T36 — at the current frontier, each available choice with its payoff:
/// scope delivered, how many tasks it frees now, how many it gates downstream.
/// Sorted by unblocks desc (highest leverage first).
pub fn alternatives(s: &Store) -> Vec<AltRoute> {
    let mut out: Vec<AltRoute> = ready(s)
        .into_iter()
        .map(|t| {
            let imp = impact(s, &t.id);
            AltRoute {
                id: t.id.clone(),
                task: t.task.clone(),
                scope: t.scope.clone(),
                unblocks: imp.unblocks.len(),
                blocks: imp.blocks.len(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.unblocks
            .cmp(&a.unblocks)
            .then(id_num(&a.id).cmp(&id_num(&b.id)))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Task;

    fn task(id: &str, status: Status, deps: &[&str], priority: i64) -> Task {
        Task {
            id: id.into(),
            status,
            task: format!("task {id}"),
            deps: deps.iter().map(|s| s.to_string()).collect(),
            priority,
            cites: vec![],
            scope: vec![format!("src/{id}.rs")],
            note: String::new(),
        }
    }

    //   T1(done) → T2 → T4
    //              T3 → T4
    //   T5 (independent)
    fn graph() -> Store {
        let mut s = Store::default();
        s.tasks = vec![
            task("T1", Status::Done, &[], 0),
            task("T2", Status::Todo, &["T1"], 0),
            task("T3", Status::Todo, &["T1"], 0),
            task("T4", Status::Todo, &["T2", "T3"], 0),
            task("T5", Status::Todo, &[], 5),
        ];
        s
    }

    #[test]
    fn ready_is_unblocked_batch_sorted() {
        let s = graph();
        let ids: Vec<&str> = ready(&s).iter().map(|t| t.id.as_str()).collect();
        // T2,T3 ready (T1 done); T5 ready but priority 5 → last. T4 blocked.
        assert_eq!(ids, vec!["T2", "T3", "T5"]);
    }

    #[test]
    fn next_respects_priority() {
        assert_eq!(next(&graph()).unwrap().id, "T2");
    }

    #[test]
    fn blocking_direct_dependents() {
        assert_eq!(blocking(&graph(), "T2"), vec!["T4"]);
        assert_eq!(blocking(&graph(), "T1"), vec!["T2", "T3"]);
    }

    #[test]
    fn topo_orders_prereqs_first() {
        let order = topo(&graph()).unwrap();
        let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
        assert!(pos("T1") < pos("T2"));
        assert!(pos("T2") < pos("T4"));
        assert!(pos("T3") < pos("T4"));
        assert_eq!(order.len(), 5);
    }

    #[test]
    fn topo_detects_cycle() {
        let mut s = graph();
        s.tasks[1].deps = vec!["T4".into()]; // T2→T4 and T4→T2,T3 → cycle
        let err = topo(&s).unwrap_err();
        assert!(err.contains(&"T2".to_string()) && err.contains(&"T4".to_string()));
    }

    #[test]
    fn impact_unblocks_and_blocks() {
        let s = graph();
        // Doing T2 alone does NOT free T4 (T3 still open).
        let i2 = impact(&s, "T2");
        assert!(i2.unblocks.is_empty());
        assert_eq!(i2.blocks, vec!["T4"]);
        assert_eq!(i2.scope, vec!["src/T2.rs"]);
    }

    #[test]
    fn impact_newly_ready_when_last_dep() {
        let mut s = graph();
        s.tasks[1].status = Status::Done; // T2 done; doing T3 now frees T4
        let i3 = impact(&s, "T3");
        assert_eq!(i3.unblocks, vec!["T4"]);
    }

    #[test]
    fn critical_path_longest_chain() {
        // T1→T2→T4 (or T1→T3→T4): length 3.
        let p = critical_path(&graph(), Some("T4"));
        assert_eq!(p.len(), 3);
        assert_eq!(p.first().unwrap(), "T1");
        assert_eq!(p.last().unwrap(), "T4");
    }

    #[test]
    fn alternatives_rank_by_leverage() {
        let s = graph();
        let alts = alternatives(&s);
        let ids: Vec<&str> = alts.iter().map(|a| a.id.as_str()).collect();
        // all three ready; none unblocks immediately → tiebreak by id.
        assert_eq!(ids, vec!["T2", "T3", "T5"]);
        let t2 = alts.iter().find(|a| a.id == "T2").unwrap();
        assert_eq!(t2.blocks, 1); // T4 downstream
    }
}
