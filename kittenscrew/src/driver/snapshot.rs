//! P1.5 — minimal git snapshot/rollback: a cheap safety net for autonomous runs.
//!
//! Snapshot the working tree BEFORE driving; roll back to undo a bad run. This is the
//! minimal correct subset of the full shadow-git engine (T87): it captures tracked
//! changes via `git stash create` (which does NOT touch the working tree) plus the set
//! of currently-untracked paths, and on rollback restores tracked files to the snapshot
//! and removes ONLY the files the run newly created (untracked-after minus
//! untracked-before). It never runs `git clean -fd` (which would nuke the user's own
//! untracked files), so it is safe to call by default.
//!
//! ponytail: T87 upgrades this to a run_id-memoized shadow ref that also restores
//! untracked-file deletions and couples loop/conversation state. This subset covers the
//! "agent wrote some files and we want to undo them" case the MVP actually needs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn untracked(repo: &Path) -> HashSet<PathBuf> {
    git(repo, &["ls-files", "--others", "--exclude-standard"])
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// A captured working-tree state. `tracked_sha` is a `git stash create` commit (None if
/// the tree had no tracked changes); `untracked_before` is the set of untracked paths at
/// snapshot time, used to distinguish the run's new files from the user's pre-existing ones.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub tracked_sha: Option<String>,
    pub untracked_before: HashSet<PathBuf>,
}

/// Capture the working tree WITHOUT modifying it.
pub fn snapshot(repo: &Path) -> Result<Snapshot, String> {
    let sha = git(repo, &["stash", "create"])?; // "" when there are no tracked changes
    Ok(Snapshot {
        tracked_sha: if sha.is_empty() { None } else { Some(sha) },
        untracked_before: untracked(repo),
    })
}

/// Restore tracked files to the snapshot and remove files the run newly created.
pub fn rollback(repo: &Path, snap: &Snapshot) -> Result<(), String> {
    match &snap.tracked_sha {
        // Restore tracked working files to the captured tree.
        Some(sha) => git(repo, &["checkout", sha, "--", "."]).map(|_| ())?,
        // Tree was clean → discard any tracked modifications made since.
        None => git(repo, &["checkout", "--", "."]).map(|_| ())?,
    }
    // Remove files that are untracked NOW but were not at snapshot time (the run's output).
    for p in untracked(repo) {
        if !snap.untracked_before.contains(&p) {
            let _ = std::fs::remove_file(repo.join(&p));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok(repo: &Path, args: &[&str]) {
        git(repo, args).unwrap();
    }

    /// Snapshot → modify a tracked file + create a new file → rollback restores the
    /// tracked file and removes the new file, while leaving pre-existing untracked alone.
    #[test]
    fn rollback_restores_tracked_and_removes_new() {
        let repo = std::env::temp_dir().join(format!("ks_snap_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        git_ok(&repo, &["init", "-q"]);
        git_ok(&repo, &["config", "user.email", "t@t"]);
        git_ok(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("a.txt"), "one").unwrap();
        std::fs::write(repo.join("keep.txt"), "mine").unwrap(); // pre-existing untracked
        git_ok(&repo, &["add", "a.txt"]);
        git_ok(&repo, &["commit", "-q", "-m", "init"]);

        let snap = snapshot(&repo).unwrap();

        // The "run" makes a mess: edits a tracked file + writes a new scope file.
        std::fs::write(repo.join("a.txt"), "TWO").unwrap();
        std::fs::write(repo.join("new.rs"), "garbage").unwrap();

        rollback(&repo, &snap).unwrap();

        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "one", "tracked file restored");
        assert!(!repo.join("new.rs").exists(), "run's new file removed");
        assert!(repo.join("keep.txt").exists(), "pre-existing untracked file untouched");
        let _ = std::fs::remove_dir_all(&repo);
    }
}
