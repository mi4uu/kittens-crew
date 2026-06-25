//! T76 — orchestration scenario runner + metric. A scenario is a project (the spec
//! store) + the ground-truth order the orchestrator SHOULD advance nodes in, run
//! against a SimDriver (no model — see sim.rs). We score how the harness MANAGED the
//! work, not the code: did it drive to convergence on its own, in the right order,
//! cheaply — or did it deviate, churn, or — the WORST — STALL: stop doing anything
//! before the work was done ("got bored / waited for the model / waited for a nudge").

use super::api::Driver;
use super::drive::{drive, DriveOpts, Outcome};
use std::path::PathBuf;

pub struct Scenario {
    pub name: String,
    pub store_path: PathBuf,
    /// The order the orchestrator SHOULD advance nodes (ground truth).
    pub expected_order: Vec<String>,
    pub max_iters: u32,
    pub max_retries: u32,
}

pub struct OrchScore {
    pub name: String,
    pub converged: bool,
    /// The WORST failure: stopped before the work was done. = Halted pre-convergence.
    pub stalled: bool,
    /// Burned the iteration budget without converging (spun in place).
    pub spun: bool,
    pub expected_order: Vec<String>,
    pub actual_order: Vec<String>,
    pub order_ok: bool,
    /// Cost: dispatches beyond the ideal (1 per node) = replan/churn delay.
    pub ideal_calls: u32,
    pub actual_calls: u32,
    pub stall_reason: String,
}

impl OrchScore {
    pub fn delay(&self) -> i64 {
        self.actual_calls as i64 - self.ideal_calls as i64
    }
    /// One honest verdict: did the harness manage the work the way we wanted, with no
    /// nudges? A stall is an automatic fail — it's the failure mode we most care about.
    pub fn passed(&self) -> bool {
        self.converged && !self.stalled && self.order_ok
    }
    /// Human-readable one-liner for a bench table.
    pub fn line(&self) -> String {
        let verdict = if self.passed() {
            "OK".to_string()
        } else if self.stalled {
            format!("STALL ({})", self.stall_reason)
        } else if self.spun {
            "SPUN (cap)".to_string()
        } else if !self.order_ok {
            "OUT-OF-ORDER".to_string()
        } else {
            "FAIL".to_string()
        };
        format!(
            "{:<22} {:<16} order={} delay={:+}",
            self.name,
            verdict,
            if self.order_ok { "ok" } else { "WRONG" },
            self.delay()
        )
    }
}

/// Run a scenario against a simulated AI and score the ORCHESTRATION (not the code).
/// `calls` reads the sim's total dispatch count (the cost signal) after the run.
pub fn run<D: Driver>(
    driver: &D,
    sc: &Scenario,
    calls: impl Fn() -> u32,
) -> Result<OrchScore, String> {
    let mut actual_order = Vec::new();
    let out = drive(
        driver,
        &DriveOpts {
            max_iters: sc.max_iters,
            max_retries: sc.max_retries,
            store_path: sc.store_path.clone(),
            workspace_root: None,
        },
        |id, _| actual_order.push(id.to_string()),
    )?;
    let (converged, stalled, spun, stall_reason) = match &out {
        Outcome::Converged { .. } => (true, false, false, String::new()),
        Outcome::Halted { node, reason, .. } => (false, true, false, format!("{node}: {reason}")),
        Outcome::CapReached { .. } => (false, false, true, "iteration cap".to_string()),
    };
    Ok(OrchScore {
        name: sc.name.clone(),
        converged,
        stalled,
        spun,
        order_ok: actual_order == sc.expected_order,
        expected_order: sc.expected_order.clone(),
        actual_order,
        ideal_calls: sc.expected_order.len() as u32,
        actual_calls: calls(),
        stall_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::sim::SimDriver;

    fn store(tag: &str, spec: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ks_scn_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("spec.toml");
        std::fs::write(&p, spec.replace("{DIR}", d.to_str().unwrap())).unwrap();
        p
    }

    /// Happy path: the orchestrator drives the chain in order, on its own, no delay.
    #[test]
    fn clean_project_passes_with_zero_delay() {
        let sp = store(
            "clean",
            "schema=1\n\
             [[task]]\nid=\"T1\"\nstatus=\"todo\"\ntask=\"alpha\"\ndeps=[]\npriority=1\nscope=[\"{DIR}/a.rs\"]\n\
             [[task]]\nid=\"T2\"\nstatus=\"todo\"\ntask=\"beta\"\ndeps=[\"T1\"]\npriority=1\nscope=[\"{DIR}/b.rs\"]\n",
        );
        let sim = SimDriver::new();
        let sc = Scenario {
            name: "clean".into(),
            store_path: sp,
            expected_order: vec!["T1".into(), "T2".into()],
            max_iters: 10,
            max_retries: 0,
        };
        let r = run(&sim, &sc, || sim.calls()).unwrap();
        assert!(r.passed(), "{}", r.line());
        assert_eq!(r.delay(), 0);
    }

    /// The worst case: a node the orchestrator cannot act on (no scope) — it must be
    /// scored as STALL (stopped doing anything), an automatic fail, with a reason.
    #[test]
    fn stall_is_caught_as_the_worst_failure() {
        let sp = store(
            "stall",
            "schema=1\n\
             [[task]]\nid=\"T1\"\nstatus=\"todo\"\ntask=\"no scope leaf\"\ndeps=[]\npriority=1\nscope=[]\n",
        );
        let sim = SimDriver::new();
        let sc = Scenario {
            name: "stall".into(),
            store_path: sp,
            expected_order: vec!["T1".into()],
            max_iters: 5,
            max_retries: 0,
        };
        let r = run(&sim, &sc, || sim.calls()).unwrap();
        assert!(r.stalled, "should detect the stall");
        assert!(!r.passed(), "stall is an automatic fail");
        assert!(r.line().contains("STALL"), "{}", r.line());
    }
}
