//! T52 — autonomous Stop-hook driver (V34, V33, V27). On turn-end the driver
//! "plays the user": it decides whether to drive the plan forward, yield, or
//! escalate. This module is the PURE decision core — `decide()` is a total
//! function of (config, state, flagged variance, next task). All IO (running
//! `check done`, persisting the iteration counter, emitting the Stop-hook JSON)
//! lives in the caller, so the safety-critical logic is unit-tested in isolation.
//!
//! Safety (V34): autonomy is opt-in, hard-bounded by `max_iters`, and ANY flagged
//! variance escalates to the real user (V27) instead of driving blindly on.

use crate::config::DriverCfg;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Per-loop counter, persisted between Stop invocations (each runs as a fresh
/// process). Reset on a genuine user turn (T51) and whenever the driver yields.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub iters: u32,
}

const STATE_PATH: &str = ".kittenscrew/driver.json";

impl State {
    /// Load the counter; a missing/garbage file is a fresh `iters: 0` (never an
    /// error — the driver must not crash a turn-end).
    pub fn load() -> State {
        std::fs::read_to_string(STATE_PATH)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(".kittenscrew")?;
        std::fs::write(STATE_PATH, serde_json::to_string(self).unwrap_or_default())
    }

    /// Drop the counter to zero (yield / new user turn).
    pub fn reset() {
        let _ = State::default().save();
    }
}

/// What the driver decided to do at turn-end.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Drive on: block the stop and inject `context` as the next instruction.
    DriveOn { context: String },
    /// Yield to the user (allow the stop). `reason` explains why.
    Halt { reason: String },
    /// Hand a decision back to the real user (allow the stop, present `packet`).
    Escalate { packet: String },
}

/// The total decision function (V34). Precedence, safest-first:
/// 1. autonomy off → always Halt (passive; installing never hijacks a session).
/// 2. iteration cap hit → Halt (⊥ runaway).
/// 3. variance flagged → Escalate to the user (⊥ drive into uncertainty, V27).
/// 4. a ready next task → DriveOn.
/// 5. frontier empty → Halt (work done).
pub fn decide(
    cfg: &DriverCfg,
    state: &State,
    flagged: &[String],
    next: Option<(&str, &str)>,
) -> Decision {
    if !cfg.autonomous {
        return Decision::Halt {
            reason: "driver off ([driver] autonomous=false)".into(),
        };
    }
    if state.iters >= cfg.max_iters {
        return Decision::Halt {
            reason: format!(
                "auto-iteration cap reached ({}/{}) — yielding to user",
                state.iters, cfg.max_iters
            ),
        };
    }
    if !flagged.is_empty() {
        return Decision::Escalate {
            packet: format!(
                "value-variance flagged on {} — review before continuing (V27): {}",
                pluralize(flagged.len(), "task"),
                flagged.join(", ")
            ),
        };
    }
    match next {
        Some((id, task)) => Decision::DriveOn {
            context: format!(
                "[kittenscrew driver] continue autonomously — do next: {id} — {task}\n\
                 when done, mark it (`kittenscrew plan done {id}`); stop and ask if scope is unclear (V35)"
            ),
        },
        None => Decision::Halt {
            reason: "frontier empty — all ready tasks done or blocked".into(),
        },
    }
}

fn pluralize(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// True if this Stop is already a driver-continued turn (Claude Code sets
/// `stop_hook_active` once a Stop hook has blocked). Used only for telemetry —
/// the hard bound is `max_iters`, not this flag.
pub fn stop_hook_active(stdin: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(stdin)
        .ok()
        .and_then(|v| v.get("stop_hook_active").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

/// Is the store path present? (The driver no-ops outside a kittenscrew project.)
pub fn has_store() -> bool {
    Path::new(crate::store::STORE_PATH).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(on: bool, max: u32) -> DriverCfg {
        DriverCfg {
            autonomous: on,
            max_iters: max,
        }
    }

    #[test]
    fn off_always_halts() {
        let d = decide(&cfg(false, 8), &State { iters: 0 }, &[], Some(("T1", "x")));
        assert!(matches!(d, Decision::Halt { .. }));
    }

    #[test]
    fn cap_halts_even_with_work() {
        let d = decide(&cfg(true, 3), &State { iters: 3 }, &[], Some(("T1", "x")));
        match d {
            Decision::Halt { reason } => assert!(reason.contains("cap reached")),
            _ => panic!("expected Halt at cap"),
        }
    }

    #[test]
    fn flagged_escalates_before_driving() {
        // Even with a ready task and budget left, flagged variance wins.
        let d = decide(
            &cfg(true, 8),
            &State { iters: 1 },
            &["T9".into()],
            Some(("T2", "build")),
        );
        match d {
            Decision::Escalate { packet } => {
                assert!(packet.contains("T9"));
                assert!(packet.contains("V27"));
            }
            _ => panic!("expected Escalate on flagged variance"),
        }
    }

    #[test]
    fn ready_task_drives_on() {
        let d = decide(
            &cfg(true, 8),
            &State { iters: 2 },
            &[],
            Some(("T7", "topo")),
        );
        match d {
            Decision::DriveOn { context } => {
                assert!(context.contains("do next: T7"));
                assert!(context.contains("plan done T7"));
            }
            _ => panic!("expected DriveOn"),
        }
    }

    #[test]
    fn empty_frontier_halts() {
        let d = decide(&cfg(true, 8), &State { iters: 0 }, &[], None);
        match d {
            Decision::Halt { reason } => assert!(reason.contains("frontier empty")),
            _ => panic!("expected Halt"),
        }
    }
}
