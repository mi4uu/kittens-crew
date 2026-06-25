//! T76 — simulated AI for the orchestration bench. The whole point: take the MODEL
//! out entirely, so the bench measures ONLY the orchestration layer (plan management
//! + task dispatch + verify + replan) deterministically — no network, no model
//! variance, fully repeatable. `SimDriver` implements the SAME `Driver` seam the real
//! backends do, but returns scripted, compiling (or deliberately-broken) stubs.
//!
//! A scenario scripts per-node behaviour by a substring of the node's task text:
//! always-OK, or fail-the-first-k-times-then-OK (to exercise bounded replan, T74).
//! The orchestrator runs for real; we compare the ACTUAL advance order + cost to the
//! scenario's ground-truth. Passivity ("waited for the model / the user") cannot hide
//! here — the sim AI never volunteers anything, so any progress is the harness driving.

use super::api::{Driver, DriverError, Turn, TurnResult};
use std::collections::HashMap;
use std::sync::Mutex;

/// How the simulated AI answers for a node (matched by a substring of its task text).
#[derive(Clone)]
pub enum SimBehavior {
    /// Always returns compiling code.
    Ok,
    /// Returns broken code for the first `n` calls, then compiling — exercises replan.
    FailThenOk(u32),
}

/// Compiling stub (any node accepts it; distinct files so crates don't clash).
const GOOD: &str = "```rust\npub fn run() -> i64 { 0 }\n```";
/// Deliberately broken (missing brace) — fails the rustc verify gate.
const BAD: &str = "```rust\npub fn run() -> i64 { 0 \n```";

pub struct SimDriver {
    script: Vec<(String, SimBehavior)>,
    seen: Mutex<HashMap<String, u32>>,
}

impl Default for SimDriver {
    fn default() -> Self {
        Self {
            script: Vec::new(),
            seen: Mutex::new(HashMap::new()),
        }
    }
}

impl SimDriver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Script a behaviour for any node whose task text contains `needle`.
    pub fn on(mut self, needle: &str, b: SimBehavior) -> Self {
        self.script.push((needle.to_string(), b));
        self
    }

    /// Total dispatches the sim served (a cost signal: extra calls = replan churn).
    pub fn calls(&self) -> u32 {
        self.seen.lock().unwrap().values().sum()
    }
}

impl Driver for SimDriver {
    fn dispatch(&self, turn: &Turn) -> Result<TurnResult, DriverError> {
        let (key, beh) = self
            .script
            .iter()
            .find(|(n, _)| turn.prompt.contains(n.as_str()))
            .map(|(n, b)| (n.clone(), b.clone()))
            .unwrap_or_else(|| ("__default__".to_string(), SimBehavior::Ok));
        let mut seen = self.seen.lock().unwrap();
        let count = seen.entry(key).or_insert(0);
        let text = match beh {
            SimBehavior::Ok => GOOD,
            SimBehavior::FailThenOk(n) if *count < n => BAD,
            SimBehavior::FailThenOk(_) => GOOD,
        };
        *count += 1;
        Ok(TurnResult {
            text: text.to_string(),
            model: "sim".to_string(),
        })
    }

    fn model(&self) -> &str {
        "sim"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::drive::{drive, DriveOpts, Outcome};

    /// Build a temp store; each task's scope is an absolute path in the temp dir.
    fn scenario(dir: &str, spec: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ks_sim_{}_{}", std::process::id(), dir));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("spec.toml");
        std::fs::write(&path, spec.replace("{DIR}", d.to_str().unwrap())).unwrap();
        path
    }

    /// Scenario #1 — INITIATIVE + ORDER. The sim AI volunteers nothing; the harness
    /// must drive a dependency chain on its own, in the right order, to convergence.
    /// Zero model calls, fully deterministic.
    #[test]
    fn drives_dependency_chain_in_order_no_model() {
        let store = scenario(
            "order",
            "schema = 1\n\
             [[task]]\nid=\"T1\"\nstatus=\"todo\"\ntask=\"alpha leaf\"\ndeps=[]\npriority=1\nscope=[\"{DIR}/a.rs\"]\n\
             [[task]]\nid=\"T2\"\nstatus=\"todo\"\ntask=\"beta leaf\"\ndeps=[\"T1\"]\npriority=1\nscope=[\"{DIR}/b.rs\"]\n\
             [[task]]\nid=\"T3\"\nstatus=\"todo\"\ntask=\"gamma leaf\"\ndeps=[\"T2\"]\npriority=1\nscope=[\"{DIR}/c.rs\"]\n",
        );
        let sim = SimDriver::new();
        let mut order = Vec::new();
        let out = drive(
            &sim,
            &DriveOpts { max_iters: 10, max_retries: 0, store_path: store, workspace_root: None },
            |id, _| order.push(id.to_string()),
        )
        .unwrap();
        assert!(matches!(out, Outcome::Converged { done: 3 }), "got {out:?}");
        assert_eq!(order, vec!["T1", "T2", "T3"], "must drive in dependency order");
        assert_eq!(sim.calls(), 3, "no replan churn on clean nodes");
    }

    /// Scenario #2 — REPLAN, offline. A node fails verify once; the orchestrator must
    /// recover it on its own (no nudge) and still converge. Cost = one extra call.
    #[test]
    fn recovers_failing_node_without_nudge_no_model() {
        let store = scenario(
            "replan",
            "schema = 1\n\
             [[task]]\nid=\"T1\"\nstatus=\"todo\"\ntask=\"flaky leaf\"\ndeps=[]\npriority=1\nscope=[\"{DIR}/x.rs\"]\n",
        );
        let sim = SimDriver::new().on("flaky", SimBehavior::FailThenOk(1));
        let out = drive(
            &sim,
            &DriveOpts { max_iters: 5, max_retries: 2, store_path: store, workspace_root: None },
            |_, _| {},
        )
        .unwrap();
        assert!(matches!(out, Outcome::Converged { done: 1 }), "got {out:?}");
        assert_eq!(sim.calls(), 2, "one failed attempt + one recovery = cost of 2");
    }
}
