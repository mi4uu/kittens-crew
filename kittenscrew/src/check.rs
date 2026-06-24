//! T30/T31 — `check done`: cyclic re-evaluation of sealed `x` tasks (V18, V19).
//!
//! For every `Done` task: scan its `scope` globs for fake-delivery markers
//! (stubs / mocks / placeholders / TODO / `todo!()` / `unimplemented!()`) and
//! verify its cited §V invariants still exist. Either failing → the task is no
//! longer truly done → demote `x`→`~` and report. A green→red flip is the
//! regression alarm (V19): loud (report + nonzero exit), never silent.
//!
//! Scanner ported faithfully from the agency harness
//! (`benchmarks/agency/harness.py` `scan_shortcuts`).

use crate::store::{Status, Store, Task};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

/// (regex, kind) — ported from agency `SHORTCUT_PATTERNS`. Case-insensitive.
const PATTERNS: &[(&str, &str)] = &[
    (r"\btodo!\s*\(", "todo!()"),
    (r"\bunimplemented!\s*\(", "unimplemented!()"),
    (r"\bunreachable!\s*\(", "unreachable!()"),
    (
        r"//\s*(TODO|FIXME|HACK|XXX|STUB)\b",
        "TODO/FIXME/HACK comment",
    ),
    (
        r#"panic!\s*\(\s*"[^"]*(not implemented|todo|unimplemented|placeholder)"#,
        "panic: not-implemented",
    ),
    (
        r"\b(placeholder|dummy data|mock(ed)?|stubbed?|not[ _-]?implemented|hard[ -]?cod|for now|temporar|FIXME)\b",
        "placeholder/mock/hardcode wording",
    ),
];

fn compiled() -> &'static Vec<(Regex, &'static str)> {
    static RE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RE.get_or_init(|| {
        PATTERNS
            .iter()
            .map(|(p, kind)| {
                (
                    Regex::new(&format!("(?i){p}")).expect("PATTERNS are valid regex"),
                    *kind,
                )
            })
            .collect()
    })
}

/// One fake-delivery marker found in a scanned source line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub file: String,
    pub line: usize,
    pub text: String,
    pub kind: String,
}

/// Per-task verdict from `check done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskReport {
    pub id: String,
    pub ok: bool,
    pub markers: Vec<Hit>,
    pub broken_cites: Vec<String>,
}

/// Scan one source string. Returns `(line_no, kind, text)` — first matching
/// pattern per line wins (mirrors agency's `break`). Pure: no fs, testable.
pub fn scan_text(content: &str) -> Vec<(usize, &'static str, String)> {
    let mut hits = Vec::new();
    for (i, line) in content.lines().enumerate() {
        for (re, kind) in compiled() {
            if re.is_match(line) {
                let mut t = line.trim().to_string();
                t.truncate(160);
                hits.push((i + 1, *kind, t));
                break;
            }
        }
    }
    hits
}

/// True for source we never scan: build output and test code (agency parity).
fn is_excluded(path: &str) -> bool {
    path.contains("target/")
        || path.contains("/tests/")
        || path.ends_with("test.rs")
        || path.ends_with("_test.rs")
}

/// Expand a task's `scope` globs and scan every matched source file.
fn scan_scope(scope: &[String]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for pattern in scope {
        let paths = match glob::glob(pattern) {
            Ok(p) => p,
            Err(_) => continue, // bad glob → skip, not fatal
        };
        for entry in paths.flatten() {
            let path = entry.to_string_lossy().to_string();
            if is_excluded(&path) || !entry.is_file() {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&entry) else {
                continue;
            };
            for (line, kind, text) in scan_text(&content) {
                hits.push(Hit {
                    file: path.clone(),
                    line,
                    text,
                    kind: kind.to_string(),
                });
            }
        }
    }
    hits
}

/// §V citations of `task` that no longer resolve to a stored invariant.
/// Only `Vn` cites are checked (§I/§T refs aren't invariants). Pure, testable.
pub fn broken_cites(task: &Task, store: &Store) -> Vec<String> {
    task.cites
        .iter()
        .filter(|c| is_invariant_ref(c))
        .filter(|c| !store.invariants.iter().any(|inv| &inv.id == *c))
        .cloned()
        .collect()
}

fn is_invariant_ref(c: &str) -> bool {
    c.strip_prefix('V')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Evaluate every `Done` task. `ok` = no markers AND no broken cites.
/// Pure (no mutation): caller decides demotion. V18.
pub fn check_done(store: &Store) -> Vec<TaskReport> {
    store
        .tasks
        .iter()
        .filter(|t| t.status == Status::Done)
        .map(|t| {
            let markers = scan_scope(&t.scope);
            let broken = broken_cites(t, store);
            TaskReport {
                id: t.id.clone(),
                ok: markers.is_empty() && broken.is_empty(),
                markers,
                broken_cites: broken,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Status;

    fn task(id: &str, cites: &[&str]) -> Task {
        Task {
            id: id.into(),
            status: Status::Done,
            task: "t".into(),
            deps: vec![],
            priority: 100,
            cites: cites.iter().map(|s| s.to_string()).collect(),
            scope: vec![],
            note: String::new(),
            ..Default::default()
        }
    }

    #[test]
    fn scan_flags_todo_macro() {
        let hits = scan_text("fn f() { todo!() }");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, "todo!()");
    }

    #[test]
    fn scan_flags_unimplemented_and_unreachable() {
        assert_eq!(scan_text("    unimplemented!();")[0].1, "unimplemented!()");
        assert_eq!(scan_text("  unreachable!()")[0].1, "unreachable!()");
    }

    #[test]
    fn scan_flags_placeholder_wording_case_insensitive() {
        assert_eq!(scan_text("// FIXME later")[0].1, "TODO/FIXME/HACK comment");
        assert_eq!(
            scan_text("let x = mock(); // for now")[0].1,
            "placeholder/mock/hardcode wording"
        );
        assert_eq!(
            scan_text("return placeholder;")[0].1,
            "placeholder/mock/hardcode wording"
        );
    }

    #[test]
    fn scan_clean_code_is_empty() {
        assert!(scan_text("fn add(a: i32, b: i32) -> i32 { a + b }").is_empty());
    }

    #[test]
    fn scan_one_hit_per_line() {
        // line has both todo! and placeholder — only first pattern counted
        let hits = scan_text("todo!() // placeholder");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn broken_cite_when_invariant_missing() {
        let mut store = Store::default();
        store.invariants.push(crate::store::Invariant {
            id: "V1".into(),
            text: "x".into(),
        });
        let t = task("T1", &["V1", "V18", "§I"]);
        // V18 missing → broken; V1 present → fine; §I not an invariant ref → ignored
        assert_eq!(broken_cites(&t, &store), vec!["V18".to_string()]);
    }

    #[test]
    fn no_broken_cites_when_all_present() {
        let mut store = Store::default();
        for id in ["V1", "V2"] {
            store.invariants.push(crate::store::Invariant {
                id: id.into(),
                text: "x".into(),
            });
        }
        assert!(broken_cites(&task("T1", &["V1", "V2"]), &store).is_empty());
    }

    #[test]
    fn check_done_only_evaluates_done_tasks() {
        let mut store = Store::default();
        let mut wip = task("T1", &[]);
        wip.status = Status::Wip; // must be ignored even with bad cite
        wip.cites = vec!["V99".into()];
        store.tasks.push(wip);
        assert!(check_done(&store).is_empty());
    }

    #[test]
    fn check_done_fails_task_with_broken_cite() {
        let mut store = Store::default();
        store.tasks.push(task("T1", &["V99"]));
        let reports = check_done(&store);
        assert_eq!(reports.len(), 1);
        assert!(!reports[0].ok);
        assert_eq!(reports[0].broken_cites, vec!["V99".to_string()]);
    }
}
