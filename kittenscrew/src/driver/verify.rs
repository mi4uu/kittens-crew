//! T63 — per-node verification: the deterministic done-oracle. This is the
//! differentiator (LLM-as-judge is 21–46% wrong on hard tasks and reward-hacks);
//! "done" here means a real, deterministic acceptance check passed, not "the model
//! said so". For a code leaf the cheapest honest check is "does it compile?" — a
//! `rustc` gate. A failed verify must STOP the loop from marking the node done
//! (bounded replan, T74, is the later escalation).

use std::path::Path;
use std::process::Command;

pub struct Verdict {
    pub ok: bool,
    pub detail: String,
}

/// Type-check a single Rust source file as a library crate — no linking, no deps,
/// no external crates. Deterministic pass/fail = the thinnest acceptance a code
/// leaf can have. (`--emit=metadata` stops after the front-end, so it's fast.)
///
/// rustc writes its temp dir alongside the `-o` target, so the output must land in
/// a writable dir (NOT `/dev/null` — rustc then tries to mkdir under `/dev`). We
/// emit a throwaway `.rmeta` into the OS temp dir and delete it.
pub fn rustc_compiles(file: &Path) -> Verdict {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("node");
    let meta = std::env::temp_dir().join(format!("ks_meta_{}_{}.rmeta", std::process::id(), stem));
    let out = Command::new("rustc")
        .args(["--edition", "2021", "--crate-type", "lib", "--emit=metadata", "-o"])
        .arg(&meta)
        .arg(file)
        .output();
    let _ = std::fs::remove_file(&meta);
    match out {
        Ok(o) if o.status.success() => Verdict {
            ok: true,
            detail: "compiles".into(),
        },
        Ok(o) => Verdict {
            ok: false,
            detail: String::from_utf8_lossy(&o.stderr).into_owned(),
        },
        Err(e) => Verdict {
            ok: false,
            detail: format!("rustc spawn failed: {e}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str, src: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("ks_verify_{name}.rs"));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(src.as_bytes()).unwrap();
        p
    }

    #[test]
    fn valid_rust_compiles() {
        let p = tmp("ok", "pub fn add(a: i64, b: i64) -> i64 { a + b }\n");
        assert!(rustc_compiles(&p).ok);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn broken_rust_fails_with_detail() {
        let p = tmp("bad", "pub fn add(a: i64, b: i64) -> i64 { a + }\n");
        let v = rustc_compiles(&p);
        assert!(!v.ok);
        assert!(!v.detail.is_empty());
        let _ = std::fs::remove_file(p);
    }
}
