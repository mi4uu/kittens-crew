//! T62 — the DAG-driven drive loop: the answer to "we wait for the smart model".
//! Instead of handing the model the whole goal and hoping it picks the right next
//! step, the harness DRIVES: it takes the next ready node (`plan::next` — deps
//! satisfied, lowest priority), dispatches a prompt scoped to ONLY that node,
//! applies the output, verifies deterministically (T63), and only then advances.
//! Done = the frontier is empty (all ready nodes green), not "the model said done".
//! Hard-bounded by `max_iters` (V34). A failed verify HALTS here; bounded replan
//! (retry → patch → full replan) is T74.

use super::api::{Driver, Turn};
use super::verify;
use crate::plan;
use crate::store::{Status, Store};
use std::path::{Path, PathBuf};

pub struct DriveOpts {
    pub max_iters: u32,
    /// Bounded replan (T74): on a failed verify, retry feeding the compiler error
    /// back as a local patch, up to this many times before halting. 0 = no retry.
    pub max_retries: u32,
    pub store_path: PathBuf,
}

#[derive(Debug)]
pub enum Outcome {
    /// Frontier empty — every ready node was driven green.
    Converged { done: u32 },
    /// Hit the iteration cap with work still pending.
    CapReached { done: u32 },
    /// A node failed its verify (or had nowhere to write). The loop stops rather
    /// than driving blindly on — escalation/replan is the operator's call (T74).
    Halted { node: String, reason: String, done: u32 },
}

/// Run the loop against the store at `opts.store_path`, filling code leaves through
/// `driver`. Returns how it ended. `progress` is called once per advanced node so a
/// CLI can narrate ("T1 → done") without this module owning any IO policy.
pub fn drive(
    driver: &dyn Driver,
    opts: &DriveOpts,
    mut progress: impl FnMut(&str, &str),
) -> Result<Outcome, String> {
    let mut done = 0u32;
    for _ in 0..opts.max_iters {
        let store = Store::load(&opts.store_path).map_err(|e| e.to_string())?;
        let next = plan::next(&store).map(|t| (t.id.clone(), t.task.clone(), t.scope.clone()));
        let (id, task, scope) = match next {
            None => return Ok(Outcome::Converged { done }),
            Some(t) => t,
        };

        let Some(target) = scope.first() else {
            return Ok(Outcome::Halted {
                node: id,
                reason: "node has no scope file to write".into(),
                done,
            });
        };
        let target = PathBuf::from(target);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Fill the leaf, then verify (T63). On a failed verify, escalate through
        // bounded replan (T74): retry feeding the rustc error back as a local patch,
        // up to `max_retries`. Full replan (planner re-derives the subgraph) is later.
        let mut last_err = String::new();
        let mut model = String::new();
        let mut passed = false;
        for attempt in 0..=opts.max_retries {
            let prompt = if attempt == 0 {
                scoped_prompt(&task, &target)
            } else {
                repair_prompt(&task, &target, &last_err)
            };
            let res = driver
                .dispatch(&Turn { prompt })
                .map_err(|e| format!("{id}: dispatch: {e}"))?;
            let code = extract_code(&res.text);
            std::fs::write(&target, &code)
                .map_err(|e| format!("{id}: write {}: {e}", target.display()))?;
            let v = verify::rustc_compiles(&target);
            if v.ok {
                model = res.model;
                passed = true;
                break;
            }
            last_err = v.detail;
        }
        if !passed {
            return Ok(Outcome::Halted {
                node: id,
                reason: format!(
                    "verify failed after {} attempt(s): {}",
                    opts.max_retries + 1,
                    first_line(&last_err)
                ),
                done,
            });
        }

        // Advance: mark the node done in the authoritative store.
        mark_done(&opts.store_path, &id)?;
        progress(&id, &model);
        done += 1;
    }
    Ok(Outcome::CapReached { done })
}

/// The whole point: the model sees ONLY this leaf, never the global plan.
/// `pub(crate)` so the A/B bench (T75) gives both arms the identical prompt.
pub(crate) fn scoped_prompt(task: &str, target: &Path) -> String {
    format!(
        "You are filling ONE leaf of a build plan. Do EXACTLY this node — nothing more, \
         no extra files, no scaffolding.\n\n\
         TASK: {task}\n\
         TARGET FILE: {file}\n\n\
         Output ONLY the complete Rust source for that file, in a single ```rust fenced \
         block. No prose. It must compile as a library crate (`rustc --crate-type lib`), \
         with no external dependencies.",
        file = target.display()
    )
}

/// Bounded-replan local patch (T74): re-dispatch with the verbatim rustc error so
/// the model fixes its own leaf instead of the harness halting on the first miss.
fn repair_prompt(task: &str, target: &Path, err: &str) -> String {
    format!(
        "Your previous attempt at this leaf did NOT compile. Fix it.\n\n\
         TASK: {task}\n\
         TARGET FILE: {file}\n\n\
         rustc error:\n{err}\n\n\
         Output ONLY the corrected complete Rust source in a single ```rust fenced \
         block. No prose. It must compile as a library crate with no external \
         dependencies.",
        file = target.display(),
        err = err.trim()
    )
}

/// Pull the first ```fenced``` block; fall back to the whole text if unfenced.
pub(crate) fn extract_code(text: &str) -> String {
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        // Drop the opening fence's language tag line (e.g. "rust\n").
        let after = after.splitn(2, '\n').nth(1).unwrap_or(after);
        if let Some(end) = after.find("```") {
            return after[..end].trim_end().to_string();
        }
    }
    text.trim().to_string()
}

fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
}

/// Mark a node done in the store. ponytail: skips the SPEC.md re-render (that's a
/// projection, not authority — `kittenscrew spec render` resyncs it); the loop
/// only needs the authoritative TOML advanced so `plan::next` returns the successor.
pub(crate) fn mark_done(path: &Path, id: &str) -> Result<(), String> {
    let mut s = Store::load(path).map_err(|e| e.to_string())?;
    let t = s
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("unknown task {id}"))?;
    t.status = Status::Done;
    s.save(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_rust_block() {
        let out = "Here you go:\n```rust\nfn add() {}\n```\nDone.";
        assert_eq!(extract_code(out), "fn add() {}");
    }

    #[test]
    fn extracts_fence_without_lang_tag() {
        let out = "```\nfn x() {}\n```";
        assert_eq!(extract_code(out), "fn x() {}");
    }

    #[test]
    fn unfenced_text_returns_trimmed_whole() {
        assert_eq!(extract_code("  fn y() {}  "), "fn y() {}");
    }

    #[test]
    fn first_line_skips_blanks() {
        assert_eq!(first_line("\n\n  error: bad\nmore"), "error: bad");
    }

    /// T74: a leaf that first comes back broken is recovered by feeding the rustc
    /// error back (local patch), then the node advances — not an immediate Halt.
    #[test]
    fn bounded_replan_recovers_bad_then_good_leaf() {
        use crate::driver::api::{Driver, DriverError, Turn, TurnResult};
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Scripted {
            n: AtomicUsize,
            replies: Vec<&'static str>,
        }
        impl Driver for Scripted {
            fn dispatch(&self, _t: &Turn) -> Result<TurnResult, DriverError> {
                let i = self.n.fetch_add(1, Ordering::SeqCst).min(self.replies.len() - 1);
                Ok(TurnResult {
                    text: self.replies[i].into(),
                    model: "scripted".into(),
                })
            }
            fn model(&self) -> &str {
                "scripted"
            }
        }

        let dir = std::env::temp_dir().join(format!("ks_replan_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let store_path = dir.join("spec.toml");
        let out_rs = dir.join("out.rs");
        std::fs::write(
            &store_path,
            format!(
                "schema = 1\n\n[[task]]\nid = \"T1\"\nstatus = \"todo\"\ntask = \"add\"\ndeps = []\npriority = 1\nscope = [{:?}]\n",
                out_rs.to_str().unwrap()
            ),
        )
        .unwrap();

        let driver = Scripted {
            n: AtomicUsize::new(0),
            replies: vec![
                "```rust\npub fn add(a: i64, b: i64) -> i64 { a + }\n```", // broken
                "```rust\npub fn add(a: i64, b: i64) -> i64 { a + b }\n```", // fixed
            ],
        };
        let opts = DriveOpts {
            max_iters: 5,
            max_retries: 2,
            store_path: store_path.clone(),
        };
        let out = drive(&driver, &opts, |_, _| {}).unwrap();
        assert!(matches!(out, Outcome::Converged { done: 1 }), "got {out:?}");
        assert_eq!(driver.n.load(Ordering::SeqCst), 2, "should have retried once");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
