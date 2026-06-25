//! T72 — deterministic scope/cost estimator.
//!
//! Pure, offline, no LLM. Derives per-node estimates from the store/DAG:
//!
//! - `scope_files`  — number of entries in `task.scope` (proxy for code surface).
//! - `fan_in`       — direct dependency count (`task.deps.len()`).
//! - `fan_out`      — transitive dependent count (`impact.blocks.len()`).
//! - `cpath_depth`  — length of the longest prerequisite chain leading to this node
//!                    (via `plan::critical_path(store, Some(id))`).
//! - `complexity`   — heuristic combining `difficulty`, `risk`, `value`, and scope.
//! - `token_cost`   — rough deterministic budget in tokens.
//!
//! Feeds: parallelization ready-set (T62), rough budget (T70), model right-sizing (T73).

use crate::plan;
use crate::store::Store;

// ── public types ─────────────────────────────────────────────────────────────

/// Per-node deterministic estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    /// Task id.
    pub node: String,
    /// Number of scope files (proxy for LOC surface; cloc upgrade path).
    pub scope_files: usize,
    /// Direct dep count — how much this node waits on.
    pub fan_in: usize,
    /// Transitive dependent count — how much waits on this node.
    pub fan_out: usize,
    /// Critical-path depth: length of longest prerequisite chain ending here
    /// (1 = root, N = leaf at depth N).
    pub cpath_depth: usize,
    /// Complexity heuristic ∈ [0, ∞). Combines difficulty, risk, value, scope.
    pub complexity: f64,
    /// Rough token budget estimate (deterministic).
    pub token_cost: u64,
}

// ── public API ────────────────────────────────────────────────────────────────

/// Compute a deterministic [`Estimate`] for `id` from the store and DAG.
///
/// Returns a zero-filled estimate if `id` is unknown (never panics).
pub fn estimate(store: &Store, id: &str) -> Estimate {
    let task = match store.task(id) {
        Some(t) => t,
        None => {
            return Estimate {
                node: id.to_string(),
                scope_files: 0,
                fan_in: 0,
                fan_out: 0,
                cpath_depth: 0,
                complexity: 0.0,
                token_cost: 0,
            }
        }
    };

    // ── graph metrics ────────────────────────────────────────────────────────

    let scope_files = task.scope.len();

    // fan_in: direct deps present in the store (dangling refs excluded).
    let fan_in = task
        .deps
        .iter()
        .filter(|d| store.task(d).is_some())
        .count();

    // fan_out: transitive dependents via plan::impact.
    let fan_out = plan::impact(store, id).blocks.len();

    // critical-path depth = chain length (includes `id` itself).
    let cpath_depth = plan::critical_path(store, Some(id)).len().max(1);

    // ── complexity heuristic ──────────────────────────────────────────────────
    //
    // Base from stored fields (0-5 scale; 0 = unscored treated as 1).
    // difficulty  — effort/rework weight (positively contributes).
    // risk        — chance of surprise (positively contributes).
    // value       — scope breadth proxy (higher value = bigger surface, + weight).
    // scope_files — additive LOC-surface proxy.
    //
    // Formula keeps all terms in [0,1] before summing so the result is
    // interpretable and easy to tune later.
    let diff = task.difficulty.max(1) as f64 / 5.0;      // [0.2, 1.0]
    let risk = task.risk.max(0) as f64 / 5.0;             // [0.0, 1.0]
    let val  = task.value.max(0) as f64 / 5.0;            // [0.0, 1.0]
    let scope_term = (scope_files as f64).ln_1p() / 5.0;  // log-compressed, soft cap
    let graph_term = (fan_in as f64 + fan_out as f64).ln_1p() / 10.0;
    let depth_term = (cpath_depth as f64).ln_1p() / 5.0;

    let complexity =
        2.0 * diff          // effort dominates
        + 1.5 * risk        // risk second
        + 0.5 * val         // value as breadth signal
        + 1.0 * scope_term  // scope surface
        + 0.5 * graph_term  // graph centrality
        + 0.5 * depth_term; // depth pressure

    // ── token cost ────────────────────────────────────────────────────────────
    //
    // Rough linear map: base of 500 tokens per node, scaled by complexity.
    // Multiply by scope file count (each file ≈ 400 tokens to read/write).
    // Floor at 200 tokens (minimal task), ceiling not imposed (let it grow).
    let base: u64 = 500;
    let scope_tokens: u64 = (scope_files as u64).saturating_mul(400);
    let complexity_scale = (1.0 + complexity).max(1.0);
    let token_cost = ((base + scope_tokens) as f64 * complexity_scale).round() as u64;
    let token_cost = token_cost.max(200);

    Estimate {
        node: id.to_string(),
        scope_files,
        fan_in,
        fan_out,
        cpath_depth,
        complexity,
        token_cost,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Write a temp spec.toml and return the path (mirrors delegation.rs pattern).
    fn store_at(tag: &str, spec: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("ks_est_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("spec.toml");
        std::fs::write(&p, spec).unwrap();
        p
    }

    /// Build the canonical 4-node test store:
    ///
    /// ```
    ///  T1 ──► T3 ──► T4
    ///  T2 ──►/
    /// ```
    /// T4 is a leaf that depends on T3, which depends on T1 and T2.
    /// T_leaf is an independent node with no deps and no dependents.
    fn make_store(tag: &str) -> (Store, PathBuf) {
        let spec = "\
schema=1
[[task]]
id=\"T1\"\nstatus=\"todo\"\ntask=\"root A\"\ndeps=[]\npriority=1\ndifficulty=2\nrisk=1\nvalue=3\nscope=[\"a.rs\"]
[[task]]
id=\"T2\"\nstatus=\"todo\"\ntask=\"root B\"\ndeps=[]\npriority=1\ndifficulty=3\nrisk=2\nvalue=2\nscope=[\"b.rs\",\"c.rs\"]
[[task]]
id=\"T3\"\nstatus=\"todo\"\ntask=\"mid\"\ndeps=[\"T1\",\"T2\"]\npriority=2\ndifficulty=4\nrisk=3\nvalue=4\nscope=[\"d.rs\"]
[[task]]
id=\"T4\"\nstatus=\"todo\"\ntask=\"leaf\"\ndeps=[\"T3\"]\npriority=3\ndifficulty=5\nrisk=4\nvalue=5\nscope=[\"e.rs\",\"f.rs\",\"g.rs\"]
";
        let p = store_at(tag, spec);
        let store = Store::load(&p).unwrap();
        (store, p)
    }

    /// Nodes with more direct deps have higher fan_in.
    #[test]
    fn fan_in_tracks_deps() {
        let (store, _) = make_store("fan_in");
        let e_t1 = estimate(&store, "T1");
        let e_t3 = estimate(&store, "T3");

        assert_eq!(e_t1.fan_in, 0, "T1 has no deps");
        assert_eq!(e_t3.fan_in, 2, "T3 depends on T1 and T2");
        assert!(
            e_t3.fan_in > e_t1.fan_in,
            "T3 fan_in ({}) should exceed T1 fan_in ({})",
            e_t3.fan_in, e_t1.fan_in
        );
    }

    /// T1 and T2 are depended on by T3 (and transitively T4); T4 has no dependents.
    #[test]
    fn fan_out_tracks_transitive_dependents() {
        let (store, _) = make_store("fan_out");
        let e_t1 = estimate(&store, "T1");
        let e_t4 = estimate(&store, "T4");

        // T1 is depended on transitively by T3 and T4 → fan_out = 2.
        assert_eq!(e_t1.fan_out, 2, "T1 blocks T3 and T4 transitively");
        // T4 is a leaf — nothing depends on it.
        assert_eq!(e_t4.fan_out, 0, "T4 has no dependents");
        assert!(
            e_t1.fan_out > e_t4.fan_out,
            "T1 fan_out ({}) should exceed T4 fan_out ({})",
            e_t1.fan_out, e_t4.fan_out
        );
    }

    /// Higher difficulty and risk yield higher complexity and token_cost.
    #[test]
    fn difficulty_and_risk_drive_complexity() {
        let (store, _) = make_store("complexity");
        let e_t1 = estimate(&store, "T1"); // difficulty=2, risk=1
        let e_t4 = estimate(&store, "T4"); // difficulty=5, risk=4

        assert!(
            e_t4.complexity > e_t1.complexity,
            "T4 complexity ({:.3}) should exceed T1 ({:.3})",
            e_t4.complexity, e_t1.complexity
        );
        assert!(
            e_t4.token_cost > e_t1.token_cost,
            "T4 token_cost ({}) should exceed T1 ({})",
            e_t4.token_cost, e_t1.token_cost
        );
    }

    /// More scope files increase the estimate.
    #[test]
    fn scope_count_drives_estimate() {
        let (store, _) = make_store("scope");
        let e_t1 = estimate(&store, "T1"); // scope=["a.rs"]          → 1 file
        let e_t4 = estimate(&store, "T4"); // scope=["e.rs","f.rs","g.rs"] → 3 files

        assert_eq!(e_t1.scope_files, 1);
        assert_eq!(e_t4.scope_files, 3);
        // complexity and token_cost must reflect the wider scope.
        assert!(
            e_t4.complexity > e_t1.complexity,
            "wider scope must raise complexity"
        );
        assert!(
            e_t4.token_cost > e_t1.token_cost,
            "wider scope must raise token_cost"
        );
    }

    /// Critical-path depth: T1 (root) = 1; T4 (depth 3: T1→T3→T4) = 3.
    #[test]
    fn cpath_depth_increases_toward_leaf() {
        let (store, _) = make_store("cpath");
        let e_t1 = estimate(&store, "T1");
        let e_t4 = estimate(&store, "T4");

        assert_eq!(e_t1.cpath_depth, 1, "T1 is a root — chain length 1");
        assert_eq!(e_t4.cpath_depth, 3, "T4 chain: T1→T3→T4 = depth 3");
        assert!(
            e_t4.cpath_depth > e_t1.cpath_depth,
            "T4 must be deeper than T1"
        );
    }

    /// Same store, same id → bit-for-bit identical result (determinism).
    #[test]
    fn estimate_is_deterministic() {
        let (store, _) = make_store("det");
        for id in &["T1", "T2", "T3", "T4"] {
            let a = estimate(&store, id);
            let b = estimate(&store, id);
            assert_eq!(a, b, "estimate({id}) must be deterministic");
        }
    }

    /// Unknown id returns a zero estimate without panicking.
    #[test]
    fn unknown_id_returns_zero() {
        let (store, _) = make_store("unknown");
        let e = estimate(&store, "T999");
        assert_eq!(e.scope_files, 0);
        assert_eq!(e.fan_in, 0);
        assert_eq!(e.fan_out, 0);
        assert_eq!(e.token_cost, 0);
    }
}
