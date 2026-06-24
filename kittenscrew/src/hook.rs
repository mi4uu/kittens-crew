//! Hook orchestration: dispatch by event. V7: single entry, delegates to squeez.

use crate::commands::{worth_params, SPEC_PATH};
use crate::error::KittenError;
use crate::{check, compression, config, driver, gate, intake, kitty, plan, spec, squeez, store};
use std::io::Read;

pub fn dispatch(event: &str) -> Result<(), KittenError> {
    let mut stdin = String::new();
    std::io::stdin().read_to_string(&mut stdin)?;
    match event {
        "session-start" => session_start(&stdin),
        "user-prompt" => user_prompt(&stdin),
        "pre-tool" => pre_tool(&stdin),
        "post-tool" => post_tool(&stdin),
        "stop" => stop(&stdin),
        "subagent-stop" => subagent_stop(&stdin),
        "pre-compact" => pre_compact(&stdin),
        "post-compact" => post_compact(&stdin),
        other => Err(KittenError::Validation(format!(
            "unknown hook event: {other}"
        ))),
    }
}

/// T53: SubagentStop — a subagent finished. The membrane covers it so squeez
/// can fold the subagent's output (V33: nothing bypasses); kittenscrew adds no
/// driving here (the parent turn's Stop hook decides). Graceful: no squeez
/// script → no-op.
fn subagent_stop(stdin: &str) -> Result<(), KittenError> {
    if let Some(out) = squeez::run_hook("subagent-stop", stdin)? {
        if !out.trim().is_empty() {
            print!("{out}");
        }
    }
    Ok(())
}

/// T53: PostCompact — after a context compaction. Delegate to squeez's
/// postcompact hook (session-state restore) so the membrane owns this event
/// too (V33). Graceful: missing script → no-op.
fn post_compact(stdin: &str) -> Result<(), KittenError> {
    if let Some(out) = squeez::run_hook("post-compact", stdin)? {
        if !out.trim().is_empty() {
            print!("{out}");
        }
    }
    Ok(())
}

/// T51: UserPromptSubmit intake (V35, V33). Classify the prompt, inject ONLY
/// targeted context (`plan next` + a referenced task's record) as
/// `additionalContext`. Deterministic; the LLM resolves any flagged ambiguity.
/// Graceful: a malformed/empty payload still emits valid (if sparse) context.
fn user_prompt(stdin: &str) -> Result<(), KittenError> {
    // A genuine human turn refills the driver's auto-iteration budget (T52).
    driver::State::reset();
    let prompt = serde_json::from_str::<serde_json::Value>(stdin)
        .ok()
        .and_then(|v| v.get("prompt").and_then(|p| p.as_str()).map(str::to_owned))
        .unwrap_or_default();

    let intent = intake::classify(&prompt);

    // Resolve next + the referenced task from the store (best-effort: a
    // missing/unreadable store just yields an empty frontier, never a crash).
    let store = store::Store::load(std::path::Path::new(store::STORE_PATH)).ok();
    let wp = worth_params();
    let next = store
        .as_ref()
        .and_then(|s| plan::next_with(s, &wp).map(|t| (t.id.clone(), t.task.clone())));
    let referenced = match (intake::task_ref(&prompt), store.as_ref()) {
        (Some(id), Some(s)) => s.tasks.iter().find(|t| t.id == id).cloned(),
        _ => None,
    };

    let mut ctx = intake::render(
        intent,
        next.as_ref().map(|(i, t)| (i.as_str(), t.as_str())),
        referenced.as_ref(),
    );
    // T55: tell the agent which hat to wear for the work it's about to do.
    if let Some((_, task)) = referenced
        .as_ref()
        .map(|t| (t.id.as_str(), t.task.as_str()))
    {
        ctx.push_str(&format!("suggested role: {}\n", kitty::role_hint(task)));
    } else if let Some((_, task)) = next.as_ref() {
        ctx.push_str(&format!("suggested role: {}\n", kitty::role_hint(task)));
    }
    // T57: no plan → no work. Whatever the user said (even casual chatter, no
    // commands), the cats listen and route it into a plan FIRST — don't free-build.
    let gcfg = config::load().unwrap_or_default();
    if gcfg.gate.enforce_plan && !gate::plan_exists() {
        ctx.push_str(&format!("\n⚠ NO PLAN EXISTS — {}\n", gate::PLAN_STEER));
    }
    // Claude Code UserPromptSubmit contract: additionalContext is injected
    // before the turn. Emit as a JSON string (serde escapes newlines).
    let payload = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": ctx,
        }
    });
    println!("{payload}");
    Ok(())
}

/// T5: SessionStart — verify install, call `squeez init`, log ready.
fn session_start(_stdin: &str) -> Result<(), KittenError> {
    match squeez::bin() {
        Some(bin) => {
            // Delegate session setup to squeez init (registers hooks, etc.)
            let _ = squeez::run(&["init"], "");
            // Unique kittenscrew value: ready banner w/ kitty identity.
            let k = kitty::lookup("planning").expect("planning kitty constant");
            println!(
                "{} [{}] session ready (squeez: {})",
                k.emoji,
                k.name,
                bin.display()
            );
        }
        None => {
            // V2: graceful degrade — warn but don't fail.
            eprintln!("kittenscrew: warning: squeez not found, compression disabled");
        }
    }

    // 🐾 Helper Kitty: narrate plan progress ONCE, if a plan store exists and
    // narration is on. Absent/empty store → emit nothing (don't crash, don't
    // narrate). Best-effort: an unreadable store is simply silent.
    let cfg = config::load().unwrap_or_default();
    if cfg.helper.narrate {
        if let Ok(store) = store::Store::load(std::path::Path::new(store::STORE_PATH)) {
            if !store.tasks.is_empty() {
                let wp = worth_params();
                let next = plan::next_with(&store, &wp).map(|t| (t.id.as_str(), t.task.as_str()));
                let msg = helper_narration(&store, next);
                let helper = kitty::lookup("helper").expect("helper kitty constant");
                println!("{}", kitty::say(helper, &msg));
            }
        }
    }
    Ok(())
}

/// Max chars of a task's text to show in the Helper's narration line.
const NARRATE_SLICE: usize = 32;

/// Build the Helper Kitty's one-line plan summary: `N/M tasks done · next: …`.
/// done = tasks `Status::Done`; total = tasks not `Status::Killed`. `next` is the
/// planner's pick (id + a short slice of its text); `None` → "plan complete"
/// when all live work is done, else "no ready task" (frontier blocked).
fn helper_narration(store: &store::Store, next: Option<(&str, &str)>) -> String {
    let done = store
        .tasks
        .iter()
        .filter(|t| t.status == store::Status::Done)
        .count();
    let total = store
        .tasks
        .iter()
        .filter(|t| t.status != store::Status::Killed)
        .count();
    let tail = match next {
        Some((id, task)) => {
            let slice: String = task.chars().take(NARRATE_SLICE).collect();
            let ellipsis = if task.chars().count() > NARRATE_SLICE {
                " …"
            } else {
                ""
            };
            format!("next: {id} {slice}{ellipsis}")
        }
        None if done == total => "plan complete".to_string(),
        None => "no ready task".to_string(),
    };
    format!("{done}/{total} tasks done · {tail}")
}

/// T52: Stop — the autonomous driver (V34). Default-OFF: with `[driver]
/// autonomous=false` (or outside a kittenscrew project) the hook allows the
/// stop silently — installing the membrane never hijacks an interactive
/// session. When ON: verify the turn's work (`check done` demote, V19), audit
/// variance, then `driver::decide` — drive on (block-stop + inject), yield, or
/// escalate to the user. Hard-bounded by `[driver] max_iters` (⊥ runaway).
fn stop(stdin: &str) -> Result<(), KittenError> {
    let cfg = config::load().unwrap_or_default();
    // Fast, safe path: no output = allow the stop.
    if !cfg.driver.autonomous || !driver::has_store() {
        return Ok(());
    }
    let _active = driver::stop_hook_active(stdin); // telemetry; bound is max_iters

    let store_path = std::path::Path::new(store::STORE_PATH);
    let mut s = store::Store::load(store_path)?;

    // 1. Verify the turn's delivery: demote any `x` that's fake (V19). Only
    //    persist the re-render when SPEC.md is in sync (T47: ⊥ clobber a
    //    pending manual edit); otherwise the demote stays in-memory for the
    //    decision and the user reconciles via `spec drift --apply`.
    let demote: Vec<String> = check::check_done(&s)
        .into_iter()
        .filter(|r| !r.ok)
        .map(|r| r.id)
        .collect();
    if !demote.is_empty() {
        for t in s.tasks.iter_mut() {
            if demote.contains(&t.id) {
                t.status = store::Status::Wip;
            }
        }
        let on_disk = std::fs::read_to_string(SPEC_PATH).unwrap_or_default();
        if on_disk.is_empty() || spec::is_synced(&s, &on_disk) {
            s.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
        }
    }

    // 2. Audit cadence: flag tasks whose delivered value missed expectation.
    let flagged: Vec<String> = check::value_variance(&s, cfg.audit.variance_threshold)
        .into_iter()
        .filter(|r| r.flagged)
        .map(|r| r.id)
        .collect();

    // 3. Decide on the next move.
    let wp = worth_params();
    let next = plan::next_with(&s, &wp).map(|t| (t.id.clone(), t.task.clone()));
    let state = driver::State::load();
    let decision = driver::decide(
        &cfg.driver,
        &state,
        &flagged,
        next.as_ref().map(|(i, t)| (i.as_str(), t.as_str())),
    );

    let k = kitty::lookup("orchestrating").expect("orchestrating kitty");
    match decision {
        driver::Decision::DriveOn { context } => {
            let _ = driver::State {
                iters: state.iters + 1,
            }
            .save();
            // T55: graft the suggested role onto the drive-on instruction.
            let context = match next.as_ref() {
                Some((_, task)) => {
                    format!("{context}\nsuggested role: {}", kitty::role_hint(task))
                }
                None => context,
            };
            // Stop-hook contract: block the stop, feed `reason` as next input.
            let payload = serde_json::json!({ "decision": "block", "reason": context });
            println!("{payload}");
        }
        driver::Decision::Halt { reason } => {
            driver::State::reset();
            eprintln!("{} [{}] driver yields: {reason}", k.emoji, k.name);
        }
        driver::Decision::Escalate { packet } => {
            driver::State::reset();
            eprintln!("{} [{}] ESCALATE → {packet}", k.emoji, k.name);
        }
    }
    Ok(())
}

/// T6: PreToolUse — run kittenscrew checks first (blocked commands etc.),
/// then delegate compression to squeez pretooluse.sh.
fn pre_tool(stdin: &str) -> Result<(), KittenError> {
    // 0. T57 plan-gate: no plan → no work. Block product-code writes until a
    //    plan store exists, with a reason that routes the agent to planning.
    let cfg = config::load().unwrap_or_default();
    if cfg.gate.enforce_plan && !gate::plan_exists() {
        let (tool, path) = tool_target(stdin);
        if gate::blocks(&tool, &path) {
            deny_pre_tool(gate::PLAN_STEER);
            return Ok(());
        }
    }
    // 1. Kittenscrew-specific: validate against blocked commands (T15 will load config).
    if let Some(block_reason) = check_blocked(stdin) {
        // Emit block decision JSON for Claude Code.
        deny_pre_tool(&format!("kittenscrew blocked: {block_reason}"));
        return Ok(());
    }
    // 2. Delegate compression to squeez hook script.
    if let Some(out) = squeez::run_hook("pre-tool", stdin)? {
        if !out.trim().is_empty() {
            print!("{out}");
        }
    }
    Ok(())
}

/// T7+T54: PostToolUse — apply the compression POLICY (V32), then delegate the
/// WORK to squeez (V10). kittenscrew classifies the tool's output and, when the
/// policy puts that content-class on the lossless floor (`off` — JSON, errors,
/// diffs), it SKIPS squeez's output rewrite so structured/actionable output is
/// never mangled (the exact failure mode V32 exists to prevent), while still
/// running squeez's telemetry. For prose/dumps it runs the full squeez hook.
fn post_tool(stdin: &str) -> Result<(), KittenError> {
    let cfg = config::load().unwrap_or_default().compression;
    let (tool, content) = tool_result(stdin);
    let class = compression::classify_content(&content);
    let level = compression::level_for(&cfg, class);

    if level == "off" {
        // Lossless floor: keep the output verbatim, telemetry only (⊥ rewrite).
        if let Some(t) = tool {
            let _ = squeez::run(&["track-result", &t], stdin);
        }
        return Ok(());
    }

    // Compressible class: full squeez post-tool hook (track + compress + track).
    if let Some(out) = squeez::run_hook("post-tool", stdin)? {
        if !out.trim().is_empty() {
            print!("{out}");
        }
    }
    Ok(())
}

/// Extract `(tool_name, output_content)` from a PostToolUse hook payload. Best
/// effort — a shape we don't recognize yields `(None, "")` → classified as
/// prose (compressible), the safe-for-telemetry default.
fn tool_result(stdin: &str) -> (Option<String>, String) {
    let v: serde_json::Value = match serde_json::from_str(stdin) {
        Ok(v) => v,
        Err(_) => return (None, String::new()),
    };
    let tool = v
        .get("tool_name")
        .and_then(|t| t.as_str())
        .map(str::to_owned);
    let content = match v.get("tool_result").or_else(|| v.get("tool_response")) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(o)) => {
            o.get("content").map(|c| c.to_string()).unwrap_or_default()
        }
        Some(other) => other.to_string(),
        None => String::new(),
    };
    (tool, content)
}

/// Extract `(tool_name, file_path)` from a PreToolUse payload (for the gate).
fn tool_target(stdin: &str) -> (String, String) {
    let v: serde_json::Value = serde_json::from_str(stdin).unwrap_or_default();
    let tool = v
        .get("tool_name")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_owned();
    let path = v
        .get("tool_input")
        .and_then(|i| i.get("file_path"))
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_owned();
    (tool, path)
}

/// Emit the PreToolUse deny decision (Claude Code routes the reason back).
fn deny_pre_tool(reason: &str) {
    let payload = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    });
    println!("{payload}");
}

/// T8: PreCompact — delegate to squeez + snapshot plan.
fn pre_compact(stdin: &str) -> Result<(), KittenError> {
    let _ = squeez::run_hook("pre-compact", stdin);
    // Snapshot plan to .kittenscrew/plan.json — T13 will read this on resume.
    if let Err(e) = snapshot_plan() {
        eprintln!("kittenscrew: warn: plan snapshot failed: {e}");
    }
    Ok(())
}

fn snapshot_plan() -> std::io::Result<()> {
    let dir = std::path::PathBuf::from(".kittenscrew");
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::fs::write(
        dir.join("plan.json"),
        format!("{{\"pre_compact_ts\":{stamp}}}\n"),
    )?;
    Ok(())
}

/// Check stdin JSON against blocked commands list. Stub for T15.
/// Currently empty (T15 will load config from kittenscrew.toml).
fn check_blocked(_stdin: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: store::Status) -> store::Task {
        store::Task {
            id: id.into(),
            status,
            ..Default::default()
        }
    }

    #[test]
    fn counts_done_over_non_killed_total() {
        let store = store::Store {
            tasks: vec![
                task("T1", store::Status::Done),
                task("T2", store::Status::Wip),
                task("T3", store::Status::Todo),
                task("T4", store::Status::Killed), // excluded from total
            ],
            ..Default::default()
        };
        let line = helper_narration(&store, Some(("T2", "init the engine")));
        assert!(line.starts_with("1/3 tasks done · "), "got: {line}");
        assert!(line.contains("next: T2 init the engine"), "got: {line}");
    }

    #[test]
    fn long_task_text_is_sliced_with_ellipsis() {
        let store = store::Store {
            tasks: vec![task("T1", store::Status::Todo)],
            ..Default::default()
        };
        let long = "a".repeat(NARRATE_SLICE + 10);
        let line = helper_narration(&store, Some(("T1", &long)));
        let expected_slice = "a".repeat(NARRATE_SLICE);
        assert!(
            line.contains(&format!("next: T1 {expected_slice} …")),
            "got: {line}"
        );
    }

    #[test]
    fn no_next_all_done_says_plan_complete() {
        let store = store::Store {
            tasks: vec![
                task("T1", store::Status::Done),
                task("T2", store::Status::Killed),
            ],
            ..Default::default()
        };
        let line = helper_narration(&store, None);
        assert_eq!(line, "1/1 tasks done · plan complete");
    }

    #[test]
    fn no_next_with_pending_says_no_ready_task() {
        let store = store::Store {
            tasks: vec![
                task("T1", store::Status::Done),
                task("T2", store::Status::Todo), // pending but frontier blocked
            ],
            ..Default::default()
        };
        let line = helper_narration(&store, None);
        assert_eq!(line, "1/2 tasks done · no ready task");
    }
}
