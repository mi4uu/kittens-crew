//! kittenscrew — Rust core for the kitten plugin.
//!
//! Wraps squeez (binary + hook scripts) w/ own hooks. Adds spec/plan management,
//! kitty:says() visual wrapper, per-project config. See SPEC.md.

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::process::{Command, ExitCode, Stdio};

mod check;
mod config;
mod drift;
mod plan;
mod score;
mod spec;
mod store;

/// Kittenscrew CLI — wraps squeez + manages spec/plan for the kitten plugin.
#[derive(Parser, Debug)]
#[command(name = "kittenscrew", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Kitty visual wrapper — prefix output w/ emoji + name.
    Kitty {
        #[command(subcommand)]
        action: KittyAction,
    },
    /// Spec management (T9-T11).
    Spec {
        #[command(subcommand)]
        action: SpecAction,
    },
    /// Plan management (T12-T14).
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },
    /// Cyclic done-eval (T30): fake-delivery scan + cited-§V integrity, demote on fail.
    Check {
        #[command(subcommand)]
        action: CheckAction,
    },
    /// Graded conformance score (T48, V31): how close to ideal, 0-100% per dim.
    Score,
    /// Hook orchestration (T5-T8). Reads JSON from stdin (Claude Code hook contract).
    Hook {
        /// Hook event: session-start | pre-tool | post-tool | pre-compact.
        event: String,
    },
    /// Per-project config (T15): `kittenscrew.toml` parse + defaults.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Init: write kittenscrew.toml + register hooks (T16).
    Init,
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Resolve `kittenscrew.toml` (defaults if absent) → JSON.
    Show,
}

#[derive(Subcommand, Debug)]
enum KittyAction {
    /// Speak as a kitty — prefixes output w/ `😽📐 [Planning Kitty] msg`.
    Says {
        /// Kitty id (planning|builder|entropy|memory|scribe|orchestrating).
        kitty: String,
        /// Message to prefix.
        message: String,
    },
    /// List all kitties w/ emoji + role.
    List,
}

#[derive(Subcommand, Debug)]
enum SpecAction {
    /// Read a section (§<S> letter, e.g. T) or whole spec, from the store.
    Read {
        /// Section letter (G|C|I|V|T|B). Optional → whole spec.
        section: Option<String>,
        /// Expand caveman symbols to English (legend baked in, no FORMAT.md needed).
        #[arg(long)]
        plain: bool,
    },
    /// Apply structured JSON diff(s) from stdin (validates vs §V; exit 2 + unchanged on violation).
    Apply,
    /// Structural validation: deps/cites resolve, ids unique, no cycle.
    Check,
    /// Bootstrap: parse SPEC.md → `.kittenscrew/spec.toml` (one-time / drift).
    Import,
    /// Regenerate SPEC.md from the store (projection).
    Render,
    /// Drift reconcile (T29): diff edited SPEC.md vs store; `--apply` reconciles structural + re-renders.
    Drift {
        /// Reconcile structural task changes into the store + re-render (else dry-run report).
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand, Debug)]
enum CheckAction {
    /// Re-verify every `x` task; demote `x`→`~` on fake-delivery or broken cites.
    Done,
    /// value-variance (T42): delivered (eval) vs expected (value) per done task.
    Variance,
}

#[derive(Subcommand, Debug)]
enum PlanAction {
    /// Topo-sort tasks by dependencies (JSON order).
    Resolve,
    /// READY frontier: all unblocked tasks (the parallelizable batch).
    Ready,
    /// Single next task (ready, lowest priority then id).
    Next,
    /// Tasks directly blocked by <id>.
    Blocking {
        /// Task id (e.g. T5).
        id: String,
    },
    /// Impact of doing <id>: scope delivered, tasks unblocked + blocked.
    Impact {
        /// Task id (e.g. T5).
        id: String,
    },
    /// Critical path (longest prereq chain), optionally ending at <goal>.
    Path {
        /// Goal task id. Optional → longest chain in the DAG.
        goal: Option<String>,
    },
    /// Frontier choices, each with {scope, unblocks, blocks, worth, rank}, ranked by worth.
    Alternatives,
    /// All tasks scored by worth/rank (value-weighted, V22/V24), highest first.
    Worth,
    /// Mark task done (store → re-render SPEC.md projection).
    Done {
        /// Task id (e.g. T5).
        id: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("kittenscrew: error: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

fn run(cli: Cli) -> Result<(), KittenError> {
    match cli.cmd {
        Cmd::Kitty { action } => match action {
            KittyAction::Says { kitty, message } => kitty_says(&kitty, &message),
            KittyAction::List => kitty_list(),
        },
        Cmd::Spec { action } => spec_cmd(action),
        Cmd::Plan { action } => plan_cmd(action),
        Cmd::Check { action } => check_cmd(action),
        Cmd::Score => score_cmd(),
        Cmd::Config { action } => config_cmd(action),
        Cmd::Hook { event } => hook::dispatch(&event),
        Cmd::Init => init_stub(),
    }
}

fn kitty_says(kitty: &str, message: &str) -> Result<(), KittenError> {
    let k = kitty::lookup(kitty)
        .ok_or_else(|| KittenError::Validation(format!("unknown kitty: {kitty}")))?;
    // V5: ∀ output → emoji + [Name] + raw message. No mutation.
    println!("{} [{}] {}", k.emoji, k.name, message);
    Ok(())
}

fn kitty_list() -> Result<(), KittenError> {
    println!("id|emoji|name|role");
    for k in kitty::all() {
        println!("{}|{}|{}|{}", k.id, k.emoji, k.name, k.role);
    }
    Ok(())
}

const SPEC_PATH: &str = "SPEC.md";

/// T47/V30: a render-triggering cmd must not clobber a pending manual SPEC.md
/// edit. If the on-disk file diverges from the store projection, abort and tell
/// the caller to reconcile via `spec drift --apply` first.
fn ensure_synced(store: &store::Store) -> Result<(), KittenError> {
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
fn worth_params() -> plan::WorthParams {
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

fn config_cmd(action: ConfigAction) -> Result<(), KittenError> {
    match action {
        ConfigAction::Show => {
            let c = config::load().map_err(KittenError::Validation)?;
            println!("{}", serde_json::to_string_pretty(&c).unwrap());
            Ok(())
        }
    }
}

fn init_stub() -> Result<(), KittenError> {
    eprintln!("init: not implemented yet (T16 pending)");
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum KittenError {
    #[error("{0}")]
    Validation(String),
    // kitten: constructed by T16 `init` (exit 3 when squeez unreachable, V6).
    #[allow(dead_code)]
    #[error("squeez binary not found in PATH or ~/.claude/squeez/bin/")]
    SqueezMissing,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("store: {0}")]
    Store(#[from] store::StoreError),
    #[error("spec: {0}")]
    Spec(#[from] spec::SpecError),
}

impl KittenError {
    /// V1: exit 0 ok, 2 validation, 3 squeez-missing, 1 other.
    fn exit_code(&self) -> u8 {
        match self {
            KittenError::Validation(_) => 2,
            KittenError::SqueezMissing => 3,
            _ => 1,
        }
    }
}

mod kitty {
    pub struct Kitty {
        pub id: &'static str,
        pub emoji: &'static str,
        pub name: &'static str,
        pub role: &'static str,
    }

    pub const ALL: &[Kitty] = &[
        Kitty {
            id: "orchestrating",
            emoji: "🎩",
            name: "Orchestrating Kitty",
            role: "routing + final summary",
        },
        Kitty {
            id: "planning",
            emoji: "📐",
            name: "Planning Kitty",
            role: "spec / SPEC.md",
        },
        Kitty {
            id: "builder",
            emoji: "🔨",
            name: "Builder Kitty",
            role: "build + ladder",
        },
        Kitty {
            id: "entropy",
            emoji: "😼",
            name: "Entropy Kitty",
            role: "check, drift & bloat hunt",
        },
        Kitty {
            id: "memory",
            emoji: "🧠",
            name: "Memory Kitty",
            role: "backprop, bug → §B+§V",
        },
        Kitty {
            id: "scribe",
            emoji: "🖋️",
            name: "Scribe Kitty",
            role: "README, docs, comments",
        },
    ];

    pub fn all() -> &'static [Kitty] {
        ALL
    }

    pub fn lookup(id: &str) -> Option<&'static Kitty> {
        ALL.iter().find(|k| k.id == id)
    }
}

/// squeez binary + hook scripts detection + invocation. V2: graceful degrade.
mod squeez {
    use super::*;

    /// Locate squeez binary. Order: $SQUEEZ_BIN, $PATH, ~/.claude/squeez/bin/squeez.
    pub fn bin() -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("SQUEEZ_BIN") {
            let pb = std::path::PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
        if let Some(p) = which("squeez") {
            return Some(p);
        }
        let home = std::env::var("HOME").ok()?;
        let p = std::path::PathBuf::from(home).join(".claude/squeez/bin/squeez");
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    }

    /// Locate squeez hook scripts dir. Returns path to `.../squeez/hooks/`.
    pub fn hooks_dir() -> Option<std::path::PathBuf> {
        let bin = bin()?;
        // bin is at <prefix>/squeez/bin/squeez → hooks at <prefix>/squeez/hooks
        bin.parent()?.parent()?.parent().map(|p| p.join("hooks"))
    }

    fn which(name: &str) -> Option<std::path::PathBuf> {
        let path = std::env::var("PATH").ok()?;
        for dir in path.split(':') {
            let candidate = std::path::PathBuf::from(dir).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Run `squeez <args...>` w/ stdin piped. Returns stdout.
    /// Returns None if squeez missing (graceful degrade per V2).
    pub fn run(args: &[&str], stdin: &str) -> Result<Option<String>, KittenError> {
        let bin = match bin() {
            Some(b) => b,
            None => return Ok(None),
        };
        let mut child = Command::new(&bin)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut sin) = child.stdin.take() {
            sin.write_all(stdin.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
    }

    /// Run squeez hook script (e.g. pretooluse.sh) w/ stdin piped.
    /// Returns stdout. Returns None if script missing.
    pub fn run_hook(event: &str, stdin: &str) -> Result<Option<String>, KittenError> {
        let hooks = match hooks_dir() {
            Some(d) => d,
            None => return Ok(None),
        };
        // Map event name to squeez hook script.
        let script = match event {
            "session-start" => hooks.join("session-start.sh"),
            "pre-tool" => hooks.join("pretooluse.sh"),
            "post-tool" => hooks.join("posttooluse.sh"),
            "pre-compact" => hooks.join("precompact.sh"),
            _ => return Ok(None),
        };
        if !script.is_file() {
            return Ok(None);
        }
        let mut child = Command::new(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut sin) = child.stdin.take() {
            sin.write_all(stdin.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
    }
}

/// Hook orchestration: dispatch by event. V7: single entry, delegates to squeez.
mod hook {
    use super::*;

    pub fn dispatch(event: &str) -> Result<(), KittenError> {
        let mut stdin = String::new();
        std::io::stdin().read_to_string(&mut stdin)?;
        match event {
            "session-start" => session_start(&stdin),
            "pre-tool" => pre_tool(&stdin),
            "post-tool" => post_tool(&stdin),
            "pre-compact" => pre_compact(&stdin),
            other => Err(KittenError::Validation(format!(
                "unknown hook event: {other}"
            ))),
        }
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
        Ok(())
    }

    /// T6: PreToolUse — run kittenscrew checks first (blocked commands etc.),
    /// then delegate compression to squeez pretooluse.sh.
    fn pre_tool(stdin: &str) -> Result<(), KittenError> {
        // 1. Kittenscrew-specific: validate against blocked commands (T15 will load config).
        if let Some(block_reason) = check_blocked(stdin) {
            // Emit block decision JSON for Claude Code.
            println!(
                r#"{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"kittenscrew blocked: {block_reason}"}}}}"#
            );
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

    /// T7: PostToolUse — delegate to squeez, then optional kittenscrew post-processing.
    fn post_tool(stdin: &str) -> Result<(), KittenError> {
        if let Some(out) = squeez::run_hook("post-tool", stdin)? {
            if !out.trim().is_empty() {
                print!("{out}");
            }
        }
        // TODO T9-T11: if SPEC.md modified → run spec check
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn kitty_all_has_six_kitties() {
        assert_eq!(kitty::all().len(), 6);
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
