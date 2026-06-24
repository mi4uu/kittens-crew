//! T23 — `docs task <id>`: per-task what+why doc, generated from the store.
//! OFF by default (V12, `[docs] auto_generate`); detail level from `[docs] detail`.

use crate::store::{Store, Task};

/// kebab slug from a task's first words (alnum only), capped for filenames.
pub fn slug(task: &str) -> String {
    let s: String = task
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    s.split_whitespace().take(6).collect::<Vec<_>>().join("-")
}

/// `docs/<id>-<slug>.md` relative path for a task.
pub fn doc_path(t: &Task) -> String {
    format!("docs/{}-{}.md", t.id, slug(&t.task))
}

/// Render the task doc. `detail` ∈ {terse, normal, explain} controls depth.
pub fn render_task_doc(t: &Task, store: &Store, detail: &str) -> String {
    let mut out = format!("# {} — {}\n\n", t.id, t.task);
    out.push_str(&format!(
        "**Status:** {}  |  **value/difficulty/risk:** {}/{}/{}\n\n",
        t.status.symbol(),
        t.value,
        t.difficulty,
        t.risk
    ));

    if detail != "terse" {
        if !t.deps.is_empty() {
            out.push_str(&format!("**Depends on:** {}\n\n", t.deps.join(", ")));
        }
        // What+why: pull the cited §V/§I text so the doc explains the intent.
        if !t.cites.is_empty() {
            out.push_str("## Why (cited invariants)\n\n");
            for c in &t.cites {
                let text = store
                    .invariants
                    .iter()
                    .find(|i| &i.id == c)
                    .map(|i| i.text.as_str())
                    .unwrap_or("(see §I / §T)");
                out.push_str(&format!("- **{c}** — {text}\n"));
            }
            out.push('\n');
        }
    }

    if detail == "explain" {
        if let Some(ev) = &t.eval {
            out.push_str(&format!(
                "## Delivered\n\nself-eval: satisfaction {}/5, conformance {}/5, ~{} tokens. {}\n",
                ev.satisfaction, ev.conformance, ev.tokens, ev.note
            ));
        }
        if !t.scope.is_empty() {
            out.push_str(&format!("\n**Scope:** `{}`\n", t.scope.join("`, `")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Invariant, Status};

    fn task() -> Task {
        Task {
            id: "T9".into(),
            status: Status::Done,
            task: "impl spec read — render section".into(),
            cites: vec!["V8".into()],
            value: 3,
            ..Default::default()
        }
    }

    #[test]
    fn slug_is_kebab_capped() {
        assert_eq!(
            slug("impl `spec read` — render section now!"),
            "impl-spec-read-render-section-now"
        );
        assert_eq!(
            doc_path(&task()),
            "docs/T9-impl-spec-read-render-section.md"
        );
    }

    #[test]
    fn detail_levels_grow() {
        let mut store = Store::default();
        store.invariants.push(Invariant {
            id: "V8".into(),
            text: "caveman output".into(),
        });
        let t = task();
        let terse = render_task_doc(&t, &store, "terse");
        let normal = render_task_doc(&t, &store, "normal");
        assert!(!terse.contains("Why")); // terse omits cited invariants
        assert!(normal.contains("V8") && normal.contains("caveman output"));
        assert!(normal.len() > terse.len());
    }
}
