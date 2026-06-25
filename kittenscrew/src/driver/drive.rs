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

        // Dispatch a prompt scoped to ONLY this node — the model fills the leaf.
        let res = driver
            .dispatch(&Turn {
                prompt: scoped_prompt(&task, &target),
            })
            .map_err(|e| format!("{id}: dispatch: {e}"))?;

        // Apply: write the extracted code to the node's scope file.
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let code = extract_code(&res.text);
        std::fs::write(&target, &code)
            .map_err(|e| format!("{id}: write {}: {e}", target.display()))?;

        // Verify deterministically (T63) BEFORE advancing.
        let v = verify::rustc_compiles(&target);
        if !v.ok {
            return Ok(Outcome::Halted {
                node: id,
                reason: format!("verify failed: {}", first_line(&v.detail)),
                done,
            });
        }

        // Advance: mark the node done in the authoritative store.
        mark_done(&opts.store_path, &id)?;
        progress(&id, res.model.as_str());
        done += 1;
    }
    Ok(Outcome::CapReached { done })
}

/// The whole point: the model sees ONLY this leaf, never the global plan.
fn scoped_prompt(task: &str, target: &Path) -> String {
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

/// Pull the first ```fenced``` block; fall back to the whole text if unfenced.
fn extract_code(text: &str) -> String {
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
fn mark_done(path: &Path, id: &str) -> Result<(), String> {
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
}
