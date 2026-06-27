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
        .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-o"])
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

/// Language-aware per-node check (the multi-language seam): which deterministic
/// "is this leaf well-formed?" gate to run depends ONLY on the file extension, so a
/// `.py` leaf gets a Python syntax check and a `.rs` leaf gets the rustc gate — without
/// the drive loop having to know any language details. WHY extension-keyed: the planner
/// already scopes one concrete path per leaf, so the path itself is the cheapest, most
/// reliable language signal (no content sniffing, no guessing).
///
///   - `.py`  → `python3 -m py_compile <path>` (byte-compiles to AST; exit 0 = syntax ok).
///   - `.rs`  → the existing `rustc_compiles` lib gate.
///   - other  → fall back to `rustc_compiles` (today's behaviour, unchanged).
pub fn check_leaf(path: &Path) -> Verdict {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py") => py_compiles(path),
        _ => rustc_compiles(path),
    }
}

/// Python's analogue of `rustc_compiles`: byte-compile the script with the stdlib's
/// `py_compile` module. Exit 0 = the file parses (the thinnest honest "it's not garbage"
/// gate for a script, mirroring rustc's metadata-only type-check). A non-zero exit carries
/// the SyntaxError on stderr, which the repair loop feeds back verbatim.
fn py_compiles(file: &Path) -> Verdict {
    let out = Command::new("python3")
        .args(["-m", "py_compile"])
        .arg(file)
        .output();
    match out {
        Ok(o) if o.status.success() => Verdict {
            ok: true,
            detail: "py-compiles".into(),
        },
        Ok(o) => Verdict {
            ok: false,
            detail: String::from_utf8_lossy(&o.stderr).into_owned(),
        },
        Err(e) => Verdict {
            ok: false,
            detail: format!("python3 spawn failed: {e}"),
        },
    }
}

/// The language-aware "how do I RUN this program?" seam. Returns the argv PREFIX that
/// executes the program root — the command + any leading args — so the behavioural
/// `run_accept` gate can append each case's args and diff stdout, regardless of language.
/// This generalises the old "build a binary, then exec it" path: a compiled language
/// (Rust) yields the built binary's path; an interpreted one (Python) yields the
/// interpreter + script. Returns:
///   - `Ok(Some(argv))` — `argv` runs the program (e.g. `[bin]` or `[python3, script.py]`).
///   - `Ok(None)`       — `root` is not a runnable program entry (a pure library/module).
///   - `Err(detail)`    — it IS a program root but failed to build (Rust only).
pub fn program_runner(root: &Path) -> Result<Option<Vec<String>>, String> {
    match root.extension().and_then(|e| e.to_str()) {
        Some("py") => py_runner(root),
        _ => {
            // Rust: the runner IS the freshly-built binary. Reuse build_binary so the
            // single-file build logic stays in one place; absolutise its path so it execs
            // from any cwd (Command treats a bare stem as a PATH lookup, not cwd-relative).
            match build_binary(std::slice::from_ref(&root.to_path_buf()))? {
                Some(bin) => {
                    let bin = std::fs::canonicalize(&bin).unwrap_or(bin);
                    Ok(Some(vec![bin.to_string_lossy().into_owned()]))
                }
                None => Ok(None),
            }
        }
    }
}

/// Python's branch of `program_runner`: the script IS the program — no compile step —
/// so the runner is simply `python3 <abs path>`. We treat a `.py` leaf as a runnable
/// program root (vs a pure imported module with nothing to execute) when it has a clear
/// entry signal: a `__main__` guard, a `sys.argv` read, or a top-level `print(`. A module
/// with none of these has no observable stdout to accept, so it returns `Ok(None)` — the
/// same "nothing to assemble" semantics the Rust library case has.
fn py_runner(root: &Path) -> Result<Option<Vec<String>>, String> {
    let code = std::fs::read_to_string(root)
        .map_err(|e| format!("could not read {}: {e}", root.display()))?;
    if !is_python_program(&code) {
        return Ok(None);
    }
    // Absolutise so `python3 <path>` runs regardless of the harness's cwd.
    let abs = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    Ok(Some(vec!["python3".into(), abs.to_string_lossy().into_owned()]))
}

/// Cheap, conservative "is this `.py` a runnable script?" signal — shared by `py_runner`
/// and the drive loop's per-node program detection so the two never disagree. A
/// `__main__` guard or a `sys.argv` read is an unambiguous program; a top-level `print(`
/// means the script produces stdout when run. A pure helper module (only `def`/`class`,
/// imported elsewhere) matches none → not a program root.
pub fn is_python_program(code: &str) -> bool {
    code.contains("__main__") || code.contains("sys.argv") || code.contains("print(")
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
        .args(["--edition", "2024", "-o"])
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

/// Behavioural acceptance gate: run the program for each case and diff its stdout
/// (trimmed) against the expected output. This is what catches "compiles but wrong" —
/// e.g. a reverse-words CLI that leaks `args[0]` (the binary path) compiles green but
/// fails its own `[a,b,c] => "c b a"` case. Returns the first mismatch as a detail the
/// repair loop can feed back to the model.
///
/// `runner` is the argv PREFIX from `program_runner` — the command plus any leading args
/// (e.g. `[bin]` for Rust, `[python3, script.py]` for Python). Each case's args are
/// appended to it, so this stays language-agnostic: it just runs `runner ++ case.args`.
pub fn run_accept(runner: &[String], cases: &[crate::store::AcceptCase]) -> Result<(), String> {
    let (passed, first_fail) = run_accept_count(runner, cases);
    match first_fail {
        Some(detail) if passed < cases.len() => Err(detail),
        _ => Ok(()),
    }
}

/// Scored variant of [`run_accept`]: runs EVERY case (doesn't stop at the first miss)
/// and returns `(how many passed, first failure detail)`. The count is what lets the
/// repair loop KEEP-BEST — only repair from an attempt that didn't regress — instead of
/// thrashing (each retry a different broken program). The detail is the behavioural diff
/// (`input → expected X, got Y`) fed back to the model so it patches the actual bug.
pub fn run_accept_count(
    runner: &[String],
    cases: &[crate::store::AcceptCase],
) -> (usize, Option<String>) {
    let Some((prog, prefix)) = runner.split_first() else {
        return (0, Some("empty runner argv".into()));
    };
    let label = std::path::Path::new(runner.last().map(String::as_str).unwrap_or("prog"))
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("prog")
        .to_string();
    let mut passed = 0usize;
    let mut first_fail = None;
    for c in cases {
        let got = match run_capped(prog, prefix, &c.args, std::time::Duration::from_secs(5)) {
            Ok(out) => out,
            Err(e) => {
                // A timeout is a real failure mode (a model that writes an infinite loop —
                // e.g. a parser whose `while let Some(op) = peek()` never advances). Report it
                // as the behavioural failure so the repair loop fixes the loop, not as a hang.
                if first_fail.is_none() {
                    first_fail = Some(format!("running `{} {}` {e}", label, c.args.join(" ")));
                }
                continue;
            }
        };
        if got.trim() == c.stdout.trim() {
            passed += 1;
        } else if first_fail.is_none() {
            first_fail = Some(format!(
                "running `{} {}` printed `{}` but should print `{}`",
                label,
                c.args.join(" "),
                got.trim(),
                c.stdout.trim()
            ));
        }
    }
    (passed, first_fail)
}

/// Run a program with a wall-clock CAP and capture stdout. Plain `Command::output()` blocks
/// forever if the program never exits, so an accept case against a model-written infinite loop
/// would hang the whole harness. We spawn, read stdout on a thread, poll for exit until the
/// deadline, and KILL on timeout — turning a hang into a deterministic failure the repair loop
/// can act on. `Err` = the program didn't exit in time (or couldn't be spawned).
fn run_capped(
    prog: &str,
    prefix: &[String],
    args: &[String],
    cap: std::time::Duration,
) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = Command::new(prog)
        .args(prefix)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start: {e}"))?;
    // Drain stdout on a thread so a program that fills the pipe buffer can't deadlock.
    let mut out = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let deadline = std::time::Instant::now() + cap;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(format!("timed out after {}s (infinite loop?)", cap.as_secs()));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    }
    Ok(reader.join().unwrap_or_default())
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

    /// An infinite-loop program must not hang the harness: run_accept_count caps each run
    /// and reports a timeout as a failure (not a hang). Mirrors the live Q6 finding where a
    /// model wrote a parser whose `while let Some(op) = peek()` never advanced.
    #[test]
    fn run_accept_times_out_infinite_loop() {
        use crate::store::AcceptCase;
        let dir = std::env::temp_dir().join(format!("ks_to_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let main = dir.join("main.rs");
        std::fs::write(&main, "fn main(){ loop {} }\n").unwrap();
        let bin = build_binary(&[main.clone()]).unwrap().unwrap();
        let runner = vec![bin.to_string_lossy().into_owned()];
        let cases = vec![AcceptCase { args: vec![], stdout: "x".into() }];
        let (passed, fail) = run_accept_count(&runner, &cases);
        assert_eq!(passed, 0);
        assert!(fail.unwrap().contains("timed out"), "should report a timeout");
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
        let runner = vec![bin.to_string_lossy().into_owned()];
        let good = vec![AcceptCase { args: vec!["a".into(), "b".into()], stdout: "a b".into() }];
        assert!(run_accept(&runner, &good).is_ok());
        let rev = vec![AcceptCase { args: vec!["a".into(), "b".into()], stdout: "b a".into() }];
        let err = run_accept(&runner, &rev).unwrap_err();
        assert!(err.contains("should print `b a`"), "got {err}");
        assert!(err.contains("printed `a b`"), "got {err}");
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

    /// Skip the Python tests when no interpreter is on PATH so CI without python3 still
    /// passes (the seam is additive — its absence must never fail the Rust-only suite).
    fn have_python3() -> bool {
        Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn tmp_py(name: &str, src: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("ks_verify_{}_{name}.py", std::process::id()));
        std::fs::write(&p, src).unwrap();
        p
    }

    /// The Python analogue of `valid_rust_compiles`: a syntactically valid script passes
    /// `check_leaf` (which dispatches `.py` → `python3 -m py_compile`).
    #[test]
    fn py_compiles_valid() {
        if !have_python3() {
            return;
        }
        let p = tmp_py("ok", "import sys\nprint(' '.join(sys.argv[1:]))\n");
        assert!(check_leaf(&p).ok, "valid python must pass check_leaf");
        let _ = std::fs::remove_file(p);
    }

    /// A broken `.py` is rejected with the SyntaxError detail (the repair loop's feedback).
    #[test]
    fn py_rejects_broken() {
        if !have_python3() {
            return;
        }
        let p = tmp_py("bad", "def f(:\n    pass\n");
        let v = check_leaf(&p);
        assert!(!v.ok, "broken python must fail check_leaf");
        assert!(!v.detail.is_empty());
        let _ = std::fs::remove_file(p);
    }

    /// `program_runner` for a runnable script returns the `python3 <abs path>` argv prefix
    /// (no compile step — the script IS the program).
    #[test]
    fn program_runner_python_returns_python3_argv() {
        let p = tmp_py("run", "import sys\nprint(len(sys.argv))\n");
        let runner = program_runner(&p).unwrap().expect("script is a program root");
        assert_eq!(runner[0], "python3");
        assert!(runner[1].ends_with(".py"), "second argv element is the script path: {runner:?}");
        // A pure module (no entry signal) is NOT a program root → Ok(None).
        let lib = tmp_py("lib", "def helper():\n    return 1\n");
        assert!(program_runner(&lib).unwrap().is_none(), "pure module is not runnable");
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(lib);
    }

    /// End-to-end Python behavioural gate: build the runner, run an accept case, diff stdout.
    #[test]
    fn py_run_accept_diffs_stdout() {
        if !have_python3() {
            return;
        }
        use crate::store::AcceptCase;
        let p = tmp_py("acc", "import sys\nprint(' '.join(reversed(sys.argv[1:])))\n");
        let runner = program_runner(&p).unwrap().unwrap();
        let good = vec![AcceptCase { args: vec!["a".into(), "b".into(), "c".into()], stdout: "c b a".into() }];
        assert!(run_accept(&runner, &good).is_ok());
        let bad = vec![AcceptCase { args: vec!["a".into(), "b".into()], stdout: "a b".into() }];
        assert!(run_accept(&runner, &bad).unwrap_err().contains("should print `a b`"));
        let _ = std::fs::remove_file(p);
    }
}
