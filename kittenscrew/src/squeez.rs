//! squeez binary + hook scripts detection + invocation. V2: graceful degrade.

use crate::error::KittenError;
use std::io::Write;
use std::process::{Command, Stdio};

/// Locate squeez binary. Order: $SQUEEZ_BIN, $PATH, ~/.claude/squeez/bin/squeez.
pub fn bin() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("SQUEEZ_BIN") {
        let pb = std::path::PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    if let Some(p) = which("squeez") {
        return Some(p);
    }
    let home = std::env::var("HOME").ok()?;
    let p = std::path::PathBuf::from(home).join(".claude/squeez/bin/squeez");
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

/// Locate squeez hook scripts dir. Returns path to `.../squeez/hooks/`.
pub fn hooks_dir() -> Option<std::path::PathBuf> {
    let bin = bin()?;
    // bin is at <prefix>/squeez/bin/squeez → hooks at <prefix>/squeez/hooks
    bin.parent()?.parent()?.parent().map(|p| p.join("hooks"))
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let candidate = std::path::PathBuf::from(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run `squeez <args...>` w/ stdin piped. Returns stdout.
/// Returns None if squeez missing (graceful degrade per V2).
pub fn run(args: &[&str], stdin: &str) -> Result<Option<String>, KittenError> {
    let bin = match bin() {
        Some(b) => b,
        None => return Ok(None),
    };
    let mut child = Command::new(&bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// Run squeez hook script (e.g. pretooluse.sh) w/ stdin piped.
/// Returns stdout. Returns None if script missing.
pub fn run_hook(event: &str, stdin: &str) -> Result<Option<String>, KittenError> {
    let hooks = match hooks_dir() {
        Some(d) => d,
        None => return Ok(None),
    };
    // Map event name to squeez hook script.
    let script = match event {
        "session-start" => hooks.join("session-start.sh"),
        "pre-tool" => hooks.join("pretooluse.sh"),
        "post-tool" => hooks.join("posttooluse.sh"),
        "subagent-stop" => hooks.join("subagentstop.sh"),
        "pre-compact" => hooks.join("precompact.sh"),
        "post-compact" => hooks.join("postcompact.sh"),
        _ => return Ok(None),
    };
    if !script.is_file() {
        return Ok(None);
    }
    let mut child = Command::new(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
}
