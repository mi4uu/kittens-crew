//! T25 — `.kittenscrew/spec.toml` authoritative store.
//!
//! Structured, deterministic source of truth (V9). SPEC.md = rendered
//! projection of this (T27, see `spec.rs`). Plan = DAG: `task.deps` are edges,
//! order is derived not stored (V13). Prose sections (§G/§C/§I) kept verbatim —
//! kittenscrew only reasons over tasks/invariants/bugs.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 1;
pub const STORE_PATH: &str = ".kittenscrew/spec.toml";

/// Task lifecycle. Renders to SPEC.md §T status cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Todo,
    Wip,
    Done,
    Killed,
}

impl Status {
    /// FORMAT.md §T symbol: `.` todo / `~` wip / `x` done / `∅` killed.
    pub fn symbol(self) -> char {
        match self {
            Status::Todo => '.',
            Status::Wip => '~',
            Status::Done => 'x',
            Status::Killed => '∅',
        }
    }

    /// Parse §T status cell symbol back to enum (import).
    pub fn from_symbol(c: char) -> Option<Status> {
        match c {
            '.' => Some(Status::Todo),
            '~' => Some(Status::Wip),
            'x' => Some(Status::Done),
            '∅' => Some(Status::Killed),
            _ => None,
        }
    }
}

fn default_priority() -> i64 {
    0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub status: Status,
    pub task: String,
    /// DAG edges — ids that must be `Done` before this is ready (V13).
    #[serde(default)]
    pub deps: Vec<String>,
    /// Tiebreak among READY tasks only (V15). Lower = sooner. toml-only, not projected to SPEC.md.
    #[serde(default = "default_priority")]
    pub priority: i64,
    /// §V/§I refs this task serves.
    #[serde(default)]
    pub cites: Vec<String>,
    /// Globs `check done` scans for fake-delivery (V18). toml-only.
    #[serde(default)]
    pub scope: Vec<String>,
    /// Ladder-kill reason when status = Killed (projected into §T cites cell).
    #[serde(default)]
    pub note: String,
    /// 1-5 contribution to §G/§V (T39, authored @ creation). 0 = unscored. toml-only.
    #[serde(default)]
    pub value: i64,
    /// 1-5 effort/complexity (T39). 0 = unscored. toml-only.
    #[serde(default)]
    pub difficulty: i64,
    /// 1-5 chance of rework/przypał (T39). 0 = none. toml-only.
    #[serde(default)]
    pub risk: i64,
    /// Self-eval filled @ done (T39, feeds value-variance V25). toml-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval: Option<TaskEval>,
    /// Behavioural acceptance cases (the done-oracle for a PROGRAM): given `args`,
    /// the built binary must print `stdout`. "Compiles" is the thinnest check for a
    /// library; for a program the honest check is "does it produce the right output".
    /// Empty = compile-only (the prior behaviour). toml-only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accept: Vec<AcceptCase>,
}

/// One behavioural acceptance case: run the binary with `args`, expect `stdout`
/// (compared trimmed). The planner emits these per program goal; they turn the
/// whole-crate gate from "it builds" into "it does what was asked".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptCase {
    #[serde(default)]
    pub args: Vec<String>,
    pub stdout: String,
}

/// Completion self-eval (V23/V25). Present only after a task is done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEval {
    /// 1-5 how happy with the delivered work.
    pub satisfaction: i64,
    /// 1-5 match to what the plan assumed.
    pub conformance: i64,
    /// Actual output tokens spent (0 = unrecorded).
    #[serde(default)]
    pub tokens: i64,
    /// Why it diverged, if it did.
    #[serde(default)]
    pub note: String,
}

impl Default for Task {
    fn default() -> Self {
        Task {
            id: String::new(),
            status: Status::Todo,
            task: String::new(),
            deps: Vec::new(),
            priority: default_priority(),
            cites: Vec::new(),
            scope: Vec::new(),
            note: String::new(),
            value: 0,
            difficulty: 0,
            risk: 0,
            eval: None,
            accept: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invariant {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bug {
    pub id: String,
    pub date: String,
    pub cause: String,
    pub fix: String,
}

fn default_schema() -> u32 {
    SCHEMA
}

/// The whole spec, structured. Authoritative (V9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// §G — opaque prose, verbatim.
    #[serde(default)]
    pub goal: String,
    /// §C — bullet lines, verbatim.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// §I — bullet lines, verbatim.
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default, rename = "invariant")]
    pub invariants: Vec<Invariant>,
    #[serde(default, rename = "task")]
    pub tasks: Vec<Task>,
    #[serde(default, rename = "bug")]
    pub bugs: Vec<Bug>,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            schema: SCHEMA,
            goal: String::new(),
            constraints: Vec::new(),
            interfaces: Vec::new(),
            invariants: Vec::new(),
            tasks: Vec::new(),
            bugs: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("toml serialize: {0}")]
    Ser(#[from] toml::ser::Error),
}

impl Store {
    /// Load from `.kittenscrew/spec.toml`. Missing file → default empty store.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Serialize to TOML and write, creating parent dir.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Lookup task by id.
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn sample() -> Store {
        Store {
            schema: SCHEMA,
            goal: "kittenscrew — Rust core".into(),
            constraints: vec!["Rust ≥ 1.75".into()],
            interfaces: vec!["cmd: `spec apply` → stdin JSON".into()],
            invariants: vec![Invariant {
                id: "V9".into(),
                text: "spec.toml authoritative".into(),
            }],
            tasks: vec![
                Task {
                    id: "T9".into(),
                    status: Status::Done,
                    task: "spec read".into(),
                    deps: vec![],
                    priority: 0,
                    cites: vec!["§I".into()],
                    scope: vec!["src/spec.rs".into()],
                    note: String::new(),
                    ..Default::default()
                },
                Task {
                    id: "T10".into(),
                    status: Status::Todo,
                    task: "spec apply".into(),
                    deps: vec!["T9".into()],
                    priority: 1,
                    cites: vec!["V3".into()],
                    scope: vec![],
                    note: String::new(),
                    ..Default::default()
                },
            ],
            bugs: vec![Bug {
                id: "B1".into(),
                date: "2026-04-20".into(),
                cause: "token < not ≤".into(),
                fix: "V2".into(),
            }],
        }
    }

    #[test]
    fn round_trip_preserves_everything() {
        let original = sample();
        let toml_text = toml::to_string_pretty(&original).unwrap();
        let parsed: Store = toml::from_str(&toml_text).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn status_symbols_match_format() {
        for s in [Status::Todo, Status::Wip, Status::Done, Status::Killed] {
            assert_eq!(Status::from_symbol(s.symbol()), Some(s));
        }
        assert_eq!(Status::from_symbol('?'), None);
    }

    #[test]
    fn missing_file_loads_default() {
        let s = Store::load(Path::new("/nonexistent/spec.toml")).unwrap();
        assert_eq!(s, Store::default());
    }

    #[test]
    fn save_then_load_is_identity() {
        let dir = std::env::temp_dir().join("kittenscrew_test_store");
        let path = dir.join("spec.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let original = sample();
        original.save(&path).unwrap();
        let loaded = Store::load(&path).unwrap();
        assert_eq!(original, loaded);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_lookup() {
        let s = sample();
        assert_eq!(s.task("T10").unwrap().deps, vec!["T9".to_string()]);
        assert!(s.task("T999").is_none());
    }
}
