//! End-to-end simulation of the action-health layer (T79-T83) composed together.
//!
//! The five modules were built in isolation with primitive interfaces; this is the
//! glue that drives them as ONE pipeline over a scripted action stream — and the
//! working prototype of the T84 in-loop integration (so it is `pub(crate)`, reused
//! there rather than thrown away):
//!
//!   action → karma.record (health) → cycle.record (loop shape) → intervene.assess
//!          → (on Terminate) response.choose → evaldump capture
//!
//! No model, no network, fully deterministic — a misbehaving ("zombie") agent's
//! action stream is replayed and the whole ladder is observed end to end.

use super::evaldump::{dump, Transcript};
use super::intervene::{assess, Intervention, Limits, Weight};
use super::karma::Karma;
use super::cycle::CycleDetector;
use super::response::{choose, Level, Policy, Strategy};

/// What the pipeline observed at one action.
#[derive(Debug, Clone)]
pub(crate) struct StepReport {
    pub action: String,
    pub ok: bool,
    pub health: f64,
    /// Period of a detected sequence-cycle on this action, if any (T80).
    pub cycle: Option<usize>,
    pub intervention: Intervention,
}

/// The full run: per-step trace, where (if) it terminated, the chosen response
/// strategy, and the captured transcript bytes (T83) ready for offline eval.
#[derive(Debug)]
pub(crate) struct SimReport {
    pub steps: Vec<StepReport>,
    pub terminated_at: Option<usize>,
    pub strategy: Option<Strategy>,
    pub dump: Vec<u8>,
}

/// Map the intervention level to the actionable response level (the T84 translation).
fn level_of(i: Intervention) -> Option<Level> {
    match i {
        Intervention::None => None,
        Intervention::SoftWarn => Some(Level::Soft),
        Intervention::HardExplain => Some(Level::Hard),
        Intervention::Terminate => Some(Level::DealBreaker),
    }
}

/// Drive the action-health pipeline over `actions` (each: signature + did-it-succeed)
/// for one agent working a task of the given `weight` under `limits`, choosing
/// responses per `policy`. Stops driving the instant an action warrants Terminate,
/// then captures the transcript. This is exactly the per-action work T84 will inline
/// into `drive()`.
pub(crate) fn simulate(
    actions: &[(&str, bool)],
    weight: &Weight,
    limits: &Limits,
    policy: &Policy,
) -> SimReport {
    let mut karma = Karma::new();
    let mut cyc = CycleDetector::new(4, 2);
    let mut transcript = Transcript::new("builder-kitty", "behavioural-health simulation");
    let mut steps = Vec::new();
    let mut terminated_at = None;
    let mut strategy = None;

    for (i, (action, ok)) in actions.iter().enumerate() {
        karma.record(action, *ok);
        let cycle = cyc.record(action);
        transcript.push(*action, if *ok { "ok" } else { "error" }, *ok);

        let intervention = assess(karma.health(), weight, (i + 1) as u32, limits);
        steps.push(StepReport {
            action: (*action).to_string(),
            ok: *ok,
            health: karma.health(),
            cycle,
            intervention,
        });

        // A soft/hard intervention would, in the real loop, trigger its response
        // strategy too; here we only act on the terminal one and stop driving.
        if intervention == Intervention::Terminate {
            terminated_at = Some(i);
            strategy = level_of(intervention).map(|l| choose(l, policy));
            break;
        }
    }

    // Zombie/terminate → capture the action transcript for offline lessons (T83).
    let mut buf = Vec::new();
    let _ = dump(&transcript, &mut buf);

    SimReport {
        steps,
        terminated_at,
        strategy,
        dump: buf,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole layer, end to end: a healthy opening, then a doom spiral of an
    /// identical failing action drives karma down, the ladder escalates
    /// None → SoftWarn → HardExplain → Terminate, the cycle guard fires on the
    /// repeat run, the deal-breaker selects the default HaltReplan response, and the
    /// transcript is captured + round-trips for offline evaluation.
    #[test]
    fn zombie_agent_runs_the_full_health_pipeline() {
        let weight = Weight { value: 2, risk: 2, difficulty: 2, blast_radius: 2 };
        // Generous iteration caps so the HEALTH axis (not the iteration axis) drives.
        let limits = Limits { soft_iter: 100, hard_iter: 200, is_research: false };
        let policy = Policy::default();

        let actions = vec![
            ("compile core", true),
            ("run tests", true),
            ("lint", true),
            ("read missing config", false),
            ("read missing config", false),
            ("read missing config", false),
            ("read missing config", false),
            ("read missing config", false),
            ("read missing config", false),
        ];

        let r = simulate(&actions, &weight, &limits, &policy);

        // It terminated, and not during the healthy opening.
        let term = r.terminated_at.expect("the doom spiral must terminate");
        assert!(term >= 3, "healthy phase must not terminate (term={term})");

        let kinds: Vec<Intervention> = r.steps.iter().map(|s| s.intervention).collect();

        // Healthy start was clean.
        assert!(kinds.iter().any(|k| *k == Intervention::None), "no None seen: {kinds:?}");

        // Escalation passes through soft THEN hard THEN terminate, in order.
        let soft = kinds.iter().position(|k| *k == Intervention::SoftWarn).expect("a SoftWarn");
        let hard = kinds.iter().position(|k| *k == Intervention::HardExplain).expect("a HardExplain");
        let tpos = kinds.iter().position(|k| *k == Intervention::Terminate).unwrap();
        assert!(soft < hard && hard < tpos, "escalation order wrong: {kinds:?}");

        // Across the doom phase the severity is monotonic non-decreasing (health only drops).
        for w in kinds[3..=tpos].windows(2) {
            assert!(w[1] >= w[0], "severity went backwards in the doom phase: {kinds:?}");
        }

        // The identical-repeat run is a loop the cycle guard reports (period 1).
        assert!(
            r.steps.iter().any(|s| s.cycle.is_some()),
            "cycle detector should fire on the repeated action"
        );

        // Deal-breaker → default response strategy.
        assert_eq!(r.strategy, Some(Strategy::HaltReplan));

        // Transcript captured for offline eval, and it round-trips losslessly.
        assert!(!r.dump.is_empty(), "transcript must be captured on terminate");
        let line = String::from_utf8(r.dump.clone()).unwrap();
        let back: Transcript = serde_json::from_str(line.trim()).expect("transcript round-trips");
        assert_eq!(back.records.len(), term + 1, "one record per driven action");
        assert!(!back.records[term].ok, "the terminating action was a failure");
    }

    /// A heavy/blocking task crosses the ladder SOONER than a light one on the same
    /// stream — tighter thresholds = earlier intervention (the weight-scaling rule).
    #[test]
    fn heavier_task_terminates_no_later_than_a_light_one() {
        let limits = Limits { soft_iter: 100, hard_iter: 200, is_research: false };
        let policy = Policy::default();
        let actions: Vec<(&str, bool)> = std::iter::repeat(("retry broken build", false)).take(12).collect();

        let light = Weight { value: 0, risk: 0, difficulty: 0, blast_radius: 0 };
        let heavy = Weight { value: 5, risk: 5, difficulty: 5, blast_radius: 8 };

        let lr = simulate(&actions, &light, &limits, &policy).terminated_at.expect("light terminates");
        let hr = simulate(&actions, &heavy, &limits, &policy).terminated_at.expect("heavy terminates");
        assert!(hr <= lr, "heavy task should terminate no later than light (heavy={hr}, light={lr})");
    }

    /// The sequence-cycle (A→B→A→B) the doom-loop's identical-consecutive guard
    /// MISSES is caught here — the "agent walking in circles" case.
    #[test]
    fn sequence_cycle_is_caught_not_just_identical_repeats() {
        let mut cyc = CycleDetector::new(4, 2);
        let mut detected = None;
        for a in ["read A", "write B", "read A", "write B", "read A", "write B"] {
            if let Some(p) = cyc.record(a) {
                detected = Some(p);
            }
        }
        assert_eq!(detected, Some(2), "A→B→A→B is a period-2 cycle");
    }
}
