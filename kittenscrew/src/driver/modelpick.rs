//! T73 — model right-sizing via WEIGHTED SCORING (soft preferences, never bans).
//!
//! Picks the best AVAILABLE model for a node by combining per-role × per-model
//! trait weights with the node's difficulty/risk/value scores. A weight degrades
//! gracefully when a preferred model is rate-limited or absent — a ban would be
//! brittle (V-principle: anti-stall guarantee — NEVER returns `None` when at
//! least one model is available).
//!
//! ## Design
//! - [`Role`] classifies the node's work (Coding / Research / Conversational / Review).
//! - [`ModelTraits`] describes a model along four axes: `coding_strength`,
//!   `ctx_window`, `speed`, `free_reliability`.
//! - [`score`] multiplies role-specific per-trait weights by the trait values,
//!   then adds a difficulty/risk/value bonus that steers harder nodes to stronger
//!   (larger) models and cheap leaf nodes to small/fast ones.
//! - [`pick`] returns the highest-scoring model from the *available* slice, never
//!   `None` if the slice is non-empty (anti-stall guarantee).
//! - [`role_of`] infers role from a [`crate::store::Task`] heuristically (scope or
//!   task text contains code-ish signals → Coding; else Conversational).

use crate::store::Task;

// ── Role ─────────────────────────────────────────────────────────────────────

/// Semantic role of a DAG node, used to look up the right trait-weight vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Produce or review source code. Gemma-family DOWN-weighted here (empirically
    /// weak at code); qwen3-coder / deepseek tier preferred.
    Coding,
    /// Literature search, synthesis, summarisation. Cheap/fast models are fine.
    Research,
    /// Free-form chat, planning prose, user-facing explanations.
    Conversational,
    /// Audit code or spec for correctness. Mid-weight: needs reasoning, not speed.
    Review,
}

// ── ModelTraits ───────────────────────────────────────────────────────────────

/// Numeric capability axes for a single model. All values are normalised to
/// `[0.0, 1.0]` so the scorer can combine them with plain multiplication.
///
/// - `coding_strength` — quality on code-gen benchmarks (HumanEval / SWE).
/// - `ctx_window`      — relative context size (1.0 = largest in the set).
/// - `speed`           — tokens/s proxy (1.0 = fastest in the set).
/// - `free_reliability`— up-time / rate-limit headroom on free/public endpoints.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelTraits {
    pub name: String,
    pub coding_strength: f64,
    pub ctx_window: f64,
    pub speed: f64,
    pub free_reliability: f64,
}

// ── Default model catalogue ───────────────────────────────────────────────────

/// Seed catalogue: a representative small/cheap model (gemma), a coding-specialist
/// (qwen3-coder), and a large/capable model (deepseek-r1). Callers may extend or
/// replace this list — the scorer is fully data-driven.
pub fn default_models() -> Vec<ModelTraits> {
    vec![
        // Small, cheap, fast — fine for Research/Conversational; weak at code.
        ModelTraits {
            name: "google/gemma-3-27b-it:free".into(),
            coding_strength: 0.35,
            ctx_window: 0.50,
            speed: 0.90,
            free_reliability: 0.85,
        },
        // Coding specialist — strong at code, moderate speed, decent reliability.
        ModelTraits {
            name: "qwen/qwen3-coder".into(),
            coding_strength: 0.90,
            ctx_window: 0.75,
            speed: 0.70,
            free_reliability: 0.75,
        },
        // Large reasoning model — strongest overall, slower, lower free headroom.
        ModelTraits {
            name: "deepseek/deepseek-r1".into(),
            coding_strength: 0.85,
            ctx_window: 1.00,
            speed: 0.40,
            free_reliability: 0.60,
        },
        // Mid-tier fast model — balanced Research/Conversational/Review.
        ModelTraits {
            name: "meta-llama/llama-3.3-70b-instruct:free".into(),
            coding_strength: 0.65,
            ctx_window: 0.65,
            speed: 0.80,
            free_reliability: 0.80,
        },
    ]
}

// ── Role inference ────────────────────────────────────────────────────────────

/// Heuristically infer a [`Role`] from a task's scope globs and text.
///
/// Rules (first match wins):
/// 1. Any scope entry ends in `.rs`, `.ts`, `.py`, `.go`, `.c`, `.cpp`, `.js`
///    or `.toml` → [`Role::Coding`].
/// 2. Task text contains "review" or "audit" → [`Role::Review`].
/// 3. Task text contains "research" or "search" or "summar" → [`Role::Research`].
/// 4. Otherwise → [`Role::Conversational`].
pub fn role_of(task: &Task) -> Role {
    let code_exts = [".rs", ".ts", ".py", ".go", ".c", ".cpp", ".js", ".toml"];
    if task
        .scope
        .iter()
        .any(|s| code_exts.iter().any(|ext| s.ends_with(ext)))
    {
        return Role::Coding;
    }
    let lower = task.task.to_lowercase();
    if lower.contains("review") || lower.contains("audit") {
        return Role::Review;
    }
    if lower.contains("research") || lower.contains("search") || lower.contains("summar") {
        return Role::Research;
    }
    Role::Conversational
}

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Per-role weight vectors over the four trait axes.
/// Weights are additive scalars; higher = "care more about this trait".
struct RoleWeights {
    coding_strength: f64,
    ctx_window: f64,
    speed: f64,
    free_reliability: f64,
}

fn weights_for(role: Role) -> RoleWeights {
    match role {
        Role::Coding => RoleWeights {
            coding_strength: 2.5, // dominant signal — code quality matters most
            ctx_window: 0.8,
            speed: 0.3,
            free_reliability: 0.4,
        },
        Role::Research => RoleWeights {
            coding_strength: 0.2,
            ctx_window: 1.5, // long context helps synthesis
            speed: 0.6,
            free_reliability: 1.0,
        },
        Role::Conversational => RoleWeights {
            coding_strength: 0.1,
            ctx_window: 0.5,
            speed: 1.2, // responsiveness matters
            free_reliability: 1.2,
        },
        Role::Review => RoleWeights {
            coding_strength: 1.5,
            ctx_window: 1.0,
            speed: 0.4,
            free_reliability: 0.6,
        },
    }
}

/// Compute the preference score for `model` on a node with the given role and
/// difficulty/risk/value metadata.
///
/// Formula:
/// ```text
/// base  = Σ (weight_i × trait_i)          // role-weighted capability
/// boost = (difficulty + risk) / 10.0 × coding_strength
///         + value / 5.0 × ctx_window       // harder/riskier/valuable → bigger model
/// score = base + boost
/// ```
///
/// The `difficulty + risk` term means high-stakes nodes prefer stronger models;
/// low-scope leaf nodes stay cheap by default. `value` adds a small ctx bonus so
/// high-value tasks land in a model that can hold more context.
pub fn score(model: &ModelTraits, role: Role, difficulty: i64, risk: i64, value: i64) -> f64 {
    let w = weights_for(role);
    let base = w.coding_strength * model.coding_strength
        + w.ctx_window * model.ctx_window
        + w.speed * model.speed
        + w.free_reliability * model.free_reliability;

    // Clamp inputs to [0,5] to guard against out-of-range store values.
    let d = (difficulty.max(0).min(5)) as f64;
    let r = (risk.max(0).min(5)) as f64;
    let v = (value.max(0).min(5)) as f64;

    let boost = (d + r) / 10.0 * model.coding_strength + v / 5.0 * model.ctx_window;

    base + boost
}

// ── Picker ────────────────────────────────────────────────────────────────────

/// Return the highest-scoring model from `available` for the given role and node
/// metadata.
///
/// **Anti-stall guarantee**: returns `Some` whenever `available` is non-empty —
/// even if every model scores poorly for the role. This is a SOFT preference
/// system: no model is ever banned. If the preferred model is unavailable the
/// next-best is used; if only a weak model is available it is still returned.
///
/// Returns `None` only when `available` is empty (truly no model to call).
pub fn pick<'a>(
    available: &'a [ModelTraits],
    role: Role,
    difficulty: i64,
    risk: i64,
    value: i64,
) -> Option<&'a ModelTraits> {
    available
        .iter()
        .max_by(|a, b| {
            score(a, role, difficulty, risk, value)
                .partial_cmp(&score(b, role, difficulty, risk, value))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Status, Task};

    fn make_task(id: &str, task: &str, scope: Vec<&str>, difficulty: i64, risk: i64, value: i64) -> Task {
        Task {
            id: id.into(),
            status: Status::Todo,
            task: task.into(),
            deps: vec![],
            priority: 0,
            cites: vec![],
            scope: scope.into_iter().map(|s| s.to_string()).collect(),
            note: String::new(),
            value,
            difficulty,
            risk,
            eval: None,
        }
    }

    /// Coding node must NOT pick gemma when a coding-specialist is also available.
    /// (Soft preference working as intended.)
    #[test]
    fn coding_node_prefers_specialist_over_gemma() {
        let gemma = ModelTraits {
            name: "google/gemma-3-27b-it:free".into(),
            coding_strength: 0.35,
            ctx_window: 0.50,
            speed: 0.90,
            free_reliability: 0.85,
        };
        let coder = ModelTraits {
            name: "qwen/qwen3-coder".into(),
            coding_strength: 0.90,
            ctx_window: 0.75,
            speed: 0.70,
            free_reliability: 0.75,
        };
        let available = vec![gemma.clone(), coder.clone()];
        let picked = pick(&available, Role::Coding, 3, 2, 3).unwrap();
        assert_eq!(
            picked.name, coder.name,
            "Coding node should prefer the coding specialist, got {}",
            picked.name
        );
    }

    /// Anti-stall: when ONLY gemma is available, a Coding node MUST still pick it.
    /// This proves no model is ever banned — weight degrades, never blocks.
    #[test]
    fn coding_node_falls_back_to_gemma_when_only_option() {
        let gemma = ModelTraits {
            name: "google/gemma-3-27b-it:free".into(),
            coding_strength: 0.35,
            ctx_window: 0.50,
            speed: 0.90,
            free_reliability: 0.85,
        };
        let available = vec![gemma.clone()];
        let picked = pick(&available, Role::Coding, 3, 2, 3).unwrap();
        assert_eq!(
            picked.name, gemma.name,
            "must return gemma even for Coding when it is the only available model"
        );
    }

    /// Empty available list → None (no stall, just honest signal to caller).
    #[test]
    fn empty_available_returns_none() {
        let result = pick(&[], Role::Coding, 3, 3, 3);
        assert!(result.is_none());
    }

    /// High-difficulty + high-risk node should favor the stronger (larger) model.
    #[test]
    fn high_difficulty_favors_strong_model() {
        let weak = ModelTraits {
            name: "weak-small".into(),
            coding_strength: 0.30,
            ctx_window: 0.30,
            speed: 0.95,
            free_reliability: 0.95,
        };
        let strong = ModelTraits {
            name: "strong-large".into(),
            coding_strength: 0.95,
            ctx_window: 1.00,
            speed: 0.35,
            free_reliability: 0.55,
        };
        let available = vec![weak.clone(), strong.clone()];
        // High difficulty (5) + high risk (5): the boost term heavily rewards coding_strength.
        let picked = pick(&available, Role::Coding, 5, 5, 4).unwrap();
        assert_eq!(
            picked.name, strong.name,
            "high-difficulty Coding node must pick the strong model, got {}",
            picked.name
        );
    }

    /// Research node is fine picking the cheap/fast model over a coding specialist.
    #[test]
    fn research_node_happy_with_cheap_model() {
        let cheap = ModelTraits {
            name: "cheap-fast".into(),
            coding_strength: 0.30,
            ctx_window: 0.80,
            speed: 0.95,
            free_reliability: 0.95,
        };
        let expensive = ModelTraits {
            name: "expensive-coder".into(),
            coding_strength: 0.95,
            ctx_window: 0.60,
            speed: 0.35,
            free_reliability: 0.50,
        };
        let available = vec![cheap.clone(), expensive.clone()];
        // Research weights: ctx_window and free_reliability dominate; coding is near-zero.
        let picked = pick(&available, Role::Research, 1, 1, 2).unwrap();
        assert_eq!(
            picked.name, cheap.name,
            "Research node should pick cheap/reliable model, got {}",
            picked.name
        );
    }

    /// role_of: .rs scope → Coding.
    #[test]
    fn role_infer_rs_scope_is_coding() {
        let t = make_task("T1", "implement X", vec!["src/foo.rs"], 2, 1, 3);
        assert_eq!(role_of(&t), Role::Coding);
    }

    /// role_of: no code scope + "research" in text → Research.
    #[test]
    fn role_infer_research_text() {
        let t = make_task("T2", "research existing approaches", vec![], 1, 1, 2);
        assert_eq!(role_of(&t), Role::Research);
    }

    /// role_of: "review" in text → Review.
    #[test]
    fn role_infer_review_text() {
        let t = make_task("T3", "review the PR diff for bugs", vec![], 2, 2, 3);
        assert_eq!(role_of(&t), Role::Review);
    }

    /// role_of: no signals → Conversational fallback.
    #[test]
    fn role_infer_fallback_conversational() {
        let t = make_task("T4", "discuss approach with user", vec![], 1, 0, 1);
        assert_eq!(role_of(&t), Role::Conversational);
    }

    /// Full default catalogue: Coding + high difficulty → NOT gemma.
    #[test]
    fn default_models_coding_high_difficulty_not_gemma() {
        let models = default_models();
        let picked = pick(&models, Role::Coding, 5, 4, 4).unwrap();
        assert_ne!(
            picked.name, "google/gemma-3-27b-it:free",
            "default catalogue + high-difficulty Coding must not pick gemma; got {}",
            picked.name
        );
    }
}
