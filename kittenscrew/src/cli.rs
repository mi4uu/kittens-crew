//! clap CLI surface — the command tree (`Cli`, `Cmd`, and the `*Action` enums).

use clap::{Parser, Subcommand};

/// Kittenscrew CLI — wraps squeez + manages spec/plan for the kitten plugin.
#[derive(Parser, Debug)]
#[command(name = "kittenscrew", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
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
    /// Drive the DAG autonomously (T62/T65): the harness takes each ready code
    /// leaf, dispatches a scoped prompt to a Driver, verifies it compiles (T63),
    /// and advances. The harness drives; the model only fills leaves.
    Run {
        /// Spec store to drive (default: the project's .kittenscrew/spec.toml). Point at
        /// a toy spec to experiment on the side without touching the real store.
        #[arg(long)]
        store: Option<std::path::PathBuf>,
        /// Backend: `api` (rig/HTTP, default) | `claude-code` (tmux, T71 — not built yet).
        #[arg(long, default_value = "api")]
        driver: String,
        /// Model id for the api driver (routes through RigDriver). Omit → codestral default.
        #[arg(long)]
        model: Option<String>,
        /// Drive scope-disjoint ready batches concurrently (T77) instead of one node at a time.
        #[arg(long)]
        parallel: bool,
        /// YOLO mode (T64): no per-tool dialogs — only the tripwire negative filter gates.
        #[arg(long)]
        yolo: bool,
        /// Rough token budget cap for the run (T70). Surfaced now; in-loop enforcement is pending.
        #[arg(long)]
        budget: Option<u64>,
        /// Safety net (P1.5): snapshot the working tree before driving and roll back
        /// (restore tracked files + remove the run's new files) if the run halts.
        #[arg(long)]
        rollback_on_fail: bool,
        /// Max nodes to drive before yielding (V34 hard bound).
        #[arg(long, default_value_t = 20)]
        max_iters: u32,
        /// Bounded replan (T74): retries per node, feeding the rustc error back.
        #[arg(long, default_value_t = 2)]
        max_retries: u32,
    },
    /// A/B benchmark (T75): bare-baseline vs kittenscrew on the SAME model/store,
    /// k trials each — reports the delta that is the harness's actual weight.
    Bench {
        /// Spec store to benchmark against (a small toy spec, NOT the live repo store).
        #[arg(long)]
        store: std::path::PathBuf,
        /// Trials per arm (pass^k consistency). Each trial is a full model run.
        #[arg(long, default_value_t = 3)]
        k: u32,
        #[arg(long, default_value_t = 20)]
        max_iters: u32,
        #[arg(long, default_value_t = 2)]
        max_retries: u32,
    },
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
    /// Compression policy (T49, V32): per content-class squeez level.
    Compression {
        #[command(subcommand)]
        action: CompressionAction,
    },
    /// Per-task docs (T23): `docs task <id>` → `docs/<id>-<slug>.md` (V12, opt-in).
    Docs {
        #[command(subcommand)]
        action: DocsAction,
    },
    /// Init: write kittenscrew.toml + register the hook membrane (T16).
    Init {
        /// Dir holding `settings.json` (default: `$HOME/.claude`). Isolates the
        /// write — pass a scratch dir for tests / Docker arms.
        #[arg(long)]
        target: Option<std::path::PathBuf>,
        /// Report the plan without touching disk.
        #[arg(long)]
        dry_run: bool,
        /// Overwrite an existing `kittenscrew.toml` (default: keep it).
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Resolve `kittenscrew.toml` (defaults if absent) → JSON.
    Show,
}

#[derive(Subcommand, Debug)]
pub enum CompressionAction {
    /// Print the full class→level policy as JSON.
    Policy,
    /// Print the squeez level for one content-class (exit 2 if unknown).
    Level {
        /// Content-class: prose | dump | structured | diff.
        class: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum DocsAction {
    /// Write `docs/<id>-<slug>.md` for a task (only if `[docs] auto_generate`).
    Task {
        /// Task id (e.g. T9).
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum KittyAction {
    /// Speak as a kitty — prefixes output w/ `😽📐 [Planning Kitty] msg`.
    Says {
        /// Kitty id (planning|builder|entropy|memory|scribe|orchestrating|helper|explorer|style|grill).
        kitty: String,
        /// Message to prefix.
        message: String,
        /// Wrap in a comic speech-box: rounded (default) | heavy | double | classic.
        #[arg(long = "box", num_args = 0..=1, default_missing_value = "rounded")]
        frame: Option<String>,
    },
    /// List all kitties w/ emoji + role.
    List,
}

#[derive(Subcommand, Debug)]
pub enum SpecAction {
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
    /// Spec-from-prose (T17): a model turns a plain-language goal into a validated
    /// DAG of build tasks, written to the store. Iterates with the model on a §V
    /// violation. The planner step of "describe it → working program".
    Gen {
        /// What you want built, in plain language (e.g. "a CLI that reverses a string").
        goal: String,
        /// Store to write the plan into (default: the project's .kittenscrew/spec.toml).
        #[arg(long)]
        store: Option<std::path::PathBuf>,
        /// Model id (else KITTENSCREW_MODEL). Endpoint via KITTENSCREW_BASE_URL.
        #[arg(long)]
        model: Option<String>,
        /// Retries feeding validation errors back to the model before giving up.
        #[arg(long, default_value_t = 3)]
        max_retries: u32,
    },
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
pub enum CheckAction {
    /// Re-verify every `x` task; demote `x`→`~` on fake-delivery or broken cites.
    Done,
    /// value-variance (T42): delivered (eval) vs expected (value) per done task.
    Variance,
}

#[derive(Subcommand, Debug)]
pub enum PlanAction {
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
    /// ASCII DAG render of tasks + deps (presentation-only, T32).
    Graph,
    /// Mark task done (store → re-render SPEC.md projection).
    Done {
        /// Task id (e.g. T5).
        id: String,
    },
}
