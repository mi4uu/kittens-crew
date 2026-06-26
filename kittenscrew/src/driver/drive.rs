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
    /// Default sandbox (T64/T90): when set, every node's scope target must resolve
    /// INSIDE this root — a write that escapes upward (outside the project) is blocked
    /// before the model is even called. `None` = no confinement (e.g. the A/B bench,
    /// which materialises scopes in an isolated temp dir it owns).
    pub workspace_root: Option<PathBuf>,
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
    // Whole-crate verify (multi-file ceiling fix): the green leaves, to assemble into a
    // runnable binary once the frontier empties. Only used on the confined `run` path.
    let mut written: Vec<PathBuf> = Vec::new();
    // T86: nodes whose verify never passed. They are retired (Killed) so they leave the
    // ready frontier, but the run CONTINUES to other independent ready nodes instead of
    // halting globally. Only a frontier emptied WITH failures is a real (partial) stall.
    let mut failed: Vec<String> = Vec::new();
    for _ in 0..opts.max_iters {
        let store = Store::load(&opts.store_path).map_err(|e| e.to_string())?;
        let next = plan::next(&store)
            .map(|t| (t.id.clone(), t.task.clone(), t.scope.clone(), t.accept.clone()));
        let (id, task, scope, accept) = match next {
            None => {
                // Frontier empty + all green → assemble the actual program (run path only).
                // A failed whole-crate build is a real Halt: green leaves, but the program
                // the user described does NOT run.
                if failed.is_empty() && opts.workspace_root.is_some() {
                    match verify::build_binary(&written) {
                        Ok(Some(bin)) => {
                            progress("·crate", &format!("✓ built {}", bin.display()));
                            // Behavioural gate: a program must also DO what was asked, not just
                            // compile. Run every task's accept cases against the binary.
                            let cases: Vec<_> = store
                                .tasks
                                .iter()
                                .flat_map(|t| t.accept.iter().cloned())
                                .collect();
                            if let Err(detail) = verify::run_accept(&bin, &cases) {
                                return Ok(Outcome::Halted {
                                    node: "·crate".into(),
                                    reason: format!("built but behaviour wrong: {}", first_line(&detail)),
                                    done,
                                });
                            }
                            if !cases.is_empty() {
                                progress("·crate", &format!("✓ {} accept case(s) passed", cases.len()));
                            }
                        }
                        Ok(None) => {}
                        Err(detail) => {
                            return Ok(Outcome::Halted {
                                node: "·crate".into(),
                                reason: format!("whole-crate build failed: {}", first_line(&detail)),
                                done,
                            });
                        }
                    }
                }
                return Ok(if failed.is_empty() {
                    Outcome::Converged { done }
                } else {
                    Outcome::Halted {
                        node: failed[0].clone(),
                        reason: format!(
                            "{} node(s) failed verify ({}); {done} independent node(s) still driven green",
                            failed.len(),
                            failed.join(", ")
                        ),
                        done,
                    }
                });
            }
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

        // Default sandbox (T64/T90): refuse to write outside the workspace root BEFORE
        // calling the model. A path-escape is a blocked node, not a halt — retire it and
        // drive on (same T86 continue-past-failure semantics).
        if let Some(root) = &opts.workspace_root {
            let rules = super::tripwire::Ruleset::default_at(root.clone());
            let verdict = super::tripwire::evaluate(
                &rules,
                &super::tripwire::Action::Write { path: target.clone(), lines: 0 },
            );
            if verdict.action == super::tripwire::TripAction::Block {
                mark_status(&opts.store_path, &id, Status::Killed)?;
                progress(&id, &format!("✗ blocked: {}", verdict.reason));
                failed.push(id);
                continue;
            }
        }

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
            // An empty (or whitespace-only) leaf compiles fine as an empty lib crate — a
            // FALSE green that converges on a program with no code. Treat it as a failed
            // attempt so the retry/repair loop re-prompts instead of marking it done.
            if code.trim().is_empty() {
                last_err = "model returned no code (empty output)".into();
                continue;
            }
            std::fs::write(&target, &code)
                .map_err(|e| format!("{id}: write {}: {e}", target.display()))?;
            let v = verify::rustc_compiles(&target);
            if !v.ok {
                last_err = v.detail;
                continue;
            }
            // Behavioural auto-repair: if this leaf is a program crate root (has `fn main`)
            // and carries accept cases, don't just type-check it — BUILD and run it. A
            // behaviour mismatch (e.g. the args[0] leak) feeds the diff back through
            // repair_prompt so the model fixes it itself, instead of converging on a wrong
            // program. rustc resolves any `mod` siblings from disk (driven earlier as deps).
            if !accept.is_empty() && code.contains("fn main") {
                match verify::build_binary(std::slice::from_ref(&target)) {
                    Ok(Some(bin)) => {
                        if let Err(detail) = verify::run_accept(&bin, &accept) {
                            last_err = detail;
                            continue;
                        }
                    }
                    Ok(None) => {}
                    Err(detail) => {
                        last_err = detail;
                        continue;
                    }
                }
            }
            model = res.model;
            passed = true;
            break;
        }
        if !passed {
            // T86: do NOT halt the whole run. Retire this node (Killed → leaves the ready
            // frontier; its transitive dependents stay blocked since its dep never goes
            // Done) and CONTINUE driving other independent ready nodes. A bare loop attempts
            // every node independently; the harness must not score worse by abandoning
            // recoverable independent work behind one bad node.
            mark_status(&opts.store_path, &id, Status::Killed)?;
            progress(&id, &format!("✗ failed: {}", first_line(&last_err)));
            failed.push(id);
            continue;
        }

        // Council deliberation (run path only): the kitties post to the blackboard so a
        // real run leaves a deliberation trail the `council` TUI surfaces. The Builder 🔨
        // reports the delivery; the Grill 🔥 red-teams the leaf for compiles-but-stub
        // smells. Surfacing only — the deterministic verify already gated the advance — so
        // this never blocks the loop (bench/tests pass workspace_root None and skip it).
        if opts.workspace_root.is_some() {
            post_council(&id, &model, &target);
        }

        // Advance: mark the node done in the authoritative store.
        mark_done(&opts.store_path, &id)?;
        written.push(target.clone());
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

/// Post the per-node council deliberation to the blackboard (topic = node id). The
/// Builder reports the build; the Grill red-teams the leaf. A best-effort side-effect:
/// a board write that fails must never derail the build, so errors are swallowed.
fn post_council(id: &str, model: &str, target: &Path) {
    use crate::board::{self, Opinion};
    let _ = board::post(&Opinion {
        kitty: "builder".into(),
        topic: id.into(),
        stance: "built".into(),
        confidence: 0.8,
        competence: 0.9, // building is the Builder's home domain
        seq: 0,
    });
    let code = std::fs::read_to_string(target).unwrap_or_default();
    let (stance, confidence) = match board::grill_smells(&code) {
        Some(reason) => (format!("reject: {reason}"), 0.9),
        None => (format!("approve ({model})"), 0.6),
    };
    let _ = board::post(&Opinion {
        kitty: "grill".into(),
        topic: id.into(),
        stance,
        confidence,
        competence: 0.9, // red-teaming is the Grill's home domain
        seq: 0,
    });
}

/// Mark a node done in the store. ponytail: skips the SPEC.md re-render (that's a
/// projection, not authority — `kittenscrew spec render` resyncs it); the loop
/// only needs the authoritative TOML advanced so `plan::next` returns the successor.
pub(crate) fn mark_done(path: &Path, id: &str) -> Result<(), String> {
    mark_status(path, id, Status::Done)
}

/// Set a node's status in the authoritative store. Used to advance a green node
/// (Done) or to retire an unrecoverable one (Killed) so it leaves the ready
/// frontier without halting the whole run (T86).
pub(crate) fn mark_status(path: &Path, id: &str, status: Status) -> Result<(), String> {
    let mut s = Store::load(path).map_err(|e| e.to_string())?;
    let t = s
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("unknown task {id}"))?;
    t.status = status;
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

    /// The accept gate auto-repairs behaviour, not just compilation: a program that
    /// COMPILES but echoes args[0] (the binary path) fails its accept case; the diff is
    /// fed back and the second attempt — which skips args[0] — passes and advances.
    #[test]
    fn accept_failure_drives_behavioural_repair() {
        use crate::driver::api::{Driver, DriverError, Turn, TurnResult};
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Scripted {
            n: AtomicUsize,
            replies: Vec<&'static str>,
        }
        impl Driver for Scripted {
            fn dispatch(&self, _t: &Turn) -> Result<TurnResult, DriverError> {
                let i = self.n.fetch_add(1, Ordering::SeqCst).min(self.replies.len() - 1);
                Ok(TurnResult { text: self.replies[i].into(), model: "scripted".into() })
            }
            fn model(&self) -> &str {
                "scripted"
            }
        }

        let dir = std::env::temp_dir().join(format!("ks_arepair_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let store_path = dir.join("spec.toml");
        let main_rs = dir.join("main.rs");
        std::fs::write(
            &store_path,
            format!(
                "schema = 1\n\n[[task]]\nid = \"T1\"\nstatus = \"todo\"\ntask = \"reverse words\"\ndeps = []\npriority = 1\nscope = [{:?}]\n\n[[task.accept]]\nargs = [\"a\", \"b\", \"c\"]\nstdout = \"c b a\"\n",
                main_rs.to_str().unwrap()
            ),
        )
        .unwrap();

        let driver = Scripted {
            n: AtomicUsize::new(0),
            replies: vec![
                // Buggy: includes args[0] (the binary path) → output has a trailing path.
                "```rust\nfn main(){let a:Vec<String>=std::env::args().collect();let mut r=a.clone();r.reverse();println!(\"{}\", r.join(\" \"));}\n```",
                // Fixed: skip(1) drops the binary name → "c b a".
                "```rust\nfn main(){let mut a:Vec<String>=std::env::args().skip(1).collect();a.reverse();println!(\"{}\", a.join(\" \"));}\n```",
            ],
        };
        let opts = DriveOpts {
            max_iters: 5,
            max_retries: 2,
            store_path: store_path.clone(),
            // workspace_root None: per-node accept still runs (it's gated on accept cases, not
            // on the sandbox); only the post-converge whole-crate gate is run-path-only.
            workspace_root: None,
        };
        let out = drive(&driver, &opts, |_, _| {}).unwrap();
        assert!(matches!(out, Outcome::Converged { done: 1 }), "got {out:?}");
        assert_eq!(driver.n.load(Ordering::SeqCst), 2, "should have repaired once");
        let _ = std::fs::remove_dir_all(&dir);
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
            workspace_root: None,
        };
        let out = drive(&driver, &opts, |_, _| {}).unwrap();
        assert!(matches!(out, Outcome::Converged { done: 1 }), "got {out:?}");
        assert_eq!(driver.n.load(Ordering::SeqCst), 2, "should have retried once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T86: a node that never compiles must NOT abandon an INDEPENDENT ready node.
    /// Node A (priority 1) always fails verify; node B (priority 2, no dep on A) is good.
    /// OLD behaviour: drive() Halted at A with done=0 — B never attempted (this lost the
    /// live A/B bench to the bare loop, -33pp). NEW: A is retired (Killed), B is driven
    /// green → done=1, Outcome::Halted reports A failed but B still completed.
    #[test]
    fn t86_failed_node_does_not_abandon_independent_node() {
        use crate::driver::api::{Driver, DriverError, Turn, TurnResult};

        struct PartlyBroken;
        impl Driver for PartlyBroken {
            fn dispatch(&self, t: &Turn) -> Result<TurnResult, DriverError> {
                let text = if t.prompt.contains("alpha") {
                    "```rust\npub fn alpha() -> i64 { 1 + }\n```".to_string() // never compiles
                } else {
                    "```rust\npub fn beta() -> i64 { 2 }\n```".to_string() // good
                };
                Ok(TurnResult { text, model: "partly".into() })
            }
            fn model(&self) -> &str {
                "partly"
            }
        }

        let dir = std::env::temp_dir().join(format!("ks_t86_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store_path = dir.join("spec.toml");
        let a = dir.join("a.rs");
        let b = dir.join("b.rs");
        std::fs::write(
            &store_path,
            format!(
                "schema = 1\n\n\
                 [[task]]\nid = \"A\"\nstatus = \"todo\"\ntask = \"alpha\"\ndeps = []\npriority = 1\nscope = [{:?}]\n\
                 [[task]]\nid = \"B\"\nstatus = \"todo\"\ntask = \"beta\"\ndeps = []\npriority = 2\nscope = [{:?}]\n",
                a.to_str().unwrap(),
                b.to_str().unwrap()
            ),
        )
        .unwrap();

        let opts = DriveOpts { max_iters: 10, max_retries: 1, store_path: store_path.clone(), workspace_root: None };
        let out = drive(&PartlyBroken, &opts, |_, _| {}).unwrap();

        match out {
            Outcome::Halted { done, .. } => {
                assert_eq!(done, 1, "independent node B must be driven green despite A failing")
            }
            other => panic!("expected Halted(A failed) with done=1, got {other:?}"),
        }
        assert!(verify::rustc_compiles(&b).ok, "B's file must compile");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T90 default sandbox: a scope path that escapes the workspace root is BLOCKED
    /// before the model is called and never written outside the project.
    #[test]
    fn workspace_confinement_blocks_escape() {
        use crate::driver::api::{Driver, DriverError, Turn, TurnResult};

        struct Any;
        impl Driver for Any {
            fn dispatch(&self, _t: &Turn) -> Result<TurnResult, DriverError> {
                Ok(TurnResult { text: "```rust\npub fn x() {}\n```".into(), model: "any".into() })
            }
            fn model(&self) -> &str {
                "any"
            }
        }

        let dir = std::env::temp_dir().join(format!("ks_conf_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        let store_path = dir.join("spec.toml");
        let escape = dir.join("escape.rs"); // sibling of project/ → OUTSIDE the root
        std::fs::write(
            &store_path,
            format!(
                "schema = 1\n\n[[task]]\nid = \"E\"\nstatus = \"todo\"\ntask = \"x\"\ndeps = []\npriority = 1\nscope = [{:?}]\n",
                escape.to_str().unwrap()
            ),
        )
        .unwrap();

        let opts = DriveOpts {
            max_iters: 5,
            max_retries: 0,
            store_path: store_path.clone(),
            workspace_root: Some(root.clone()),
        };
        let out = drive(&Any, &opts, |_, _| {}).unwrap();

        assert!(matches!(out, Outcome::Halted { .. }), "escape must be blocked → Halted, got {out:?}");
        assert!(!escape.exists(), "must NOT write outside the workspace root");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
