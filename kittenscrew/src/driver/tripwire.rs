//! T64 — deterministic YOLO-safe tripwire gate.
//!
//! The harness operates YOLO by default (no per-tool dialogs). This module is the
//! negative filter that catches actions which "don't look right" before they execute.
//! Determinism: `evaluate` is a pure total function — same input → same verdict.
//! No model calls, no IO, no randomness. The caller owns all IO consequences.
//!
//! Default ruleset (safe-side defaults, operator may override):
//! | Tripwire        | Default action |
//! |-----------------|---------------|
//! | path-escape     | Block          |
//! | destructive     | Block          |
//! | secret-touch    | Block          |
//! | force-push      | Block          |
//! | oversized-diff  | Ask            |
//! | network-egress  | Flag           |

use std::path::{Path, PathBuf};

// ── public types ──────────────────────────────────────────────────────────────

/// The action the harness is about to take. `Command` wraps an arbitrary shell
/// string; `Write` represents a file-write operation with a known line count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Command(String),
    Write { path: PathBuf, lines: usize },
}

/// What the gate decided to do when a tripwire fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TripAction {
    /// Refuse — do not execute the action.
    Block,
    /// Pause — ask the operator before proceeding.
    Ask,
    /// Allow but record a snapshot / audit note.
    Flag,
}

/// Per-tripwire configuration + workspace bounds.
#[derive(Debug, Clone)]
pub struct Ruleset {
    pub workspace_root: PathBuf,
    /// Line threshold for the `oversized-diff` tripwire.
    pub oversized_diff_lines: usize,
    pub path_escape: TripAction,
    pub destructive: TripAction,
    pub secret_touch: TripAction,
    pub force_push: TripAction,
    pub oversized_diff: TripAction,
    pub network_egress: TripAction,
}

impl Ruleset {
    /// Sensible production defaults rooted at `workspace_root`.
    pub fn default_at(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            oversized_diff_lines: 500,
            path_escape: TripAction::Block,
            destructive: TripAction::Block,
            secret_touch: TripAction::Block,
            force_push: TripAction::Block,
            oversized_diff: TripAction::Ask,
            network_egress: TripAction::Flag,
        }
    }
}

/// Gate verdict. `tripwire == None` means the action is clean (ALLOW).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Which tripwire fired, or `None` for a clean pass.
    pub tripwire: Option<&'static str>,
    /// The configured action for the fired tripwire (`TripAction::Flag` for a clean
    /// pass — caller should treat `tripwire == None` as ALLOW regardless of this
    /// field, but we keep it coherent).
    pub action: TripAction,
    /// Human-readable explanation. Empty string on clean pass.
    pub reason: String,
}

impl Verdict {
    /// Convenience: true iff no tripwire fired.
    pub fn is_clean(&self) -> bool {
        self.tripwire.is_none()
    }

    fn clean() -> Self {
        Self { tripwire: None, action: TripAction::Flag, reason: String::new() }
    }

    fn fired(name: &'static str, action: TripAction, reason: String) -> Self {
        Self { tripwire: Some(name), action, reason }
    }
}

// ── evaluate — the total gate function ────────────────────────────────────────

/// Evaluate `action` against `rules`. Deterministic pure function — no IO, no side
/// effects. Precedence: tripwires are checked in order; the first match wins.
pub fn evaluate(rules: &Ruleset, action: &Action) -> Verdict {
    match action {
        Action::Write { path, lines } => {
            // 1. path-escape
            if let Some(v) = check_path_escape(rules, path) {
                return v;
            }
            // 2. secret-touch
            if let Some(v) = check_secret_touch(rules, path) {
                return v;
            }
            // 3. oversized-diff
            if *lines > rules.oversized_diff_lines {
                return Verdict::fired(
                    "oversized-diff",
                    rules.oversized_diff.clone(),
                    format!(
                        "write of {} lines exceeds threshold {}",
                        lines, rules.oversized_diff_lines
                    ),
                );
            }
            Verdict::clean()
        }
        Action::Command(cmd) => {
            // 1. force-push (before generic destructive so its message is specific)
            if is_force_push(cmd) {
                return Verdict::fired(
                    "force-push",
                    rules.force_push.clone(),
                    format!("force-push detected in command: {cmd}"),
                );
            }
            // 2. destructive
            if is_destructive(cmd) {
                return Verdict::fired(
                    "destructive",
                    rules.destructive.clone(),
                    format!("destructive command detected: {cmd}"),
                );
            }
            // 3. secret-touch — path argument in command touching a secret file
            if let Some(name) = secret_arg_in_command(cmd) {
                return Verdict::fired(
                    "secret-touch",
                    rules.secret_touch.clone(),
                    format!("command touches secret file '{name}': {cmd}"),
                );
            }
            // 4. network-egress
            if is_network_egress(cmd) {
                return Verdict::fired(
                    "network-egress",
                    rules.network_egress.clone(),
                    format!("network-egress command detected: {cmd}"),
                );
            }
            Verdict::clean()
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Canonicalize `path` (without requiring it to exist) and check it stays
/// under `workspace_root`. Uses lexical `starts_with` after resolving `..`
/// components so the check is purely deterministic.
fn check_path_escape(rules: &Ruleset, path: &Path) -> Option<Verdict> {
    let root = lexical_clean(&rules.workspace_root);
    // A RELATIVE target is interpreted relative to the workspace root (the normal case:
    // scope paths are project-relative). Only `..` segments can then escape upward.
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let resolved = lexical_clean(&abs);
    if !resolved.starts_with(&root) {
        return Some(Verdict::fired(
            "path-escape",
            rules.path_escape.clone(),
            format!(
                "path '{}' escapes workspace root '{}'",
                resolved.display(),
                root.display()
            ),
        ));
    }
    None
}

/// Lexically normalize a path (resolve `.` / `..` without filesystem access).
fn lexical_clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

fn check_secret_touch(rules: &Ruleset, path: &Path) -> Option<Verdict> {
    if is_secret_path(path) {
        let name = path.to_string_lossy().to_string();
        return Some(Verdict::fired(
            "secret-touch",
            rules.secret_touch.clone(),
            format!("path touches a secret file: {name}"),
        ));
    }
    None
}

/// True if the path looks like a secret file by name/extension.
fn is_secret_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    // Exact names
    matches!(
        name.as_str(),
        ".env"
            | "id_rsa"
            | "id_ed25519"
            | "id_ecdsa"
            | "id_dsa"
            | "credentials"
            | ".netrc"
            | ".htpasswd"
    ) || matches!(ext.as_str(), "pem" | "key" | "p12" | "pfx" | "crt" | "cer")
        || name.starts_with(".env.")
        || name.contains("secret")
        || name.contains("password")
        || name.contains("passwd")
        || name.contains("credentials")
}

/// Scan the tokens of `cmd` for any that look like secret files.
fn secret_arg_in_command(cmd: &str) -> Option<String> {
    for token in cmd.split_whitespace() {
        // strip common shell redirects/flags
        let tok = token.trim_start_matches('-');
        let path = Path::new(tok);
        if is_secret_path(path) {
            return Some(tok.to_string());
        }
    }
    None
}

/// Matches `rm`, `rm -rf`, `rmdir`, `unlink`, `del`.
fn is_destructive(cmd: &str) -> bool {
    let first = cmd.split_whitespace().next().unwrap_or("").trim_end_matches('/');
    // Also handle paths like `/bin/rm`
    let base = Path::new(first)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    matches!(base.as_str(), "rm" | "rmdir" | "unlink" | "del" | "shred")
}

/// Matches `git push --force` / `git push -f` in any position.
fn is_force_push(cmd: &str) -> bool {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    let has_git = tokens.first().map_or(false, |t| {
        Path::new(t).file_name().unwrap_or_default().to_string_lossy() == "git"
    });
    if !has_git {
        return false;
    }
    let has_push = tokens.iter().any(|t| *t == "push");
    let has_force = tokens.iter().any(|t| *t == "--force" || *t == "-f");
    has_push && has_force
}

/// curl, wget, nc, ssh, scp, rsync, ftp, sftp, netcat.
fn is_network_egress(cmd: &str) -> bool {
    let first = cmd.split_whitespace().next().unwrap_or("").trim_end_matches('/');
    let base = Path::new(first)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    matches!(
        base.as_str(),
        "curl" | "wget" | "nc" | "netcat" | "ssh" | "scp" | "rsync" | "ftp" | "sftp" | "telnet"
    )
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Ruleset {
        let root = std::env::temp_dir().join(format!("ks_trip_{}", std::process::id()));
        Ruleset::default_at(root)
    }

    // ── clean pass ───────────────────────────────────────────────────────────

    #[test]
    fn clean_command_passes() {
        let v = evaluate(&rules(), &Action::Command("cargo build".into()));
        assert!(v.is_clean(), "clean command must pass: {v:?}");
    }

    #[test]
    fn clean_write_inside_root_passes() {
        let r = rules();
        let inside = r.workspace_root.join("src/main.rs");
        let v = evaluate(&r, &Action::Write { path: inside, lines: 10 });
        assert!(v.is_clean(), "write inside root must pass: {v:?}");
    }

    // ── path-escape ──────────────────────────────────────────────────────────

    #[test]
    fn path_escape_blocked() {
        let r = rules();
        let outside = r.workspace_root.join("../../../etc/passwd");
        let v = evaluate(&r, &Action::Write { path: outside, lines: 1 });
        assert_eq!(v.tripwire, Some("path-escape"));
        assert_eq!(v.action, TripAction::Block);
    }

    #[test]
    fn path_inside_root_not_escape() {
        let r = rules();
        // A path that contains a `..` but resolves inside the root.
        let inside = r.workspace_root.join("src/../src/lib.rs");
        let v = evaluate(&r, &Action::Write { path: inside, lines: 5 });
        assert!(v.is_clean(), "resolved-inside path must not fire path-escape: {v:?}");
    }

    // ── destructive ──────────────────────────────────────────────────────────

    #[test]
    fn rm_rf_blocked() {
        let v = evaluate(&rules(), &Action::Command("rm -rf /tmp/foo".into()));
        assert_eq!(v.tripwire, Some("destructive"));
        assert_eq!(v.action, TripAction::Block);
    }

    #[test]
    fn rmdir_blocked() {
        let v = evaluate(&rules(), &Action::Command("rmdir /tmp/foo".into()));
        assert_eq!(v.tripwire, Some("destructive"));
        assert_eq!(v.action, TripAction::Block);
    }

    // ── secret-touch ─────────────────────────────────────────────────────────

    #[test]
    fn write_env_blocked() {
        let r = rules();
        let secret = r.workspace_root.join(".env");
        let v = evaluate(&r, &Action::Write { path: secret, lines: 3 });
        assert_eq!(v.tripwire, Some("secret-touch"));
        assert_eq!(v.action, TripAction::Block);
    }

    #[test]
    fn write_pem_blocked() {
        let r = rules();
        let secret = r.workspace_root.join("server.pem");
        let v = evaluate(&r, &Action::Write { path: secret, lines: 10 });
        assert_eq!(v.tripwire, Some("secret-touch"));
        assert_eq!(v.action, TripAction::Block);
    }

    #[test]
    fn command_touching_id_rsa_blocked() {
        let v = evaluate(&rules(), &Action::Command("cat id_rsa".into()));
        assert_eq!(v.tripwire, Some("secret-touch"));
        assert_eq!(v.action, TripAction::Block);
    }

    // ── oversized-diff ───────────────────────────────────────────────────────

    #[test]
    fn oversized_diff_asks() {
        let r = rules();
        let path = r.workspace_root.join("big.rs");
        let v = evaluate(&r, &Action::Write { path, lines: r.oversized_diff_lines + 1 });
        assert_eq!(v.tripwire, Some("oversized-diff"));
        assert_eq!(v.action, TripAction::Ask);
    }

    #[test]
    fn exactly_at_threshold_passes() {
        let r = rules();
        let path = r.workspace_root.join("ok.rs");
        // exactly at the limit is fine — only strictly over triggers
        let v = evaluate(&r, &Action::Write { path, lines: r.oversized_diff_lines });
        assert!(v.is_clean(), "exactly-at-threshold must not fire: {v:?}");
    }

    // ── network-egress ───────────────────────────────────────────────────────

    #[test]
    fn curl_flagged() {
        let v = evaluate(&rules(), &Action::Command("curl https://example.com".into()));
        assert_eq!(v.tripwire, Some("network-egress"));
        assert_eq!(v.action, TripAction::Flag);
    }

    #[test]
    fn wget_flagged() {
        let v = evaluate(&rules(), &Action::Command("wget https://example.com -O out.tar".into()));
        assert_eq!(v.tripwire, Some("network-egress"));
        assert_eq!(v.action, TripAction::Flag);
    }

    // ── force-push ───────────────────────────────────────────────────────────

    #[test]
    fn force_push_long_blocked() {
        let v = evaluate(&rules(), &Action::Command("git push --force".into()));
        assert_eq!(v.tripwire, Some("force-push"));
        assert_eq!(v.action, TripAction::Block);
    }

    #[test]
    fn force_push_short_blocked() {
        let v = evaluate(&rules(), &Action::Command("git push origin main -f".into()));
        assert_eq!(v.tripwire, Some("force-push"));
        assert_eq!(v.action, TripAction::Block);
    }

    #[test]
    fn normal_push_passes() {
        let v = evaluate(&rules(), &Action::Command("git push origin main".into()));
        assert!(v.is_clean(), "normal push must not fire force-push: {v:?}");
    }
}
