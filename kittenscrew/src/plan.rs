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

// ---- worth / ranking (T40, V22/V24; knobs T41/V24) -------------------------

/// How the forward (downstream) term aggregates child worths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agg {
    Max,    // critical-path: deepest single chain
    Sum,    // portfolio: total downstream value
    Hybrid, // max + portfolio_w·sum (default)
}

/// Which metric `rank` exposes for ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankBy {
    Worth,    // raw worth
    Roi,      // worth / difficulty
    Expected, // roi · (1 − risk/6) (default)
}

/// `[plan]` knobs feeding worth/rank (T41). Defaults reproduce the T40 behaviour
/// exactly, so unconfigured projects (and tests) are unchanged.
#[derive(Debug, Clone, Copy)]
pub struct WorthParams {
    pub gamma: f64,
    pub portfolio_w: f64,
    pub agg: Agg,
    pub rank_by: RankBy,
}

impl Default for WorthParams {
    fn default() -> Self {
        WorthParams {
            gamma: 0.85,
            portfolio_w: 0.30,
            agg: Agg::Hybrid,
            rank_by: RankBy::Expected,
        }
    }
}

/// `worth` per task (V24) with default knobs.
// kitten: default-param convenience used by tests; main always passes config knobs.
#[allow(dead_code)]
pub fn worth_map(s: &Store) -> HashMap<String, f64> {
    worth_map_with(s, &WorthParams::default())
}

/// `worth` per task (V24): `worth = value + γ·forward`, `forward` aggregated per
/// `params.agg`; children = direct dependents. Memoized DP, O(V+E) (V20).
pub fn worth_map_with(s: &Store, p: &WorthParams) -> HashMap<String, f64> {
    let ids: HashSet<&str> = s.tasks.iter().map(|t| t.id.as_str()).collect();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in &s.tasks {
        for d in &t.deps {
            if ids.contains(d.as_str()) {
                dependents
                    .entry(d.as_str())
                    .or_default()
                    .push(t.id.as_str());
            }
        }
    }
    let value_of: HashMap<&str, f64> = s
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), t.value as f64))
        .collect();
    let mut memo: HashMap<String, f64> = HashMap::new();
    for t in &s.tasks {
        worth_of(t.id.as_str(), &dependents, &value_of, p, &mut memo);
    }
    memo
}

fn worth_of(
    id: &str,
    dependents: &HashMap<&str, Vec<&str>>,
    value_of: &HashMap<&str, f64>,
    p: &WorthParams,
    memo: &mut HashMap<String, f64>,
) -> f64 {
    if let Some(&w) = memo.get(id) {
        return w;
    }
    memo.insert(id.to_string(), 0.0); // cycle guard (validate rejects real cycles)
    let children = dependents.get(id).cloned().unwrap_or_default();
    let child_worths: Vec<f64> = children
        .iter()
        .map(|c| worth_of(c, dependents, value_of, p, memo))
        .collect();
    let maxw = child_worths.iter().copied().fold(0.0, f64::max);
    let sumw: f64 = child_worths.iter().sum();
    let forward = match p.agg {
        Agg::Max => maxw,
        Agg::Sum => sumw,
        Agg::Hybrid => maxw + p.portfolio_w * sumw,
    };
    let w = value_of.get(id).copied().unwrap_or(0.0) + p.gamma * forward;
    memo.insert(id.to_string(), w);
    w
}

/// Rank with default knobs.
#[allow(dead_code)] // default-param convenience (tests); main passes config knobs.
pub fn rank_of(t: &Task, worth: &HashMap<String, f64>) -> f64 {
    rank_of_with(t, worth, &WorthParams::default())
}

/// Rank per `params.rank_by`: worth | ROI (worth/difficulty) | expected
/// (ROI·(1−risk/6)). Difficulty floors at 1. A filler (worth 0) → rank 0 (V22).
pub fn rank_of_with(t: &Task, worth: &HashMap<String, f64>, p: &WorthParams) -> f64 {
    let w = worth.get(&t.id).copied().unwrap_or(0.0);
    let diff = t.difficulty.max(1) as f64;
    match p.rank_by {
        RankBy::Worth => w,
        RankBy::Roi => w / diff,
        RankBy::Expected => (w / diff) * (1.0 - (t.risk as f64) / 6.0),
    }
}

fn rank_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal) // desc, NaN-safe
}

/// One row of the worth ranking (`plan worth`).
#[derive(Debug, Serialize)]
pub struct WorthRow {
    pub id: String,
    pub status: String,
    pub value: i64,
    pub difficulty: i64,
    pub risk: i64,
    pub worth: f64,
    pub rank: f64,
    pub ready: bool,
}

/// All tasks scored, highest rank first. `ready` flags the actionable frontier.
#[allow(dead_code)] // default-param convenience (tests); main passes config knobs.
pub fn worth_ranking(s: &Store) -> Vec<WorthRow> {
    worth_ranking_with(s, &WorthParams::default())
}

pub fn worth_ranking_with(s: &Store, p: &WorthParams) -> Vec<WorthRow> {
    let worth = worth_map_with(s, p);
    let ready_ids: HashSet<&str> = ready(s).iter().map(|t| t.id.as_str()).collect();
    let mut rows: Vec<WorthRow> = s
        .tasks
        .iter()
        .map(|t| WorthRow {
            id: t.id.clone(),
            status: format!("{:?}", t.status).to_lowercase(),
            value: t.value,
            difficulty: t.difficulty,
            risk: t.risk,
            worth: round2(worth.get(&t.id).copied().unwrap_or(0.0)),
            rank: round2(rank_of_with(t, &worth, p)),
            ready: ready_ids.contains(t.id.as_str()),
        })
        .collect();
    rows.sort_by(|a, b| rank_cmp(a.rank, b.rank).then(id_num(&a.id).cmp(&id_num(&b.id))));
    rows
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
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

/// Single best next task: highest `rank` (worth-weighted) among READY, tiebreak
/// priority then id (V22 — ⊥ pure cheapness/id). Falls back to id order when no
/// task carries value (rank all 0), preserving old behaviour on unscored specs.
#[allow(dead_code)] // default-param convenience (tests); main passes config knobs.
pub fn next(s: &Store) -> Option<&Task> {
    next_with(s, &WorthParams::default())
}

pub fn next_with<'a>(s: &'a Store, p: &WorthParams) -> Option<&'a Task> {
    let worth = worth_map_with(s, p);
    let mut r = ready(s);
    r.sort_by(|a, b| {
        rank_cmp(rank_of_with(a, &worth, p), rank_of_with(b, &worth, p))
            .then(a.priority.cmp(&b.priority))
            .then(id_num(&a.id).cmp(&id_num(&b.id)))
    });
    r.into_iter().next()
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

#[derive(Debug, Serialize, PartialEq)]
pub struct AltRoute {
    pub id: String,
    pub task: String,
    pub scope: Vec<String>,
    /// Count of tasks this choice immediately frees.
    pub unblocks: usize,
    /// Count of tasks downstream (eventually gated on it).
    pub blocks: usize,
    /// Value-weighted worth (V24).
    pub worth: f64,
    /// Risk-adjusted ROI rank (V22) — the sort key.
    pub rank: f64,
}

/// T36 — at the current frontier, each available choice with its payoff:
/// scope delivered, how many tasks it frees now, how many it gates downstream.
/// Sorted by unblocks desc (highest leverage first).
#[allow(dead_code)] // default-param convenience (tests); main passes config knobs.
pub fn alternatives(s: &Store) -> Vec<AltRoute> {
    alternatives_with(s, &WorthParams::default())
}

pub fn alternatives_with(s: &Store, p: &WorthParams) -> Vec<AltRoute> {
    let worth = worth_map_with(s, p);
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
                worth: round2(worth.get(&t.id).copied().unwrap_or(0.0)),
                rank: round2(rank_of_with(t, &worth, p)),
            }
        })
        .collect();
    // Rank (worth-weighted) first; leverage then id break ties (incl. unscored).
    out.sort_by(|a, b| {
        rank_cmp(a.rank, b.rank)
            .then(b.unblocks.cmp(&a.unblocks))
            .then(id_num(&a.id).cmp(&id_num(&b.id)))
    });
    out
}

/// T32 — ASCII DAG render: one `child → dep` edge per line (topo-ish by id),
/// status-tagged, plus isolated nodes. Deterministic, presentation-only, zero
/// deps (ponytail: a Mermaid/ascii-dag crate would be overkill for a text list).
pub fn graph(s: &Store) -> String {
    let sym = |id: &str| s.task(id).map(|t| t.status.symbol()).unwrap_or('?');
    let mut lines = vec!["DAG (child → dep it waits on):".to_string()];
    let mut edges: Vec<(String, String)> = s
        .tasks
        .iter()
        .flat_map(|t| t.deps.iter().map(move |d| (t.id.clone(), d.clone())))
        .collect();
    edges.sort_by(|a, b| {
        id_num(&a.0)
            .cmp(&id_num(&b.0))
            .then(id_num(&a.1).cmp(&id_num(&b.1)))
    });
    for (child, dep) in &edges {
        lines.push(format!("  {} {} → {} {}", sym(child), child, dep, sym(dep)));
    }
    let connected: HashSet<&str> = edges
        .iter()
        .flat_map(|(c, d)| [c.as_str(), d.as_str()])
        .collect();
    let mut isolated: Vec<&str> = s
        .tasks
        .iter()
        .map(|t| t.id.as_str())
        .filter(|id| !connected.contains(id))
        .collect();
    isolated.sort_by_key(|i| id_num(i));
    if !isolated.is_empty() {
        lines.push(format!(
            "isolated: {}",
            isolated
                .iter()
                .map(|id| format!("{}{}", sym(id), id))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    lines.join("\n")
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
            ..Default::default()
        }
    }

    //   T1(done) → T2 → T4
    //              T3 → T4
    //   T5 (independent)
    fn graph() -> Store {
        Store {
            tasks: vec![
                task("T1", Status::Done, &[], 0),
                task("T2", Status::Todo, &["T1"], 0),
                task("T3", Status::Todo, &["T1"], 0),
                task("T4", Status::Todo, &["T2", "T3"], 0),
                task("T5", Status::Todo, &[], 5),
            ],
            ..Default::default()
        }
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

    // T1 filler (value 0) vs T2 keystone (unblocks high-value T3).
    fn valued() -> Store {
        Store {
            tasks: vec![
                Task {
                    id: "T1".into(),
                    status: Status::Todo,
                    difficulty: 1,
                    ..Default::default()
                },
                Task {
                    id: "T2".into(),
                    status: Status::Todo,
                    value: 1,
                    difficulty: 1,
                    ..Default::default()
                },
                Task {
                    id: "T3".into(),
                    status: Status::Todo,
                    value: 5,
                    difficulty: 1,
                    deps: vec!["T2".into()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn worth_rewards_forward_value_not_id() {
        let s = valued();
        let w = worth_map(&s);
        // keystone T2 carries the discounted value of T3 it unlocks → beats filler T1.
        assert!(
            w["T2"] > w["T1"],
            "keystone {} ≤ filler {}",
            w["T2"],
            w["T1"]
        );
        assert_eq!(w["T1"], 0.0); // value 0, no dependents → worth 0
    }

    #[test]
    fn next_picks_worth_not_lowest_id() {
        // ready = {T1, T2}; old engine would pick T1 (lowest id). worth picks T2.
        assert_eq!(next(&valued()).unwrap().id, "T2");
    }

    #[test]
    fn filler_sinks_in_ranking() {
        let rows = worth_ranking(&valued());
        // T1 (filler) ranks last among the three despite lowest id.
        assert_eq!(rows.last().unwrap().id, "T1");
        assert_eq!(rows.first().unwrap().id, "T2"); // highest rank of the ready pair
    }

    #[test]
    fn rank_by_knob_selects_metric() {
        let s = Store {
            tasks: vec![Task {
                id: "T1".into(),
                status: Status::Todo,
                value: 10,
                difficulty: 2,
                risk: 3,
                ..Default::default()
            }],
            ..Default::default()
        };
        let w = worth_map(&s); // no children → worth = value = 10
        let t = &s.tasks[0];
        let knob = |rb| {
            rank_of_with(
                t,
                &w,
                &WorthParams {
                    rank_by: rb,
                    ..Default::default()
                },
            )
        };
        assert_eq!(knob(RankBy::Worth), 10.0);
        assert_eq!(knob(RankBy::Roi), 5.0); // 10/2
        assert!((knob(RankBy::Expected) - 2.5).abs() < 1e-9); // 5·(1−3/6)
    }

    #[test]
    fn agg_sum_vs_max_diverge_with_multiple_children() {
        // T0 has two dependents T1(v2) T2(v4); max=4, sum=6 → forward differs.
        let s = Store {
            tasks: vec![
                Task {
                    id: "T0".into(),
                    status: Status::Todo,
                    value: 0,
                    ..Default::default()
                },
                Task {
                    id: "T1".into(),
                    status: Status::Todo,
                    value: 2,
                    deps: vec!["T0".into()],
                    ..Default::default()
                },
                Task {
                    id: "T2".into(),
                    status: Status::Todo,
                    value: 4,
                    deps: vec!["T0".into()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let pmax = WorthParams {
            agg: Agg::Max,
            ..Default::default()
        };
        let psum = WorthParams {
            agg: Agg::Sum,
            ..Default::default()
        };
        let max = worth_map_with(&s, &pmax)["T0"]; // 0.85·max(4,2)=3.4
        let sum = worth_map_with(&s, &psum)["T0"]; // 0.85·(4+2)=5.1
        assert!(sum > max);
    }
}
