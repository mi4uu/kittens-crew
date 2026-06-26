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

/// Whole-crate verify (the multi-file ceiling fix): per-node `rustc_compiles` only
/// type-checks each leaf as an isolated `lib`, so a plan split across files gets green
/// leaves but never assembles into a runnable program. This is the honest final gate —
/// it builds the actual binary the user asked for.
///
/// We find the crate root among the written leaves (the one file with a `fn main`) and
/// hand THAT to `rustc`. rustc resolves `mod foo;` to the sibling `foo.rs` itself, so a
/// properly-wired multi-file program links with no Cargo.toml. Returns:
///   - `Ok(None)`   — no `fn main` among the leaves → a pure library, nothing to assemble.
///   - `Ok(Some(p))`— a binary built at `p` (the program actually runs).
///   - `Err(detail)`— the crate root exists but the whole program does NOT build.
pub fn build_binary(written: &[std::path::PathBuf]) -> Result<Option<std::path::PathBuf>, String> {
    let root = written.iter().find(|p| {
        std::fs::read_to_string(p)
            .map(|s| s.contains("fn main"))
            .unwrap_or(false)
    });
    let Some(root) = root else {
        return Ok(None);
    };
    let stem = root.file_stem().and_then(|s| s.to_str()).unwrap_or("prog");
    // Place the binary next to the crate root (== the project dir for `run`), so a
    // single-file program lands as `./<stem>` exactly where the user expects it.
    let bin = root.with_file_name(stem);
    let out = Command::new("rustc")
        .args(["--edition", "2021", "-o"])
        .arg(&bin)
        .arg(root)
        .output()
        .map_err(|e| format!("rustc spawn failed: {e}"))?;
    if out.status.success() {
        Ok(Some(bin))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// Behavioural acceptance gate: run the built binary for each case and diff its
/// stdout (trimmed) against the expected output. This is what catches "compiles but
/// wrong" — e.g. a reverse-words CLI that leaks `args[0]` (the binary path) compiles
/// green but fails its own `[a,b,c] => "c b a"` case. Returns the first mismatch as a
/// detail the repair loop can feed back to the model.
pub fn run_accept(bin: &std::path::Path, cases: &[crate::store::AcceptCase]) -> Result<(), String> {
    // `Command::new` treats a bare name with no separator (e.g. "main", a single-file
    // program built at the workspace root) as a PATH lookup, not a cwd-relative file —
    // so it fails to find the binary we just built. Absolutise so it always runs the file.
    let bin = std::fs::canonicalize(bin).unwrap_or_else(|_| bin.to_path_buf());
    let bin = bin.as_path();
    for c in cases {
        let out = Command::new(bin)
            .args(&c.args)
            .output()
            .map_err(|e| format!("could not run {}: {e}", bin.display()))?;
        let got = String::from_utf8_lossy(&out.stdout);
        if got.trim() != c.stdout.trim() {
            return Err(format!(
                "accept case `{} {}` — expected `{}`, got `{}`",
                bin.file_name().and_then(|s| s.to_str()).unwrap_or("prog"),
                c.args.join(" "),
                c.stdout.trim(),
                got.trim()
            ));
        }
    }
    Ok(())
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

    /// The ceiling fix: two leaves that each lib-compile in isolation must ALSO assemble
    /// into a runnable binary. `main.rs` declares `mod helper;` → rustc pulls in helper.rs.
    #[test]
    fn whole_crate_assembles_multi_file_binary() {
        let dir = std::env::temp_dir().join(format!("ks_wc_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let helper = dir.join("helper.rs");
        let main = dir.join("main.rs");
        std::fs::write(&helper, "pub fn val() -> i64 { 42 }\n").unwrap();
        std::fs::write(&main, "mod helper;\nfn main() { println!(\"{}\", helper::val()); }\n").unwrap();
        // Order shouldn't matter — build_binary finds the `fn main` leaf as the root.
        let bin = build_binary(&[helper.clone(), main.clone()]).unwrap();
        assert!(bin.is_some(), "expected a binary");
        assert!(bin.unwrap().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The behavioural gate catches "compiles but wrong": a program that echoes its
    /// args verbatim passes a build but FAILS a reverse-words accept case.
    #[test]
    fn accept_catches_wrong_behaviour() {
        use crate::store::AcceptCase;
        let dir = std::env::temp_dir().join(format!("ks_acc_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // A buggy "reverse" that just re-prints args in order.
        let main = dir.join("main.rs");
        std::fs::write(
            &main,
            "fn main(){ let a:Vec<String>=std::env::args().skip(1).collect(); println!(\"{}\", a.join(\" \")); }\n",
        )
        .unwrap();
        let bin = build_binary(&[main.clone()]).unwrap().unwrap();
        let good = vec![AcceptCase { args: vec!["a".into(), "b".into()], stdout: "a b".into() }];
        assert!(run_accept(&bin, &good).is_ok());
        let rev = vec![AcceptCase { args: vec!["a".into(), "b".into()], stdout: "b a".into() }];
        let err = run_accept(&bin, &rev).unwrap_err();
        assert!(err.contains("expected `b a`"), "got {err}");
        assert!(err.contains("got `a b`"), "got {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Pure-library leaves (no `fn main`) → nothing to assemble, not an error.
    #[test]
    fn whole_crate_skips_library() {
        let dir = std::env::temp_dir().join(format!("ks_wclib_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let lib = dir.join("lib.rs");
        std::fs::write(&lib, "pub fn add(a: i64, b: i64) -> i64 { a + b }\n").unwrap();
        assert!(build_binary(&[lib]).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
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
