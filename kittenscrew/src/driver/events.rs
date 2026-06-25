//! T78 — progress monitoring + event/trigger engine.
//!
//! After each task completes (post-mark-done), `evaluate` scans the store plus a
//! small `EventCtx` (signals the evaluator cannot derive from the store alone) and
//! emits zero or more `Event` values. Pure, deterministic, offline-testable — no
//! network, no IO, no model call.
//!
//! ## Event taxonomy
//! - `Regression`          — a previously-green node went red.
//! - `Blocking`            — task is stalled and has transitive dependents (`blast_radius`).
//! - `SlowTask`            — over-estimate AND no progress (doom-loop signal).
//! - `Stall`               — WORST: always Critical + escalate regardless of severity.
//! - `SignificantChange`   — group fan-in gate triggered.
//! - `UserFeedback`        — repeated correction meta-flag.
//! - `MissingVerify`       — done node, no verify artifact; severity SCALED by value+risk.
//! - `CriterionTampering`  — CATEGORICAL: always Critical, always escalates, mandatory reason.
//!
//! ## Severity rules
//! Numeric band (`severity_band`) maps `[0, ∞)` → Low / Med / High / Critical. BUT:
//! - `Stall`               → always Critical + escalate (overrides band).
//! - `CriterionTampering`  → always Critical + escalate (never downgraded).

use crate::plan;
use crate::store::Store;

// ── Severity ─────────────────────────────────────────────────────────────────

/// Four-level priority for surfacing. Numeric banding is deterministic (V9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Med,
    High,
    Critical,
}

impl Severity {
    /// Short tag used in `Event::line()`.
    pub fn tag(self) -> &'static str {
        match self {
            Severity::Low => "LOW",
            Severity::Med => "MED",
            Severity::High => "HIGH",
            Severity::Critical => "CRIT",
        }
    }
}

/// Map a raw numeric score → severity band (deterministic).
/// Thresholds: <3 → Low, <6 → Med, <12 → High, ≥12 → Critical.
fn severity_band(score: u64) -> Severity {
    match score {
        0..=2 => Severity::Low,
        3..=5 => Severity::Med,
        6..=11 => Severity::High,
        _ => Severity::Critical,
    }
}

// ── EventKind ─────────────────────────────────────────────────────────────────

/// Discriminant for each event type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    /// A previously-green node went red (regression).
    Regression,
    /// Task stalled with transitive dependents; `blast_radius` = count of those dependents.
    Blocking { blast_radius: usize },
    /// Task is over-estimate AND making no progress (doom-loop).
    SlowTask,
    /// Worst failure: harness stopped making progress before work was done.
    Stall,
    /// Group fan-in gate: a batch of tasks all flipped to done.
    SignificantChange,
    /// Model or user feedback; `repeated_correction` = true when same point was corrected >1×.
    UserFeedback { repeated_correction: bool },
    /// Task marked done with no verify artifact. Severity is scaled by value + risk.
    MissingVerify,
    /// Criterion tampering — CATEGORICAL: never silent, always Critical, mandatory explanation.
    CriterionTampering,
}

impl EventKind {
    /// Short label for `line()`.
    pub fn label(&self) -> &'static str {
        match self {
            EventKind::Regression => "REGRESSION",
            EventKind::Blocking { .. } => "BLOCKING",
            EventKind::SlowTask => "SLOW_TASK",
            EventKind::Stall => "STALL",
            EventKind::SignificantChange => "SIG_CHANGE",
            EventKind::UserFeedback { .. } => "USER_FB",
            EventKind::MissingVerify => "NO_VERIFY",
            EventKind::CriterionTampering => "TAMPERING",
        }
    }
}

// ── EventCtx ─────────────────────────────────────────────────────────────────

/// Caller-supplied signals that the evaluator cannot derive from the store alone.
/// Build from whatever your post-done hook knows; unset fields default to empty/false.
#[derive(Debug, Clone, Default)]
pub struct EventCtx {
    /// Task ids that were previously `Done` and are now open again (regression).
    pub regressed: Vec<String>,
    /// True when the harness has stalled (no node made progress this iteration).
    pub stalled: bool,
    /// Task ids that are over-estimate AND made zero progress this iteration.
    pub slow: Vec<String>,
    /// Task ids whose acceptance criteria were modified after the task was started.
    pub tampered: Vec<String>,
    /// True when the same correction was issued more than once in recent history.
    pub repeated_correction: bool,
    /// Task ids marked `Done` but lacking a verify artifact (no test / no proof).
    pub missing_verify: Vec<String>,
    /// When multiple tasks complete in the same batch, signal significant-change.
    pub significant_change: bool,
    /// Task ids that are blocking the frontier (open, blocking other open tasks).
    pub blocking: Vec<String>,
    /// Optional user-feedback ids to surface (non-empty = surface a UserFeedback event).
    pub user_feedback: Vec<String>,
}

// ── Event ─────────────────────────────────────────────────────────────────────

/// One actionable signal produced by `evaluate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    /// Task id this event is about.
    pub task: String,
    pub severity: Severity,
    /// True → surface immediately to the operator / escalation path.
    pub escalate: bool,
    /// Human-readable explanation (mandatory for `CriterionTampering`).
    pub reason: String,
}

impl Event {
    /// Compact fzf-style one-liner: `[SEV] KIND task — reason`.
    pub fn line(&self) -> String {
        let extra = match &self.kind {
            EventKind::Blocking { blast_radius } => format!(" (blast={blast_radius})"),
            EventKind::UserFeedback { repeated_correction } => {
                if *repeated_correction {
                    " (repeated)".to_string()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };
        let esc = if self.escalate { " [ESC]" } else { "" };
        format!(
            "[{}] {}{}{} {} — {}",
            self.severity.tag(),
            self.kind.label(),
            extra,
            esc,
            self.task,
            self.reason
        )
    }
}

// ── evaluate ─────────────────────────────────────────────────────────────────

/// Post-mark-done trigger evaluation. Pure function over store + ctx.
///
/// Rules applied in order:
/// 1. `CriterionTampering` — categorical Critical+escalate, always surfaced.
/// 2. `Stall`              — always Critical+escalate.
/// 3. `Regression`         — banded by count.
/// 4. `Blocking`           — blast radius from `plan::impact`.
/// 5. `SlowTask`           — banded by count.
/// 6. `MissingVerify`      — severity SCALED by task value + risk.
/// 7. `SignificantChange`  — Med baseline.
/// 8. `UserFeedback`       — repeated_correction lifts severity.
pub fn evaluate(store: &Store, ctx: &EventCtx) -> Vec<Event> {
    let mut events: Vec<Event> = Vec::new();

    // 1. CriterionTampering — CATEGORICAL (never downgraded, always Critical+escalate).
    for id in &ctx.tampered {
        events.push(Event {
            kind: EventKind::CriterionTampering,
            task: id.clone(),
            severity: Severity::Critical,
            escalate: true,
            reason: format!(
                "acceptance criteria modified after task started; \
                 all downstream assumptions must be re-verified"
            ),
        });
    }

    // 2. Stall — WORST, always Critical+escalate.
    if ctx.stalled {
        events.push(Event {
            kind: EventKind::Stall,
            task: "—".to_string(),
            severity: Severity::Critical,
            escalate: true,
            reason: "harness made no progress this iteration; \
                     frontier may be blocked or missing scope"
                .to_string(),
        });
    }

    // 3. Regression — banded by count.
    for id in &ctx.regressed {
        let score = 2u64 + blast_score(store, id);
        let sev = severity_band(score);
        events.push(Event {
            kind: EventKind::Regression,
            task: id.clone(),
            severity: sev,
            escalate: sev >= Severity::High,
            reason: format!("previously-done task is open again"),
        });
    }

    // 4. Blocking — blast radius from plan::impact.
    for id in &ctx.blocking {
        let blast = plan::impact(store, id).blocks.len();
        let score = 1u64 + blast as u64 * 2;
        let sev = severity_band(score);
        events.push(Event {
            kind: EventKind::Blocking { blast_radius: blast },
            task: id.clone(),
            severity: sev,
            escalate: sev >= Severity::High,
            reason: format!(
                "task is blocking {blast} transitive dependent(s)"
            ),
        });
    }

    // 5. SlowTask — banded by count.
    for id in &ctx.slow {
        let score = 1u64 + blast_score(store, id);
        let sev = severity_band(score);
        events.push(Event {
            kind: EventKind::SlowTask,
            task: id.clone(),
            severity: sev,
            escalate: sev >= Severity::High,
            reason: "task over-estimated and making zero progress (doom-loop risk)".to_string(),
        });
    }

    // 6. MissingVerify — severity SCALED by task value + risk (not flat).
    for id in &ctx.missing_verify {
        let (val, risk) = store
            .task(id)
            .map(|t| (t.value.max(0) as u64, t.risk.max(0) as u64))
            .unwrap_or((0, 0));
        // blast radius adds context-weight too.
        let score = val + risk + blast_score(store, id);
        let sev = severity_band(score);
        events.push(Event {
            kind: EventKind::MissingVerify,
            task: id.clone(),
            severity: sev,
            escalate: sev >= Severity::High,
            reason: format!(
                "task marked done with no verify artifact (value={val}, risk={risk})"
            ),
        });
    }

    // 7. SignificantChange — Med baseline, escalate at High+.
    if ctx.significant_change {
        events.push(Event {
            kind: EventKind::SignificantChange,
            task: "—".to_string(),
            severity: Severity::Med,
            escalate: false,
            reason: "fan-in gate: multiple tasks completed in same batch".to_string(),
        });
    }

    // 8. UserFeedback — repeated correction lifts to High.
    for id in &ctx.user_feedback {
        let sev = if ctx.repeated_correction {
            Severity::High
        } else {
            Severity::Med
        };
        events.push(Event {
            kind: EventKind::UserFeedback {
                repeated_correction: ctx.repeated_correction,
            },
            task: id.clone(),
            severity: sev,
            escalate: sev >= Severity::High,
            reason: if ctx.repeated_correction {
                "same correction issued more than once; model may not be absorbing the rule"
                    .to_string()
            } else {
                "user issued a correction".to_string()
            },
        });
    }

    events
}

/// Blast-radius score helper: count of transitive open dependents (capped for banding).
fn blast_score(store: &Store, id: &str) -> u64 {
    plan::impact(store, id).blocks.len() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Status, Store, Task};
    use std::path::PathBuf;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn write_store(tag: &str, spec: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("ks_events_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("spec.toml");
        std::fs::write(&p, spec).unwrap();
        p
    }

    /// Minimal single-task store (no deps).
    fn lone_store(id: &str, status: Status, value: i64, risk: i64) -> Store {
        Store {
            tasks: vec![Task {
                id: id.to_string(),
                status,
                task: id.to_string(),
                value,
                risk,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    // ── Stall always escalates ────────────────────────────────────────────────

    #[test]
    fn stall_always_critical_and_escalates() {
        let store = lone_store("T1", Status::Todo, 1, 1);
        let ctx = EventCtx {
            stalled: true,
            ..Default::default()
        };
        let evs = evaluate(&store, &ctx);
        let stall = evs.iter().find(|e| e.kind == EventKind::Stall).expect("no Stall event");
        assert_eq!(stall.severity, Severity::Critical, "Stall must be Critical");
        assert!(stall.escalate, "Stall must always escalate");
        // Verify line contains [ESC]
        assert!(stall.line().contains("[ESC]"), "line must flag escalation: {}", stall.line());
    }

    // ── CriterionTampering is categorical ─────────────────────────────────────

    #[test]
    fn criterion_tampering_categorical_always_critical() {
        // Even a tiny single-node store with no blast radius must be Critical+escalate.
        let store = lone_store("T2", Status::Wip, 1, 0);
        let ctx = EventCtx {
            tampered: vec!["T2".into()],
            ..Default::default()
        };
        let evs = evaluate(&store, &ctx);
        let ev = evs
            .iter()
            .find(|e| e.kind == EventKind::CriterionTampering)
            .expect("no CriterionTampering event");
        assert_eq!(ev.severity, Severity::Critical);
        assert!(ev.escalate);
        assert!(!ev.reason.is_empty(), "mandatory reason must be non-empty");
        assert!(ev.line().contains("[ESC]"), "{}", ev.line());
    }

    // ── MissingVerify is severity-SCALED ──────────────────────────────────────

    #[test]
    fn missing_verify_scaled_by_value_risk() {
        // Small node (value=1, risk=0, no deps) → Low severity.
        let small_store = lone_store("T3", Status::Done, 1, 0);
        let ctx_small = EventCtx {
            missing_verify: vec!["T3".into()],
            ..Default::default()
        };
        let evs_small = evaluate(&small_store, &ctx_small);
        let ev_small = evs_small
            .iter()
            .find(|e| e.kind == EventKind::MissingVerify)
            .expect("no MissingVerify event for small node");

        // High-value + high-risk node (value=5, risk=5) → higher severity.
        let big_store = lone_store("T4", Status::Done, 5, 5);
        let ctx_big = EventCtx {
            missing_verify: vec!["T4".into()],
            ..Default::default()
        };
        let evs_big = evaluate(&big_store, &ctx_big);
        let ev_big = evs_big
            .iter()
            .find(|e| e.kind == EventKind::MissingVerify)
            .expect("no MissingVerify event for big node");

        assert!(
            ev_big.severity > ev_small.severity,
            "big node (val=5,risk=5) must have higher severity than small node (val=1,risk=0); \
             got small={:?} big={:?}",
            ev_small.severity,
            ev_big.severity
        );
    }

    // ── Blocking reports real transitive blast_radius ─────────────────────────

    #[test]
    fn blocking_blast_radius_from_impact() {
        // T1 → T2 → T3 → T4 (chain): blocking T1 should give blast_radius=3.
        let sp = write_store(
            "blast",
            "schema=1\n\
             [[task]]\nid=\"T1\"\nstatus=\"wip\"\ntask=\"root\"\ndeps=[]\n\
             [[task]]\nid=\"T2\"\nstatus=\"todo\"\ntask=\"dep1\"\ndeps=[\"T1\"]\n\
             [[task]]\nid=\"T3\"\nstatus=\"todo\"\ntask=\"dep2\"\ndeps=[\"T2\"]\n\
             [[task]]\nid=\"T4\"\nstatus=\"todo\"\ntask=\"dep3\"\ndeps=[\"T3\"]\n",
        );
        let store = Store::load(&sp).unwrap();
        let ctx = EventCtx {
            blocking: vec!["T1".into()],
            ..Default::default()
        };
        let evs = evaluate(&store, &ctx);
        let ev = evs
            .iter()
            .find(|e| matches!(e.kind, EventKind::Blocking { .. }))
            .expect("no Blocking event");
        match &ev.kind {
            EventKind::Blocking { blast_radius } => {
                assert_eq!(*blast_radius, 3, "T1 blocks T2+T3+T4 transitively; got {blast_radius}");
            }
            _ => unreachable!(),
        }
        // Severity must reflect 3-task blast (score = 1 + 3*2 = 7 → High).
        assert_eq!(ev.severity, Severity::High);
    }

    // ── line() formatting smoke test ──────────────────────────────────────────

    #[test]
    fn line_format_is_parseable() {
        let ev = Event {
            kind: EventKind::Regression,
            task: "T5".into(),
            severity: Severity::Med,
            escalate: false,
            reason: "went red".into(),
        };
        let l = ev.line();
        assert!(l.starts_with("[MED]"), "bad prefix: {l}");
        assert!(l.contains("REGRESSION"), "{l}");
        assert!(l.contains("T5"), "{l}");
        assert!(!l.contains("[ESC]"), "non-escalating must not have [ESC]: {l}");
    }

    // ── Stall overrides low blast radius ──────────────────────────────────────

    #[test]
    fn stall_critical_even_with_tiny_blast() {
        // Single node, no deps — blast radius is 0. Stall must still be Critical.
        let store = lone_store("T6", Status::Todo, 0, 0);
        let ctx = EventCtx {
            stalled: true,
            ..Default::default()
        };
        let evs = evaluate(&store, &ctx);
        let stall = evs.iter().find(|e| e.kind == EventKind::Stall).unwrap();
        assert_eq!(stall.severity, Severity::Critical);
        assert!(stall.escalate);
    }
}
