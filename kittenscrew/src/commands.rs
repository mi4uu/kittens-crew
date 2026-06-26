//! Command dispatch (`run`) + every subcommand handler and its shared helpers.

use crate::cli::{
    CheckAction, Cli, Cmd, CompressionAction, ConfigAction, DocsAction, KittyAction, PlanAction,
    SpecAction,
};
use crate::error::KittenError;
use crate::{
    check, compression, config, docs, drift, hook, init, kitty, plan, score, spec, squeez, store,
};
use std::io::Read;

pub(crate) fn run(cli: Cli) -> Result<(), KittenError> {
    match cli.cmd {
        Cmd::Kitty { action } => match action {
            KittyAction::Says {
                kitty,
                message,
                frame,
            } => kitty_says(&kitty, &message, frame.as_deref()),
            KittyAction::List => kitty_list(),
        },
        Cmd::Spec { action } => spec_cmd(action),
        Cmd::Plan { action } => plan_cmd(action),
        Cmd::Check { action } => check_cmd(action),
        Cmd::Score => score_cmd(),
        Cmd::Run {
            store,
            driver,
            model,
            parallel,
            yolo,
            budget,
            rollback_on_fail,
            max_iters,
            max_retries,
        } => run_cmd(store, driver, model, parallel, yolo, budget, rollback_on_fail, max_iters, max_retries),
        Cmd::Bench {
            store,
            k,
            max_iters,
            max_retries,
        } => bench_cmd(store, k, max_iters, max_retries),
        Cmd::Config { action } => config_cmd(action),
        Cmd::Compression { action } => compression_cmd(action),
        Cmd::Docs { action } => docs_cmd(action),
        Cmd::Hook { event } => hook::dispatch(&event),
        Cmd::Init {
            target,
            dry_run,
            force,
        } => init_cmd(target, dry_run, force),
    }
}

fn kitty_says(kitty: &str, message: &str, frame: Option<&str>) -> Result<(), KittenError> {
    let k = kitty::lookup(kitty)
        .ok_or_else(|| KittenError::Validation(format!("unknown kitty: {kitty}")))?;
    // V5: role-coloured frame + sentiment emotion + role emoji + [Name] + raw message.
    // `--box [style]` wraps it in a comic speech-box instead of the one-line bar.
    match frame {
        Some(style) => println!("{}", kitty::boxed(k, message, style)),
        None => println!("{}", kitty::say(k, message)),
    }
    Ok(())
}

fn kitty_list() -> Result<(), KittenError> {
    println!("id|emoji|name|role");
    for k in kitty::all() {
        println!("{}|{}|{}|{}", k.id, k.emoji, k.name, k.role);
    }
    Ok(())
}

pub(crate) const SPEC_PATH: &str = "SPEC.md";

/// T47/V30: a render-triggering cmd must not clobber a pending manual SPEC.md
/// edit. If the on-disk file diverges from the store projection, abort and tell
/// the caller to reconcile via `spec drift --apply` first.
pub(crate) fn ensure_synced(store: &store::Store) -> Result<(), KittenError> {
    let on_disk = std::fs::read_to_string(SPEC_PATH).unwrap_or_default();
    if !on_disk.is_empty() && !spec::is_synced(store, &on_disk) {
        return Err(KittenError::Validation(
            "SPEC.md diverges from store (manual edit?) — run `kittenscrew spec drift --apply` to reconcile first"
                .into(),
        ));
    }
    Ok(())
}

fn spec_cmd(action: SpecAction) -> Result<(), KittenError> {
    use std::path::Path;
    match action {
        SpecAction::Read { section, plain } => {
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            let md = spec::render(&s);
            let out = match section {
                Some(sel) => {
                    let letter = sel.trim_start_matches('§').chars().next().unwrap_or(' ');
                    spec::section(&md, letter)
                        .ok_or_else(|| KittenError::Validation(format!("no section §{letter}")))?
                }
                None => md,
            };
            print!("{}", if plain { spec::expand(&out) } else { out });
            Ok(())
        }
        SpecAction::Apply => {
            let store_path = Path::new(store::STORE_PATH);
            let mut s = store::Store::load(store_path)?;
            ensure_synced(&s)?; // T47: don't clobber a pending manual SPEC.md edit
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            // Accept a single diff object or an array of diffs (batch).
            let trimmed = input.trim();
            let diffs: Vec<spec::Diff> = if trimmed.starts_with('[') {
                serde_json::from_str(trimmed)
                    .map_err(|e| KittenError::Validation(format!("bad diff JSON: {e}")))?
            } else {
                vec![serde_json::from_str(trimmed)
                    .map_err(|e| KittenError::Validation(format!("bad diff JSON: {e}")))?]
            };
            for d in &diffs {
                spec::apply(&mut s, d).map_err(KittenError::Validation)?;
            }
            // V3: never write a spec that breaks a structural §V rule.
            let violations = spec::validate(&s);
            let k = kitty::lookup("planning").expect("planning kitty");
            if !violations.is_empty() {
                for v in &violations {
                    eprintln!("{}", kitty::say(k, v));
                }
                // Return the rejected diff to the caller (LLM) to fix + resubmit.
                eprintln!("{}", kitty::say(k, "diff rejected — SPEC.md unchanged:"));
                println!("{trimmed}");
                return Err(KittenError::Validation(format!(
                    "{} §V violation(s) — not written",
                    violations.len()
                )));
            }
            s.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
            println!(
                "{}",
                kitty::say(
                    k,
                    &format!(
                        "applied {} diff(s) → SPEC.md ({} tasks)",
                        diffs.len(),
                        s.tasks.len()
                    )
                )
            );
            Ok(())
        }
        SpecAction::Gen { goal, store, model, max_retries } => {
            gen_cmd(goal, store, model, max_retries)
        }
        SpecAction::Check => {
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            let violations = spec::validate(&s);
            let k = kitty::lookup("entropy").expect("entropy kitty");
            if violations.is_empty() {
                println!(
                    "{}",
                    kitty::say(
                        k,
                        &format!("spec clean — {} tasks, no violations", s.tasks.len())
                    )
                );
                Ok(())
            } else {
                for v in &violations {
                    println!("{}", kitty::say(k, v));
                }
                Err(KittenError::Validation(format!(
                    "{} violation(s)",
                    violations.len()
                )))
            }
        }
        SpecAction::Import => {
            let md = std::fs::read_to_string(SPEC_PATH)?;
            let mut s = spec::import(&md)?;
            // T46: SPEC.md can't carry toml-only fields (value/difficulty/risk/
            // priority/scope/eval). Re-import would drop them → merge from the
            // existing store by id so they survive the round-trip.
            let store_path = Path::new(store::STORE_PATH);
            if let Ok(old) = store::Store::load(store_path) {
                for t in s.tasks.iter_mut() {
                    if let Some(prev) = old.tasks.iter().find(|o| o.id == t.id) {
                        drift::carry_toml_only(t, prev);
                    }
                }
            }
            s.save(store_path)?;
            let k = kitty::lookup("planning").expect("planning kitty");
            println!(
                "{}",
                kitty::say(
                    k,
                    &format!(
                        "imported → {} ({} tasks, {} invariants, {} bugs)",
                        store::STORE_PATH,
                        s.tasks.len(),
                        s.invariants.len(),
                        s.bugs.len()
                    )
                )
            );
            Ok(())
        }
        SpecAction::Render => {
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
            let k = kitty::lookup("planning").expect("planning kitty");
            println!(
                "{}",
                kitty::say(k, &format!("rendered {SPEC_PATH} from store"))
            );
            Ok(())
        }
        SpecAction::Drift { apply } => {
            let store_path = Path::new(store::STORE_PATH);
            let current = store::Store::load(store_path)?;
            let incoming = spec::import(&std::fs::read_to_string(SPEC_PATH)?)?;
            let d = drift::diff(&current, &incoming);
            let k = kitty::lookup("entropy").expect("entropy kitty");

            if d.is_empty() {
                println!("{}", kitty::say(k, "no drift — SPEC.md ≡ store"));
                return Ok(());
            }
            // Structured summary (V16): structural auto-reconcilable, prose escalates.
            println!("{}", serde_json::to_string_pretty(&d).unwrap());
            if !d.prose_changed.is_empty() {
                println!(
                    "{}",
                    kitty::say(
                        k,
                        &format!(
                            "prose drift in {} → review (adopted from SPEC.md, not silent)",
                            d.prose_changed.join(",")
                        )
                    )
                );
            }
            if !apply {
                println!(
                    "{}",
                    kitty::say(k, "dry-run — rerun w/ --apply to reconcile")
                );
                return Ok(());
            }
            let merged = drift::reconcile(&current, &incoming);
            let violations = spec::validate(&merged);
            if !violations.is_empty() {
                for v in &violations {
                    eprintln!("{}", kitty::say(k, v));
                }
                return Err(KittenError::Validation(format!(
                    "{} §V violation(s) — store unchanged",
                    violations.len()
                )));
            }
            merged.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&merged))?;
            println!(
                "{}",
                kitty::say(
                    k,
                    &format!(
                        "reconciled → store + SPEC.md re-rendered ({} task change(s))",
                        d.task_added.len() + d.task_removed.len() + d.task_changed.len()
                    )
                )
            );
            Ok(())
        }
    }
}

/// Build worth knobs from `[plan]` config; malformed/absent config → defaults.
pub(crate) fn worth_params() -> plan::WorthParams {
    let cfg = config::load().unwrap_or_default().plan;
    plan::WorthParams {
        gamma: cfg.discount,
        portfolio_w: cfg.portfolio_weight,
        agg: match cfg.forward_agg.as_str() {
            "max" => plan::Agg::Max,
            "sum" => plan::Agg::Sum,
            _ => plan::Agg::Hybrid,
        },
        rank_by: match cfg.rank_by.as_str() {
            "worth" => plan::RankBy::Worth,
            "roi" => plan::RankBy::Roi,
            _ => plan::RankBy::Expected,
        },
    }
}

fn plan_cmd(action: PlanAction) -> Result<(), KittenError> {
    use std::path::Path;
    let store_path = Path::new(store::STORE_PATH);
    let s = store::Store::load(store_path)?;
    let wp = worth_params(); // T41: [plan] config knobs (defaults if absent)
    let json = |v: &serde_json::Value| println!("{}", serde_json::to_string_pretty(v).unwrap());
    match action {
        PlanAction::Resolve => match plan::topo(&s) {
            Ok(order) => {
                json(&serde_json::json!({ "order": order }));
                Ok(())
            }
            Err(cycle) => {
                json(&serde_json::json!({ "cycle": cycle }));
                Err(KittenError::Validation("cycle in plan DAG".into()))
            }
        },
        PlanAction::Ready => {
            let ids: Vec<&str> = plan::ready(&s).iter().map(|t| t.id.as_str()).collect();
            json(&serde_json::json!({ "ready": ids }));
            Ok(())
        }
        PlanAction::Next => {
            match plan::next_with(&s, &wp) {
                Some(t) => json(&serde_json::json!({ "next": t.id, "task": t.task })),
                None => json(&serde_json::json!({ "next": null })),
            }
            Ok(())
        }
        PlanAction::Blocking { id } => {
            json(&serde_json::json!({ "blocking": plan::blocking(&s, &id) }));
            Ok(())
        }
        PlanAction::Impact { id } => {
            let i = plan::impact(&s, &id);
            println!("{}", serde_json::to_string_pretty(&i).unwrap());
            Ok(())
        }
        PlanAction::Path { goal } => {
            let p = plan::critical_path(&s, goal.as_deref());
            json(&serde_json::json!({ "path": p, "length": p.len() }));
            Ok(())
        }
        PlanAction::Alternatives => {
            let a = plan::alternatives_with(&s, &wp);
            println!("{}", serde_json::to_string_pretty(&a).unwrap());
            Ok(())
        }
        PlanAction::Worth => {
            let rows = plan::worth_ranking_with(&s, &wp);
            println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            Ok(())
        }
        PlanAction::Graph => {
            println!("{}", plan::graph(&s));
            Ok(())
        }
        PlanAction::Done { id } => {
            ensure_synced(&s)?; // T47: don't clobber a pending manual SPEC.md edit
            let mut s = s;
            let t = s
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| KittenError::Validation(format!("unknown task {id}")))?;
            t.status = store::Status::Done;
            s.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
            let k = kitty::lookup("builder").expect("builder kitty");
            println!(
                "{}",
                kitty::say(k, &format!("{id} → done; SPEC.md re-rendered"))
            );
            Ok(())
        }
    }
}

/// T17 — spec-from-prose: a model turns a plain-language goal into a validated DAG of
/// build tasks. The planner half of "describe it → working program" (`gen` plans, `run`
/// builds). Reuses the exact apply+validate path as `spec apply`; on a §V violation the
/// errors are fed back to the model to fix and resubmit (bounded).
fn gen_cmd(
    goal: String,
    store: Option<std::path::PathBuf>,
    model: Option<String>,
    max_retries: u32,
) -> Result<(), KittenError> {
    use crate::driver::api::{Driver, HttpDriver};
    use crate::driver::drive::extract_code;

    let store_path = store.unwrap_or_else(|| std::path::PathBuf::from(store::STORE_PATH));
    if let Some(p) = store_path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if !store_path.exists() {
        std::fs::write(&store_path, "schema = 1\n")?;
    }

    // Same endpoint policy as run/bench: KITTENSCREW_BASE_URL + --model/KITTENSCREW_MODEL.
    let base = std::env::var("KITTENSCREW_BASE_URL")
        .map_err(|_| KittenError::Validation("set KITTENSCREW_BASE_URL (OpenAI-compatible endpoint)".into()))?;
    let m = model
        .or_else(|| std::env::var("KITTENSCREW_MODEL").ok())
        .unwrap_or_else(|| "default".into());
    let key = std::env::var("KITTENSCREW_API_KEY").unwrap_or_else(|_| "x".into());
    let driver = HttpDriver::openai(&base, m, key);

    let k = kitty::lookup("planning").expect("planning kitty");
    let mut feedback = String::new();
    for attempt in 0..=max_retries {
        let prompt = gen_prompt(&goal, &feedback);
        let res = driver
            .dispatch(&crate::driver::api::Turn { prompt })
            .map_err(|e| KittenError::Validation(format!("model: {e}")))?;
        let json = extract_code(&res.text);

        let diffs: Vec<spec::Diff> = match serde_json::from_str(json.trim()) {
            Ok(d) => d,
            Err(e) => {
                feedback = format!("Your output was not a valid JSON array of diffs: {e}. Output ONLY the JSON array.");
                println!("{}", kitty::say(k, &format!("attempt {}: bad JSON, retrying", attempt + 1)));
                continue;
            }
        };

        // Lenient section coercion: gen ONLY ever emits build tasks (§T). Weaker models
        // (lfm2, ornith) mislabel the section as §1/§2/etc and `spec::apply` rejects them,
        // burning every retry on a token typo. Force every gen diff to §T — there is no
        // other valid target here, so this can't mask a real mistake.
        let diffs: Vec<spec::Diff> = diffs
            .into_iter()
            .map(|mut d| {
                d.section = "§T".into();
                d
            })
            .collect();

        // Apply onto a fresh load each attempt so a failed try doesn't accumulate.
        let mut s = store::Store::load(&store_path)?;
        let mut apply_err = None;
        for d in &diffs {
            if let Err(e) = spec::apply(&mut s, d) {
                apply_err = Some(e);
                break;
            }
        }
        if let Some(e) = apply_err {
            feedback = format!("A diff was rejected: {e}. Fix it and resubmit the full array.");
            println!("{}", kitty::say(k, &format!("attempt {}: {}", attempt + 1, feedback)));
            continue;
        }

        let violations = spec::validate(&s);
        if !violations.is_empty() {
            feedback = format!("§V violations to fix: {}", violations.join("; "));
            println!("{}", kitty::say(k, &format!("attempt {}: {}", attempt + 1, feedback)));
            continue;
        }

        s.save(&store_path)?;
        if store_path == std::path::Path::new(store::STORE_PATH) {
            std::fs::write(SPEC_PATH, spec::render(&s))?;
        }
        println!(
            "{}",
            kitty::say(k, &format!("planned {} task(s) → {}", diffs.len(), store_path.display()))
        );
        return Ok(());
    }
    Err(KittenError::Validation(format!(
        "could not produce a valid plan in {} attempt(s)",
        max_retries + 1
    )))
}

/// The planner prompt: prose goal → a JSON array of `spec apply` diffs (one §T add per
/// code leaf). `feedback` carries the prior attempt's validation errors (empty first try).
fn gen_prompt(goal: &str, feedback: &str) -> String {
    let retry = if feedback.is_empty() {
        String::new()
    } else {
        format!("\n\nYOUR PREVIOUS ATTEMPT FAILED: {feedback}\nFix it and output the corrected array.")
    };
    format!(
        "You are a build planner. Turn the GOAL into a DAG of small build tasks, one task per \
         Rust source file (a 'leaf' the builder will fill). Output ONLY a JSON array, no prose, \
         in a single ```json fenced block.\n\n\
         Each array element is:\n\
         {{\"section\":\"§T\",\"op\":\"add\",\"payload\":{{\"id\":\"T1\",\"task\":\"<one clear sentence of what this file must contain>\",\"deps\":[],\"scope\":[\"<relative_file.rs>\"],\"priority\":1}}}}\n\n\
         Rules: ids are unique T1,T2,...; deps reference earlier ids that must build first (a real \
         dependency DAG, no cycles); each scope is exactly ONE relative .rs path; keep it MINIMAL — \
         the fewest leaves that satisfy the goal. Each task sentence must be self-contained so the \
         builder needs only that one sentence.\n\n\
         STRONGLY PREFER A SINGLE FILE (`main.rs`) — a small program is ONE leaf. Split into more \
         files ONLY when the goal is genuinely large. If you DO split: exactly one file is the crate \
         root `main.rs`, it contains `fn main`, and its task says it declares `mod <name>;` for every \
         other file (so the program links as one binary). Other files expose `pub` items only.\n\n\
         GOAL: {goal}{retry}"
    )
}

/// T62/T65 — drive the DAG: fill ready code leaves via Codestral, verify each
/// compiles, advance. Minimal front door for the harness (full flag surface — yolo,
/// budget, driver selection — is the rest of T65/T64/T70).
#[allow(clippy::too_many_arguments)]
fn run_cmd(
    store: Option<std::path::PathBuf>,
    driver: String,
    model: Option<String>,
    parallel: bool,
    yolo: bool,
    budget: Option<u64>,
    rollback_on_fail: bool,
    max_iters: u32,
    max_retries: u32,
) -> Result<(), KittenError> {
    use crate::driver::api::{Driver, HttpDriver, RigDriver};
    use crate::driver::delegation::drive_parallel;
    use crate::driver::drive::{drive, DriveOpts, Outcome};

    let k = kitty::lookup("builder").expect("builder kitty");
    let opts = DriveOpts {
        max_iters,
        max_retries,
        store_path: store.unwrap_or_else(|| std::path::PathBuf::from(store::STORE_PATH)),
        // Safe by default (T90): confine all scope writes to the project dir and below.
        workspace_root: Some(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))),
    };

    // P1.5 safety net: snapshot the working tree before driving so a halted run can be undone.
    let snap = if rollback_on_fail {
        opts.workspace_root
            .as_ref()
            .and_then(|r| crate::driver::snapshot::snapshot(r).ok())
    } else {
        None
    };

    // YOLO (T64) and budget (T70) modules exist standalone; wiring their enforcement
    // INTO the drive loop is a follow-up (drive() takes neither yet). Surface intent
    // honestly rather than silently ignoring the flags.
    if yolo {
        println!("{}", kitty::say(k, "yolo — tripwire gate (T64) is the only guard; in-loop wiring pending"));
    }
    if let Some(cap) = budget {
        println!("{}", kitty::say(k, &format!("budget ~{cap} tokens noted (T70); in-loop enforcement pending")));
    }

    // Serial (T62) vs scope-disjoint parallel (T77), generic over the concrete Driver.
    // Each advanced node is both narrated to the user AND appended as a liveness event
    // (T68) to `.kittenscrew/events.jsonl` — the one-line-per-event feed the fzf TUI
    // (T89) tails. Watch a live run with:
    //   tail -f .kittenscrew/events.jsonl | fzf --tail=1000 --preview 'echo {}'
    fn go<D: Driver + Sync>(d: &D, opts: &DriveOpts, parallel: bool, k: &kitty::Kitty) -> Result<Outcome, String> {
        use crate::driver::status::{Heartbeat, Liveness, Reporter};
        let prog = |id: &str, model: &str| {
            println!("{}", kitty::say(k, &format!("{id} → {model}")));
            // A "✗ "-prefixed model slot is a failed/blocked node (drive() convention).
            let (status, detail) = if model.starts_with('✗') {
                (Liveness::Blocked, model.to_string())
            } else {
                (Liveness::Done, format!("done ({model})"))
            };
            if let Ok(f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(".kittenscrew/events.jsonl")
            {
                let _ = Reporter::new(f).report(&Heartbeat { status, node: id.to_string(), detail });
            }
        };
        if parallel {
            drive_parallel(d, opts, prog)
        } else {
            drive(d, opts, prog)
        }
    }

    let outcome = match driver.as_str() {
        "claude-code" => {
            return Err(KittenError::Validation(
                "claude-code driver not built yet (T71, tmux backend) — use --driver api".into(),
            ))
        }
        "api" => match std::env::var("KITTENSCREW_BASE_URL").ok() {
            // Endpoint override (same as bench): target ANY OpenAI-compatible endpoint —
            // LM Studio (:1234), ollama (:11434), openrouter — via the known-good
            // HttpDriver (rig 0.39 500s on real endpoints). Model = --model or KITTENSCREW_MODEL.
            Some(base) => {
                let m = model
                    .clone()
                    .or_else(|| std::env::var("KITTENSCREW_MODEL").ok())
                    .unwrap_or_else(|| "default".into());
                let key = std::env::var("KITTENSCREW_API_KEY").unwrap_or_else(|_| "x".into());
                let d = HttpDriver::openai(&base, m, key);
                go(&d, &opts, parallel, k)
            }
            // No override: a chosen --model routes through rig (T61); else codestral.
            None => match model {
                Some(m) => {
                    let d = RigDriver::from_env(
                        Some("https://codestral.mistral.ai/v1"),
                        m,
                        "CODESTRAL_API_KEY",
                    )
                    .map_err(|e| KittenError::Validation(format!("driver: {e}")))?;
                    go(&d, &opts, parallel, k)
                }
                None => {
                    let d = HttpDriver::codestral()
                        .map_err(|e| KittenError::Validation(format!("driver: {e}")))?;
                    go(&d, &opts, parallel, k)
                }
            },
        },
        other => {
            return Err(KittenError::Validation(format!(
                "unknown driver `{other}` — expected `api` or `claude-code`"
            )))
        }
    }
    .map_err(KittenError::Validation)?;

    // P1.5: if the run halted and a snapshot was taken, restore the pre-run tree.
    if let (Some(snap), Some(root)) = (&snap, opts.workspace_root.as_ref()) {
        if matches!(outcome, Outcome::Halted { .. }) {
            match crate::driver::snapshot::rollback(root, snap) {
                Ok(()) => println!("{}", kitty::say(k, "run halted — rolled back to the pre-run snapshot")),
                Err(e) => println!("{}", kitty::say(k, &format!("rollback failed: {e}"))),
            }
        }
    }

    let msg = match outcome {
        Outcome::Converged { done } => {
            format!("converged — {done} node(s) green, frontier empty")
        }
        Outcome::CapReached { done } => {
            format!("iteration cap hit — {done} node(s) done, work remains")
        }
        Outcome::Halted { node, reason, done } => {
            format!("halted at {node}: {reason} ({done} done before stop)")
        }
    };
    println!("{}", kitty::say(k, &msg));
    Ok(())
}

/// T75 — A/B benchmark: bare baseline vs kittenscrew on the same model/store, print
/// the delta that is the harness's actual weight.
fn bench_cmd(
    store: std::path::PathBuf,
    k: u32,
    max_iters: u32,
    max_retries: u32,
) -> Result<(), KittenError> {
    use crate::driver::api::{Driver, HttpDriver};
    use crate::driver::bench::{bench, BenchOpts};

    // Endpoint override: point the bench at ANY OpenAI-compatible endpoint via env —
    // Codestral, OpenRouter direct, LM Studio (:1234) or ollama (:11434) — else the
    // proven codestral driver. Uses the simple known-good HttpDriver (manual OpenAI
    // request), NOT rig: a real run against LM Studio showed rig 0.39's agent path
    // 500s where a plain /chat/completions POST succeeds.
    //   KITTENSCREW_BASE_URL=http://localhost:1234/v1 KITTENSCREW_MODEL=qwen/qwen3.6-27b \
    //   KITTENSCREW_API_KEY=lmstudio kittenscrew bench --store toy.toml --k 1
    let driver: Box<dyn Driver> = match (
        std::env::var("KITTENSCREW_BASE_URL").ok(),
        std::env::var("KITTENSCREW_MODEL").ok(),
    ) {
        (Some(base), Some(model)) => {
            let key = std::env::var("KITTENSCREW_API_KEY").unwrap_or_else(|_| "x".into());
            Box::new(HttpDriver::openai(&base, model, key))
        }
        _ => Box::new(
            HttpDriver::codestral()
                .map_err(|e| KittenError::Validation(format!("driver: {e}")))?,
        ),
    };
    let model_label = driver.model().to_string();
    let rep = bench(
        driver.as_ref(),
        &BenchOpts {
            store_path: store,
            k,
            max_iters,
            max_retries,
        },
    )
    .map_err(KittenError::Validation)?;

    let n = rep.nodes;
    let yn = |b: bool| if b { "yes" } else { "no" };
    let body = format!(
        "A/B over {k} trial(s), {n} node(s) — same model ({model_label}), only the harness differs\n\
         bare      : pass^1={:>3}  full-rate={:>3.0}%  mean-green={:.2}/{n}\n\
         kittenscrw: pass^1={:>3}  full-rate={:>3.0}%  mean-green={:.2}/{n}\n\
         DELTA     : full-rate {:+.0}pp   mean-green {:+.2}",
        yn(rep.bare.pass_1()),
        rep.bare.full_rate() * 100.0,
        rep.bare.mean_green(),
        yn(rep.harness.pass_1()),
        rep.harness.full_rate() * 100.0,
        rep.harness.mean_green(),
        (rep.harness.full_rate() - rep.bare.full_rate()) * 100.0,
        rep.harness.mean_green() - rep.bare.mean_green(),
    );
    let kit = kitty::lookup("builder").expect("builder kitty");
    println!("{}", kitty::say(kit, &body));
    Ok(())
}

fn check_cmd(action: CheckAction) -> Result<(), KittenError> {
    use std::path::Path;
    match action {
        CheckAction::Variance => {
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            let cfg = config::load().unwrap_or_default().audit;
            let rows = check::value_variance(&s, cfg.variance_threshold);
            let k = kitty::lookup("memory").expect("memory kitty");
            println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            let flagged: Vec<&str> = rows
                .iter()
                .filter(|r| r.flagged)
                .map(|r| r.id.as_str())
                .collect();
            if flagged.is_empty() {
                println!(
                    "{}",
                    kitty::say(
                        k,
                        &format!(
                            "value-variance ok — {} eval'd task(s) within ±{}",
                            rows.len(),
                            cfg.variance_threshold
                        )
                    )
                );
                return Ok(());
            }
            println!(
                "{}",
                kitty::say(
                    k,
                    &format!(
                        "variance flagged: {} → on_variance={}",
                        flagged.join(","),
                        cfg.on_variance
                    )
                )
            );
            // V25/V27: halt is a hard stop; brainstorm/report just surface it.
            if cfg.on_variance == "halt" {
                return Err(KittenError::Validation(format!(
                    "{} task(s) past variance threshold — halt",
                    flagged.len()
                )));
            }
            Ok(())
        }
        CheckAction::Done => {
            let store_path = Path::new(store::STORE_PATH);
            let mut s = store::Store::load(store_path)?;
            ensure_synced(&s)?; // T47: a demote re-renders — don't clobber manual edits
            let reports = check::check_done(&s);
            let k = kitty::lookup("entropy").expect("entropy kitty");

            // V19: a sealed `x` going red is the regression alarm — demote + report, never silent.
            let failed: Vec<&check::TaskReport> = reports.iter().filter(|r| !r.ok).collect();
            for r in &reports {
                if r.ok {
                    println!("{}", kitty::say(k, &format!("{} ok", r.id)));
                } else {
                    let cites: String = if r.broken_cites.is_empty() {
                        "-".into()
                    } else {
                        r.broken_cites.join(",")
                    };
                    println!(
                        "{}",
                        kitty::say(
                            k,
                            &format!(
                                "{} FAIL → demote x→~ ({} marker(s), broken cites: {})",
                                r.id,
                                r.markers.len(),
                                cites
                            )
                        )
                    );
                    for m in &r.markers {
                        println!("    {}:{} [{}] {}", m.file, m.line, m.kind, m.text);
                    }
                }
            }

            if failed.is_empty() {
                println!(
                    "{}",
                    kitty::say(
                        k,
                        &format!(
                            "all {} done task(s) verified — no fake delivery",
                            reports.len()
                        )
                    )
                );
                return Ok(());
            }

            let demote: Vec<String> = failed.iter().map(|r| r.id.clone()).collect();
            for t in s.tasks.iter_mut() {
                if demote.contains(&t.id) {
                    t.status = store::Status::Wip;
                }
            }
            s.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
            Err(KittenError::Validation(format!(
                "{} task(s) demoted x→~: {}",
                demote.len(),
                demote.join(",")
            )))
        }
    }
}

/// All CLI subcommand paths (clap introspection) — e.g. "spec apply", "plan next".
fn binary_cmds() -> std::collections::HashSet<String> {
    use clap::CommandFactory;
    fn walk(cmd: &clap::Command, prefix: &str, out: &mut std::collections::HashSet<String>) {
        for sub in cmd.get_subcommands() {
            let path = if prefix.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{prefix} {}", sub.get_name())
            };
            walk(sub, &path, out);
            out.insert(path);
        }
    }
    let mut out = std::collections::HashSet::new();
    walk(&Cli::command(), "", &mut out);
    out
}

fn score_cmd() -> Result<(), KittenError> {
    use std::path::Path;
    let s = store::Store::load(Path::new(store::STORE_PATH))?;
    let on_disk = std::fs::read_to_string(SPEC_PATH).unwrap_or_default();
    let synced = on_disk.is_empty() || spec::is_synced(&s, &on_disk);
    let sc = score::conformance(&s, &binary_cmds(), synced);
    println!("{}", serde_json::to_string_pretty(&sc).unwrap());
    Ok(())
}

fn docs_cmd(action: DocsAction) -> Result<(), KittenError> {
    use std::path::Path;
    match action {
        DocsAction::Task { id } => {
            let cfg = config::load().unwrap_or_default().docs;
            let k = kitty::lookup("scribe").unwrap_or_else(|| kitty::lookup("planning").unwrap());
            // V12: ⊥ runs unless opted in. The cmd is the manual trigger; the
            // [docs] auto_generate gate keeps it silent by default.
            if !cfg.auto_generate {
                eprintln!(
                    "{}",
                    kitty::say(
                        k,
                        &format!("docs off ([docs] auto_generate=false) — skipped {id}")
                    )
                );
                return Ok(());
            }
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            let t = s
                .task(&id)
                .ok_or_else(|| KittenError::Validation(format!("unknown task {id}")))?;
            let path = docs::doc_path(t);
            std::fs::create_dir_all("docs")?;
            std::fs::write(&path, docs::render_task_doc(t, &s, &cfg.detail))?;
            println!("{}", kitty::say(k, &format!("wrote {path}")));
            Ok(())
        }
    }
}

fn config_cmd(action: ConfigAction) -> Result<(), KittenError> {
    match action {
        ConfigAction::Show => {
            let c = config::load().map_err(KittenError::Validation)?;
            println!("{}", serde_json::to_string_pretty(&c).unwrap());
            Ok(())
        }
    }
}

/// T49: the compression POLICY (V32). Reads `[compression]` (defaults if absent)
/// and reports the squeez level per content-class. Pure policy — no compression
/// happens here (V10); squeez consumes the level.
fn compression_cmd(action: CompressionAction) -> Result<(), KittenError> {
    let cfg = config::load().map_err(KittenError::Validation)?.compression;
    match action {
        CompressionAction::Policy => {
            let map: std::collections::BTreeMap<&str, &str> =
                compression::policy(&cfg).into_iter().collect();
            println!("{}", serde_json::to_string_pretty(&map).unwrap());
            Ok(())
        }
        CompressionAction::Level { class } => {
            let c = compression::Class::parse(&class).ok_or_else(|| {
                KittenError::Validation(format!(
                    "unknown content-class: {class} (expected prose|dump|structured|diff)"
                ))
            })?;
            println!("{}", compression::level_for(&cfg, c));
            Ok(())
        }
    }
}

/// T16: write `kittenscrew.toml` + register the hook membrane. V6: the squeez
/// gate is checked here (the binary lookup) and passed into `init::run`, which
/// refuses without it (→ exit 3).
fn init_cmd(
    target: Option<std::path::PathBuf>,
    dry_run: bool,
    force: bool,
) -> Result<(), KittenError> {
    let target = target.unwrap_or_else(default_claude_dir);
    let squeez_ok = squeez::bin().is_some();
    let report = init::run(&target, squeez_ok, dry_run, force).map_err(|e| match e {
        init::InitError::SqueezMissing => KittenError::SqueezMissing,
        init::InitError::Io(io) => KittenError::Io(io),
    })?;

    let k = kitty::lookup("orchestrating").expect("orchestrating kitty");
    let tag = if report.dry_run { "[dry-run] " } else { "" };
    let cfg = match (report.dry_run, report.config_written) {
        (true, true) => "would write",
        (true, false) => "would keep",
        (false, true) => "wrote",
        (false, false) => "kept",
    };
    println!(
        "{}",
        kitty::say(
            k,
            &format!(
                "{tag}{cfg} {} · {} membrane event(s) registered, {} already wired → {}",
                report.config_path.display(),
                report.registered.len(),
                report.already.len(),
                report.settings_path.display(),
            )
        )
    );
    Ok(())
}

/// `$HOME/.claude` — where Claude Code reads `settings.json`. Falls back to a
/// relative `.claude` if `$HOME` is unset (rare; keeps init total).
fn default_claude_dir() -> std::path::PathBuf {
    match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h).join(".claude"),
        Err(_) => std::path::PathBuf::from(".claude"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hook, kitty};

    // T55: deterministic task → kitty role assignment for context injection.
    #[test]
    fn for_task_maps_work_to_the_right_kitty() {
        assert_eq!(kitty::for_task("impl the parser").id, "builder");
        assert_eq!(kitty::for_task("write README section").id, "scribe");
        assert_eq!(kitty::for_task("check done on touched scope").id, "entropy");
        assert_eq!(kitty::for_task("fix the regression bug").id, "memory");
        assert_eq!(kitty::for_task("topo-sort the spec DAG").id, "planning");
        // role_hint is emoji + name + role.
        assert!(kitty::role_hint("impl X").contains("Builder Kitty"));
    }

    // T45: the §I-completeness gate has two halves — `score::declared_cmds` +
    // `score::interface_dim` (tested in score.rs) parse/compare, and this proves
    // the clap-introspection half (`binary_cmds`) sees nested subcommands. At
    // runtime `kittenscrew score` joins them against the real spec.
    #[test]
    fn binary_cmds_introspects_nested_subcommands() {
        let cmds = binary_cmds();
        for expected in [
            "spec apply",
            "plan next",
            "plan graph",
            "check done",
            "check variance",
            "config show",
            "kitty says",
        ] {
            assert!(cmds.contains(expected), "binary_cmds missing `{expected}`");
        }
    }

    #[test]
    fn kitty_says_prefixes_output() {
        let k = kitty::lookup("planning").unwrap();
        assert_eq!(k.emoji, "📐");
        assert_eq!(k.name, "Planning Kitty");
    }

    #[test]
    fn kitty_lookup_unknown_returns_none() {
        assert!(kitty::lookup("nonexistent").is_none());
    }

    #[test]
    fn kitty_roster_is_populated() {
        // 6 founding cats + the control-plane crew (helper/explorer/style/grill).
        assert_eq!(kitty::all().len(), 10);
        assert!(kitty::lookup("grill").is_some());
    }

    #[test]
    fn hook_dispatch_rejects_unknown_event() {
        let r = hook::dispatch("bogus-event");
        assert!(matches!(r, Err(KittenError::Validation(_))));
    }

    #[test]
    fn kitenerror_exit_codes_match_v1() {
        assert_eq!(KittenError::Validation("x".into()).exit_code(), 2);
        assert_eq!(KittenError::SqueezMissing.exit_code(), 3);
    }
}
