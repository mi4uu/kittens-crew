//! T81 — intervention ladder.
//!
//! Maps a behavioural-health score + iteration counts to an [`Intervention`]
//! level.  Pure, deterministic, offline.  Takes only primitives and its own
//! structs — no sibling driver imports, no IO, no allocator beyond std.
//!
//! ## Health axis
//! Default cutoffs are `-1.0` (SoftWarn), `-2.0` (HardExplain), `-3.0`
//! (Terminate).  They are scaled *toward zero* by task weight so that a
//! heavy / high-blast task triggers sooner.
//!
//! ### Weight formula
//! ```text
//! weight_norm  = (value + risk + difficulty + blast_radius) / NORM_DENOM
//! scale_factor = 1.0 / (1.0 + K_WEIGHT * weight_norm)   // ∈ (0, 1]
//! effective_N  = base_N * scale_factor
//! ```
//! `NORM_DENOM = 18.0` (max of 5+5+5+3 = 18 — blast capped at 3 for the
//! normalisation so a giant blast_radius can't trivially make thresholds ≈ 0).
//! `K_WEIGHT = 2.0` — empirically doubles the tightening at max weight.
//! All constants are documented; swap them in one place.
//!
//! ## Iteration axis
//! - `iters >= soft_iter`: at least SoftWarn.
//! - `iters >= hard_iter`: Terminate.
//! Research grace (`is_research = true`): add `RESEARCH_GRACE` extra turns to
//! both limits, letting the agent do something useful with gathered data.
//!
//! ## Combination
//! [`assess`] returns the **most severe** of the health-derived and the
//! iteration-derived level.

/// How urgently the orchestrator should intervene in agent behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Intervention {
    /// Agent looks fine — let it run.
    None,
    /// Warn the agent and ask "do you know what you are doing?"; continue.
    SoftWarn,
    /// Interrupt and demand a written explanation before the next turn.
    HardExplain,
    /// Deal-breaker: stop the agent unconditionally.
    Terminate,
}

// ── constants ──────────────────────────────────────────────────────────────

/// Default health threshold for SoftWarn.
const SOFT_WARN_BASE: f64 = -1.0;
/// Default health threshold for HardExplain.
const HARD_EXPLAIN_BASE: f64 = -2.0;
/// Default health threshold for Terminate.
const TERMINATE_BASE: f64 = -3.0;

/// Denominator used to normalise raw weight into [0, 1].
/// Max raw = 5+5+5 + min(blast, BLAST_CAP=3) = 18.
const NORM_DENOM: f64 = 18.0;
/// Cap applied to `blast_radius` before normalisation, so a pathologically
/// large blast cannot collapse thresholds to near-zero.
const BLAST_CAP: usize = 3;
/// Scaling coefficient: at max weight the thresholds shrink by 1/(1+K_WEIGHT) ≈ 0.33.
const K_WEIGHT: f64 = 2.0;

/// Extra turns granted on both iteration limits for research tasks.
const RESEARCH_GRACE: u32 = 5;

// ── public structs ─────────────────────────────────────────────────────────

/// Task weight parameters.  All values are ~0..5 for the i64 fields;
/// `blast_radius` is the number of downstream tasks blocked by this one.
#[derive(Debug, Clone, Copy)]
pub struct Weight {
    pub value: i64,
    pub risk: i64,
    pub difficulty: i64,
    pub blast_radius: usize,
}

/// Iteration budget for one task execution.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// At or above this iteration count → at least SoftWarn.
    pub soft_iter: u32,
    /// At or above this iteration count → Terminate (no explanation helps).
    pub hard_iter: u32,
    /// Research / non-blocking work: grant extra turns on both limits.
    pub is_research: bool,
}

// ── core logic ─────────────────────────────────────────────────────────────

/// Compute the `scale_factor` that tightens health thresholds for heavy tasks.
///
/// Returns a value in `(0, 1]`.  A zero-weight task returns 1.0 (no change);
/// a maximum-weight task returns 1/(1+K_WEIGHT) ≈ 0.33.
fn scale_factor(w: &Weight) -> f64 {
    let blast_capped = w.blast_radius.min(BLAST_CAP) as f64;
    let raw = w.value.max(0) as f64 + w.risk.max(0) as f64 + w.difficulty.max(0) as f64 + blast_capped;
    let norm = raw / NORM_DENOM; // ∈ [0, 1]
    1.0 / (1.0 + K_WEIGHT * norm)
}

/// Derive the intervention level from the health score alone.
fn health_level(health: f64, w: &Weight) -> Intervention {
    let sf = scale_factor(w);
    // Multiply base thresholds (all negative) by sf → they move toward 0 (tighter).
    let soft_thr = SOFT_WARN_BASE * sf;
    let hard_thr = HARD_EXPLAIN_BASE * sf;
    let term_thr = TERMINATE_BASE * sf;

    if health <= term_thr {
        Intervention::Terminate
    } else if health < hard_thr {
        Intervention::HardExplain
    } else if health < soft_thr {
        Intervention::SoftWarn
    } else {
        Intervention::None
    }
}

/// Derive the intervention level from the iteration count alone.
fn iter_level(iters: u32, lim: &Limits) -> Intervention {
    let grace = if lim.is_research { RESEARCH_GRACE } else { 0 };
    let soft = lim.soft_iter.saturating_add(grace);
    let hard = lim.hard_iter.saturating_add(grace);

    if iters >= hard {
        Intervention::Terminate
    } else if iters >= soft {
        Intervention::SoftWarn
    } else {
        Intervention::None
    }
}

/// Assess the required intervention level.
///
/// Returns the **most severe** of the health-derived and iteration-derived
/// levels.  Both axes are independent; the combination is a simple max.
pub fn assess(health: f64, w: &Weight, iters: u32, lim: &Limits) -> Intervention {
    health_level(health, w).max(iter_level(iters, lim))
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn light() -> Weight {
        Weight { value: 0, risk: 0, difficulty: 0, blast_radius: 0 }
    }

    fn baseline() -> Weight {
        // "baseline" — zero weight so default thresholds apply unscaled.
        Weight { value: 0, risk: 0, difficulty: 0, blast_radius: 0 }
    }

    fn heavy() -> Weight {
        Weight { value: 5, risk: 5, difficulty: 5, blast_radius: 3 }
    }

    fn normal_limits() -> Limits {
        Limits { soft_iter: 10, hard_iter: 20, is_research: false }
    }

    fn research_limits() -> Limits {
        Limits { soft_iter: 10, hard_iter: 20, is_research: true }
    }

    // ── health axis ────────────────────────────────────────────────────────

    /// Healthy score on a light task → None.
    #[test]
    fn healthy_light_none() {
        let iv = assess(-0.5, &light(), 0, &normal_limits());
        assert_eq!(iv, Intervention::None);
    }

    /// -1.5 on baseline weight → SoftWarn (between default -1 and -2).
    #[test]
    fn health_soft_warn_baseline() {
        let iv = assess(-1.5, &baseline(), 0, &normal_limits());
        assert_eq!(iv, Intervention::SoftWarn);
    }

    /// -2.5 on baseline weight → HardExplain.
    #[test]
    fn health_hard_explain_baseline() {
        let iv = assess(-2.5, &baseline(), 0, &normal_limits());
        assert_eq!(iv, Intervention::HardExplain);
    }

    /// -3.0 on baseline weight → Terminate.
    #[test]
    fn health_terminate_baseline() {
        let iv = assess(-3.0, &baseline(), 0, &normal_limits());
        assert_eq!(iv, Intervention::Terminate);
    }

    // ── weight scaling ─────────────────────────────────────────────────────

    /// Same health score is more severe on heavy task than on light task.
    #[test]
    fn weight_scaling_heavy_triggers_sooner() {
        // -0.8 is above the default -1.0 SoftWarn threshold → None on light.
        // On a heavy task the threshold shifts toward 0, so -0.8 triggers SoftWarn.
        let light_iv = assess(-0.8, &light(), 0, &normal_limits());
        let heavy_iv = assess(-0.8, &heavy(), 0, &normal_limits());
        assert_eq!(light_iv, Intervention::None, "light task should be None at -0.8");
        assert!(heavy_iv > light_iv, "heavy task must be more severe: {heavy_iv:?} vs {light_iv:?}");
    }

    // ── iteration axis ─────────────────────────────────────────────────────

    /// iters >= hard_iter → Terminate even with healthy score.
    #[test]
    fn iter_hard_terminates() {
        let iv = assess(0.0, &light(), 20, &normal_limits());
        assert_eq!(iv, Intervention::Terminate);
    }

    /// iters >= soft_iter → at least SoftWarn (even with healthy score).
    #[test]
    fn iter_soft_warns() {
        let iv = assess(0.0, &light(), 10, &normal_limits());
        assert_eq!(iv, Intervention::SoftWarn);
    }

    /// Below both iter limits, healthy score → None.
    #[test]
    fn iter_below_limits_none() {
        let iv = assess(0.0, &light(), 9, &normal_limits());
        assert_eq!(iv, Intervention::None);
    }

    // ── research grace ─────────────────────────────────────────────────────

    /// is_research=true pushes the hard limit out by RESEARCH_GRACE turns —
    /// a run that would Terminate without research is downgraded.
    #[test]
    fn research_grace_downgrades_terminate() {
        // iters = hard_iter exactly → Terminate without grace.
        let no_grace = assess(0.0, &light(), 20, &normal_limits());
        // Same iters, but is_research grants +5 → now below the effective hard limit.
        let with_grace = assess(0.0, &light(), 20, &research_limits());
        assert_eq!(no_grace, Intervention::Terminate, "no grace must Terminate at hard limit");
        assert!(with_grace < no_grace, "research grace must downgrade: {with_grace:?}");
    }

    /// Research grace also extends the soft limit.
    #[test]
    fn research_grace_extends_soft_limit() {
        // iters = soft_iter → SoftWarn without grace.
        let no_grace = assess(0.0, &light(), 10, &normal_limits());
        let with_grace = assess(0.0, &light(), 10, &research_limits());
        assert_eq!(no_grace, Intervention::SoftWarn);
        assert_eq!(with_grace, Intervention::None, "research grace must defer soft warn");
    }

    // ── combination ────────────────────────────────────────────────────────

    /// Most-severe wins: HardExplain from health + SoftWarn from iters → HardExplain.
    #[test]
    fn most_severe_combined() {
        // health -2.5 on baseline → HardExplain; iters 10 → SoftWarn.
        let iv = assess(-2.5, &baseline(), 10, &normal_limits());
        assert_eq!(iv, Intervention::HardExplain, "HardExplain must win over SoftWarn");
    }

    /// Most-severe wins: Terminate from iters beats HardExplain from health.
    #[test]
    fn iter_terminate_beats_health_hard_explain() {
        let iv = assess(-2.5, &baseline(), 20, &normal_limits());
        assert_eq!(iv, Intervention::Terminate, "iter Terminate must win");
    }
}
