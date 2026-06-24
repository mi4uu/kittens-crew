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
                    eprintln!("{} [{}] {}", k.emoji, k.name, v);
                }
                // Return the rejected diff to the caller (LLM) to fix + resubmit.
                eprintln!(
                    "{} [{}] diff rejected — SPEC.md unchanged:",
                    k.emoji, k.name
                );
                println!("{trimmed}");
                return Err(KittenError::Validation(format!(
                    "{} §V violation(s) — not written",
                    violations.len()
                )));
            }
            s.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
            println!(
                "{} [{}] applied {} diff(s) → SPEC.md ({} tasks)",
                k.emoji,
                k.name,
                diffs.len(),
                s.tasks.len()
            );
            Ok(())
        }
        SpecAction::Check => {
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            let violations = spec::validate(&s);
            let k = kitty::lookup("entropy").expect("entropy kitty");
            if violations.is_empty() {
                println!(
                    "{} [{}] spec clean — {} tasks, no violations",
                    k.emoji,
                    k.name,
                    s.tasks.len()
                );
                Ok(())
            } else {
                for v in &violations {
                    println!("{} [{}] {}", k.emoji, k.name, v);
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
                "{} [{}] imported → {} ({} tasks, {} invariants, {} bugs)",
                k.emoji,
                k.name,
                store::STORE_PATH,
                s.tasks.len(),
                s.invariants.len(),
                s.bugs.len()
            );
            Ok(())
        }
        SpecAction::Render => {
            let s = store::Store::load(Path::new(store::STORE_PATH))?;
            std::fs::write(SPEC_PATH, spec::render(&s))?;
            let k = kitty::lookup("planning").expect("planning kitty");
            println!("{} [{}] rendered {} from store", k.emoji, k.name, SPEC_PATH);
            Ok(())
        }
        SpecAction::Drift { apply } => {
            let store_path = Path::new(store::STORE_PATH);
            let current = store::Store::load(store_path)?;
            let incoming = spec::import(&std::fs::read_to_string(SPEC_PATH)?)?;
            let d = drift::diff(&current, &incoming);
            let k = kitty::lookup("entropy").expect("entropy kitty");

            if d.is_empty() {
                println!("{} [{}] no drift — SPEC.md ≡ store", k.emoji, k.name);
                return Ok(());
            }
            // Structured summary (V16): structural auto-reconcilable, prose escalates.
            println!("{}", serde_json::to_string_pretty(&d).unwrap());
            if !d.prose_changed.is_empty() {
                println!(
                    "{} [{}] prose drift in {} → review (adopted from SPEC.md, not silent)",
                    k.emoji,
                    k.name,
                    d.prose_changed.join(",")
                );
            }
            if !apply {
                println!(
                    "{} [{}] dry-run — rerun w/ --apply to reconcile",
                    k.emoji, k.name
                );
                return Ok(());
            }
            let merged = drift::reconcile(&current, &incoming);
            let violations = spec::validate(&merged);
            if !violations.is_empty() {
                for v in &violations {
                    eprintln!("{} [{}] {}", k.emoji, k.name, v);
                }
                return Err(KittenError::Validation(format!(
                    "{} §V violation(s) — store unchanged",
                    violations.len()
                )));
            }
            merged.save(store_path)?;
            std::fs::write(SPEC_PATH, spec::render(&merged))?;
            println!(
                "{} [{}] reconciled → store + SPEC.md re-rendered ({} task change(s))",
                k.emoji,
                k.name,
                d.task_added.len() + d.task_removed.len() + d.task_changed.len()
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
            println!("{} [{}] {id} → done; SPEC.md re-rendered", k.emoji, k.name);
            Ok(())
        }
    }
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
                    "{} [{}] value-variance ok — {} eval'd task(s) within ±{}",
                    k.emoji,
                    k.name,
                    rows.len(),
                    cfg.variance_threshold
                );
                return Ok(());
            }
            println!(
                "{} [{}] variance flagged: {} → on_variance={}",
                k.emoji,
                k.name,
                flagged.join(","),
                cfg.on_variance
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
                    println!("{} [{}] {} ok", k.emoji, k.name, r.id);
                } else {
                    println!(
                        "{} [{}] {} FAIL → demote x→~ ({} marker(s), broken cites: {})",
                        k.emoji,
                        k.name,
                        r.id,
                        r.markers.len(),
                        if r.broken_cites.is_empty() {
                            "-".into()
                        } else {
                            r.broken_cites.join(",")
                        }
                    );
                    for m in &r.markers {
                        println!("    {}:{} [{}] {}", m.file, m.line, m.kind, m.text);
                    }
                }
            }

            if failed.is_empty() {
                println!(
                    "{} [{}] all {} done task(s) verified — no fake delivery",
                    k.emoji,
                    k.name,
                    reports.len()
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
                    "{} [{}] docs off ([docs] auto_generate=false) — skipped {id}",
                    k.emoji, k.name
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
            println!("{} [{}] wrote {path}", k.emoji, k.name);
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
        "{} [{}] {tag}{cfg} {} · {} membrane event(s) registered, {} already wired → {}",
        k.emoji,
        k.name,
        report.config_path.display(),
        report.registered.len(),
        report.already.len(),
        report.settings_path.display(),
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
