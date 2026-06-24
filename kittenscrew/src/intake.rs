//! T51 — UserPromptSubmit intake (V35, V33). Deterministic classification of the
//! user's prompt + TARGETED context injection. The drive-wheel's front door: when
//! a prompt arrives, the agent gets exactly what it needs to act — `plan next`,
//! and (when the prompt names a task) that task's record — NEVER the whole spec
//! front-loaded. Semantic ambiguity is flagged for the LLM to resolve (judgement
//! stays with the model, §G); everything mechanical is decided here.

use crate::store::Task;

/// How the prompt reads to the deterministic classifier.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Intent {
    /// Names a `Tn` task → inject that task's record.
    MapsTask,
    /// Actionable, concrete → inject `plan next` only.
    Clear,
    /// Vague reference, no concrete anchor → steer the agent to clarify (V35).
    Ambiguous,
}

impl Intent {
    pub fn as_str(self) -> &'static str {
        match self {
            Intent::MapsTask => "maps-§T",
            Intent::Clear => "clear",
            Intent::Ambiguous => "ambiguous",
        }
    }
}

/// Vague references that, with no concrete anchor, mark a prompt ambiguous.
const VAGUE: &[&str] = &[
    "it",
    "this",
    "that",
    "those",
    "these",
    "them",
    "everything",
    "stuff",
    "thing",
];

/// First `Tn` task id mentioned (boundary-anchored, case-insensitive), normalized
/// to upper-case `T<digits>`. Existence is the caller's check — this is lexical.
pub fn task_ref(prompt: &str) -> Option<String> {
    let b = prompt.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let is_t = b[i] == b'T' || b[i] == b't';
        let boundary = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        if is_t && boundary && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            return Some(format!("T{}", &prompt[i + 1..j]));
        }
        i += 1;
    }
    None
}

/// Classify the prompt. Task ref wins; else a short vague-referential prompt is
/// ambiguous; else clear. Deliberately conservative — over-flagging ambiguity is
/// noise, and the LLM can always override the steer.
pub fn classify(prompt: &str) -> Intent {
    if task_ref(prompt).is_some() {
        return Intent::MapsTask;
    }
    if is_ambiguous(prompt) {
        return Intent::Ambiguous;
    }
    Intent::Clear
}

fn is_ambiguous(prompt: &str) -> bool {
    let words: Vec<&str> = prompt.split_whitespace().collect();
    if words.is_empty() {
        return true;
    }
    let has_vague = words.iter().any(|w| {
        let norm = w
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_ascii_lowercase();
        VAGUE.contains(&norm.as_str())
    });
    // Short + vague-referential, no concrete anchor: "fix it", "redo that".
    has_vague && words.len() < 6
}

/// A compact one-block record of a referenced task (V33: small, not the spec).
fn task_block(t: &Task) -> String {
    let deps = if t.deps.is_empty() {
        "-".to_string()
    } else {
        t.deps.join(",")
    };
    let cites = if t.cites.is_empty() {
        "-".to_string()
    } else {
        t.cites.join(",")
    };
    format!(
        "{} [{}] {}\n  deps:{deps} cites:{cites} value:{} risk:{}",
        t.id,
        t.status.symbol(),
        t.task,
        t.value,
        t.risk
    )
}

/// The `additionalContext` payload Claude Code injects before the turn. Targeted
/// by construction: a header, the next action, the referenced task (if any), and
/// — only when ambiguous — a clarify steer.
pub fn render(intent: Intent, next: Option<(&str, &str)>, task: Option<&Task>) -> String {
    let mut out = format!("[kittenscrew intake] intent={}\n", intent.as_str());
    match next {
        Some((id, task)) => out.push_str(&format!("do next: {id} — {task}\n")),
        None => out.push_str("do next: frontier empty (all ready tasks done or blocked)\n"),
    }
    if let Some(t) = task {
        out.push_str("referenced task:\n");
        out.push_str(&task_block(t));
        out.push('\n');
    }
    if intent == Intent::Ambiguous {
        out.push_str("scope unclear — confirm the target with the user before acting (V35)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str) -> Task {
        Task {
            id: id.into(),
            status: crate::store::Status::Todo,
            task: "build the thing".into(),
            deps: vec!["T16".into()],
            cites: vec!["V35".into()],
            value: 5,
            risk: 3,
            ..Default::default()
        }
    }

    #[test]
    fn task_ref_anchors_on_boundary_and_normalizes_case() {
        assert_eq!(task_ref("work on T51 now"), Some("T51".into()));
        assert_eq!(task_ref("do t7 please"), Some("T7".into()));
        assert_eq!(task_ref("the first task"), None);
        // Not a boundary: NOT a task ref (e.g. an identifier like NT5).
        assert_eq!(task_ref("var NT5 = 1"), None);
        // First wins.
        assert_eq!(task_ref("T9 then T12"), Some("T9".into()));
    }

    #[test]
    fn classify_routes_three_ways() {
        assert_eq!(classify("implement T51 intake hook"), Intent::MapsTask);
        assert_eq!(classify("add a compression policy command"), Intent::Clear);
        assert_eq!(classify("fix it"), Intent::Ambiguous);
        assert_eq!(classify("make that better"), Intent::Ambiguous);
        assert_eq!(classify(""), Intent::Ambiguous);
        // Vague word but long/concrete enough → not flagged.
        assert_eq!(
            classify("refactor the store module that holds tasks into smaller files"),
            Intent::Clear
        );
    }

    #[test]
    fn render_is_targeted_and_flags_ambiguity() {
        let t = task("T51");
        let r = render(Intent::MapsTask, Some(("T51", "build intake")), Some(&t));
        assert!(r.contains("intent=maps-§T"));
        assert!(r.contains("do next: T51"));
        assert!(r.contains("T51 [.]"));
        assert!(r.contains("value:5 risk:3"));
        assert!(!r.contains("scope unclear")); // not ambiguous

        let amb = render(Intent::Ambiguous, None, None);
        assert!(amb.contains("scope unclear"));
        assert!(amb.contains("frontier empty"));
    }
}
