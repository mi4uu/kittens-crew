//! T75 — A/B benchmark: the only honest measure of the harness's WEIGHT. Same
//! model, same per-node prompts; the ONLY difference is the harness layer
//! (deterministic verify + bounded replan). Arm "bare" fills each leaf once and
//! nobody checks (the model-picks-and-hopes baseline). Arm "kittenscrew" drives
//! with verify + replan. Each runs k times; we report how often every node ends
//! green (full-convergence rate, ~pass^k) and the mean nodes-green. The DELTA is
//! what the harness is worth — without this an A/B "it built a toy spec" proves
//! nothing.

use super::api::{Driver, Turn};
use super::drive::{drive, extract_code, scoped_prompt, DriveOpts};
use super::verify;
use crate::store::{Status, Store};
use std::path::{Path, PathBuf};

pub struct BenchOpts {
    pub store_path: PathBuf,
    pub k: u32,
    pub max_iters: u32,
    pub max_retries: u32,
}

/// One arm's results across k trials (each entry = how many nodes ended green).
pub struct ArmScore {
    pub trials: Vec<usize>,
    pub nodes: usize,
}

impl ArmScore {
    /// First trial fully green (~pass^1).
    pub fn pass_1(&self) -> bool {
        self.trials.first().map_or(false, |&g| g == self.nodes)
    }
    /// Fraction of trials where EVERY node ended green (~pass^k as k→trials).
    pub fn full_rate(&self) -> f64 {
        if self.trials.is_empty() {
            return 0.0;
        }
        let full = self.trials.iter().filter(|&&g| g == self.nodes).count();
        full as f64 / self.trials.len() as f64
    }
    pub fn mean_green(&self) -> f64 {
        if self.trials.is_empty() {
            return 0.0;
        }
        self.trials.iter().sum::<usize>() as f64 / self.trials.len() as f64
    }
}

pub struct BenchReport {
    pub bare: ArmScore,
    pub harness: ArmScore,
    pub nodes: usize,
    pub k: u32,
}

pub fn bench(driver: &dyn Driver, opts: &BenchOpts) -> Result<BenchReport, String> {
    let base = Store::load(&opts.store_path).map_err(|e| e.to_string())?;
    let nodes = base
        .tasks
        .iter()
        .filter(|t| t.status != Status::Done)
        .count();
    if nodes == 0 {
        return Err("store has no pending tasks to benchmark".into());
    }
    let mut bare = ArmScore { trials: vec![], nodes };
    let mut harness = ArmScore { trials: vec![], nodes };
    for trial in 0..opts.k {
        bare.trials.push(run_bare(driver, &base, trial)?);
        harness.trials.push(run_harness(driver, &base, trial, opts)?);
    }
    Ok(BenchReport {
        bare,
        harness,
        nodes,
        k: opts.k,
    })
}

/// Fresh isolated workspace for one trial: every scope path rewritten ABSOLUTE
/// into a per-trial temp dir, all tasks reset to Todo. Both arms get the same
/// materialisation so the only difference is the harness layer.
fn materialize(base: &Store, tag: &str, trial: u32) -> Result<(PathBuf, Store), String> {
    let dir =
        std::env::temp_dir().join(format!("ks_bench_{}_{}_{}", std::process::id(), tag, trial));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut s = base.clone();
    for t in &mut s.tasks {
        t.status = Status::Todo;
        t.scope = t
            .scope
            .iter()
            .map(|sc| dir.join(sc).to_string_lossy().into_owned())
            .collect();
        t.eval = None;
    }
    let store_path = dir.join("spec.toml");
    s.save(&store_path).map_err(|e| e.to_string())?;
    Ok((store_path, s))
}

/// Bare baseline: fill each leaf once, no verify, no retry — then score how many
/// happen to compile.
fn run_bare(driver: &dyn Driver, base: &Store, trial: u32) -> Result<usize, String> {
    let (_store_path, s) = materialize(base, "bare", trial)?;
    for t in s.tasks.iter().filter(|t| t.status != Status::Done) {
        let Some(scope) = t.scope.first() else {
            continue;
        };
        let target = Path::new(scope);
        if let Some(p) = target.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if let Ok(res) = driver.dispatch(&Turn {
            prompt: scoped_prompt(&t.task, target),
        }) {
            let _ = std::fs::write(target, extract_code(&res.text));
        }
    }
    Ok(count_green(&s))
}

/// Kittenscrew arm: drive with verify + bounded replan, then score.
fn run_harness(
    driver: &dyn Driver,
    base: &Store,
    trial: u32,
    opts: &BenchOpts,
) -> Result<usize, String> {
    let (store_path, _s) = materialize(base, "harness", trial)?;
    let _ = drive(
        driver,
        &DriveOpts {
            max_iters: opts.max_iters,
            max_retries: opts.max_retries,
            store_path: store_path.clone(),
            // No confinement: the bench materialises scopes in an isolated temp dir it owns.
            workspace_root: None,
        },
        |_, _| {},
    )?;
    let s = Store::load(&store_path).map_err(|e| e.to_string())?;
    Ok(count_green(&s))
}

/// Green = the node's scope file type-checks (same deterministic gate for both arms).
fn count_green(s: &Store) -> usize {
    s.tasks
        .iter()
        .filter(|t| {
            t.scope
                .first()
                .map_or(false, |sc| verify::rustc_compiles(Path::new(sc)).ok)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_score_metrics() {
        let a = ArmScore {
            trials: vec![3, 2, 3],
            nodes: 3,
        };
        assert!(a.pass_1()); // first trial fully green
        assert!((a.full_rate() - 2.0 / 3.0).abs() < 1e-9); // 2 of 3 fully green
        assert!((a.mean_green() - 8.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn empty_arm_is_zero_not_panic() {
        let a = ArmScore {
            trials: vec![],
            nodes: 3,
        };
        assert!(!a.pass_1());
        assert_eq!(a.full_rate(), 0.0);
        assert_eq!(a.mean_green(), 0.0);
    }
}
