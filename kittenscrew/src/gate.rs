//! T57 — the plan-gate: *no plan → no work*. The benchmark caught a weak model
//! free-building with no plan (wandering, overclaiming, burning tokens). The
//! membrane now refuses code edits until a plan store exists: the request is
//! first distilled into tasks, confirmed, saved — THEN the build unlocks. Pure
//! decision logic; the hook wires it to the PreToolUse deny + intake steer.

use std::path::Path;

/// Is there a saved plan for this project? = a `.kittenscrew/spec.toml` with ≥1 task.
pub fn plan_exists() -> bool {
    crate::store::Store::load(Path::new(crate::store::STORE_PATH))
        .map(|s| !s.tasks.is_empty())
        .unwrap_or(false)
}

/// Product code we gate. Plan/scaffold artifacts (SPEC.md, configs, docs) are NOT
/// gated — the agent must be able to write the plan + scaffold to escape the gate.
const CODE_EXT: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "kt", "kts", "c", "h", "cpp", "hpp", "cc",
    "rb", "php", "swift", "scala", "cs", "ml", "ex", "exs", "clj", "lua", "dart", "zig",
];

/// Does this path point at product code (vs a plan/scaffold/doc/config file)?
pub fn is_code_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.eq_ignore_ascii_case("SPEC.md") {
        return false; // the plan projection itself
    }
    match name.rsplit_once('.') {
        Some((_, ext)) => CODE_EXT.iter().any(|e| e.eq_ignore_ascii_case(ext)),
        None => false, // extensionless (Makefile, Dockerfile, …) = not gated
    }
}

/// Should this tool call be blocked for lack of a plan? Only mutating writes to
/// product code; reads, bash, and plan/scaffold writes pass.
pub fn blocks(tool: &str, path: &str) -> bool {
    matches!(tool, "Write" | "Edit" | "MultiEdit") && is_code_path(path)
}

/// The deny reason / planning steer — tells the agent exactly how to escape the
/// gate (make the plan), so it routes to planning instead of getting stuck.
pub const PLAN_STEER: &str = "no plan yet — distill this request into tasks FIRST: \
`kittenscrew spec apply` with §T add ops (or draft SPEC.md then `kittenscrew spec import`), \
confirm scope with the user, then build. No plan → no work (T57).";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_product_code_not_plan_artifacts() {
        assert!(is_code_path("src/main.rs"));
        assert!(is_code_path("/work/feedcat/src/cache.rs"));
        assert!(is_code_path("app.py"));
        // NOT gated: plan, docs, config, scaffold.
        assert!(!is_code_path("SPEC.md"));
        assert!(!is_code_path("README.md"));
        assert!(!is_code_path("Cargo.toml"));
        assert!(!is_code_path(".kittenscrew/spec.toml"));
        assert!(!is_code_path("Dockerfile"));
    }

    #[test]
    fn blocks_only_code_writes() {
        assert!(blocks("Write", "src/lib.rs"));
        assert!(blocks("Edit", "main.go"));
        assert!(!blocks("Write", "SPEC.md")); // plan write allowed
        assert!(!blocks("Read", "src/lib.rs")); // reads allowed
        assert!(!blocks("Bash", "anything")); // commands allowed
    }
}
