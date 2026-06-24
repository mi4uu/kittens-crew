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

#[test]
fn score_emits_overall() {
    let dir = workspace("score");
    let o = run(&dir, &["score"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("overall"));
}
