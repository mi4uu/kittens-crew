//! T18 — end-to-end CLI tests: drive the real binary, assert the V1 exit-code
//! contract (0 ok / 2 validation / others). Each test runs in its own temp dir
//! with an isolated SPEC.md + store, so they don't touch the project's spec.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kittenscrew")
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn kittenscrew")
}

/// Run with a fully controlled environment (for V6: presence/absence of squeez).
fn run_env(dir: &Path, args: &[&str], envs: &[(&str, &str)], clear: bool) -> Output {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(dir);
    if clear {
        cmd.env_clear();
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn kittenscrew")
}

fn run_stdin(dir: &Path, args: &[&str], stdin: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

const SPEC: &str = "# SPEC\n\n## §V INVARIANTS\n\nV1: every cmd has an exit code\n\n## §T TASKS\n\nid|status|task|deps|cites\nT1|x|scaffold|-|V1\nT2|.|build feature|T1|V1\n";

/// Fresh isolated workspace, imported + rendered (so the sync guard is satisfied).
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ks-it-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SPEC.md"), SPEC).unwrap();
    assert!(run(&dir, &["spec", "import"]).status.success());
    assert!(run(&dir, &["spec", "render"]).status.success());
    dir
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

#[test]
fn version_exits_zero() {
    let o = run(&std::env::temp_dir(), &["--version"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("kittenscrew"));
}

#[test]
fn spec_check_clean_exits_zero() {
    let dir = workspace("check");
    assert_eq!(code(&run(&dir, &["spec", "check"])), 0);
}

#[test]
fn plan_next_returns_ready_task() {
    let dir = workspace("next");
    let o = run(&dir, &["plan", "next"]);
    assert_eq!(code(&o), 0);
    // T1 done, T2 ready → next is T2.
    assert!(String::from_utf8_lossy(&o.stdout).contains("T2"));
}

#[test]
fn apply_valid_diff_exits_zero() {
    let dir = workspace("apply-ok");
    let o = run_stdin(
        &dir,
        &["spec", "apply"],
        r#"{"section":"§T","op":"done","payload":{"id":"T2"}}"#,
    );
    assert_eq!(code(&o), 0);
}

#[test]
fn apply_bad_dep_exits_two() {
    // V3/V1: a diff that breaks a §V structural rule → exit 2, SPEC.md unchanged.
    let dir = workspace("apply-bad");
    let o = run_stdin(
        &dir,
        &["spec", "apply"],
        r#"{"section":"§T","op":"add","payload":{"task":"x","deps":["T999"]}}"#,
    );
    assert_eq!(code(&o), 2);
}

#[test]
fn apply_malformed_json_exits_two() {
    // V11: malformed JSON → exit 2.
    let dir = workspace("apply-json");
    let o = run_stdin(&dir, &["spec", "apply"], "not json at all");
    assert_eq!(code(&o), 2);
}

// --- T16 init (V6 squeez gate, isolation, idempotency) ---

/// Fresh empty workspace with an isolated `--target` subdir (no SPEC needed).
fn init_workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ks-init-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("claude")).unwrap();
    dir
}

#[test]
fn init_without_squeez_exits_three() {
    // V6: no reachable squeez → exit 3, nothing written. Cleared env so neither
    // PATH, SQUEEZ_BIN, nor $HOME/.claude/squeez can resolve.
    let dir = init_workspace("no-squeez");
    let target = dir.join("claude");
    let o = run_env(
        &dir,
        &["init", "--target", target.to_str().unwrap()],
        &[("HOME", dir.to_str().unwrap())],
        true,
    );
    assert_eq!(code(&o), 3);
    assert!(!target.join("settings.json").exists());
    assert!(!dir.join("kittenscrew.toml").exists());
}

#[test]
fn init_dry_run_writes_nothing() {
    let dir = init_workspace("dry");
    let target = dir.join("claude");
    // SQUEEZ_BIN points at a real file (the test binary) → V6 satisfied.
    let o = run_env(
        &dir,
        &["init", "--target", target.to_str().unwrap(), "--dry-run"],
        &[("SQUEEZ_BIN", bin())],
        true,
    );
    assert_eq!(code(&o), 0);
    assert!(!target.join("settings.json").exists());
    assert!(!dir.join("kittenscrew.toml").exists());
}

#[test]
fn init_registers_membrane_and_is_idempotent() {
    let dir = init_workspace("wire");
    let target = dir.join("claude");
    let args = ["init", "--target", target.to_str().unwrap()];
    let env = [("SQUEEZ_BIN", bin())];

    let o = run_env(&dir, &args, &env, true);
    assert_eq!(code(&o), 0);
    let settings = std::fs::read_to_string(target.join("settings.json")).unwrap();
    assert!(settings.contains("hook session-start"));
    assert!(settings.contains("PreToolUse"));
    assert!(dir.join("kittenscrew.toml").exists());

    // Re-run: still exit 0, no duplicate entries (idempotent membrane).
    let o2 = run_env(&dir, &args, &env, true);
    assert_eq!(code(&o2), 0);
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(target.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
}

#[test]
fn score_emits_overall() {
    let dir = workspace("score");
    let o = run(&dir, &["score"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("overall"));
}
