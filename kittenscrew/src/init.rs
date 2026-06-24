//! T16 — `kittenscrew init`: write a `kittenscrew.toml` template + register the
//! hook membrane in `<target>/settings.json`. V6: hooks are registered only
//! after `squeez` is verified reachable (else exit 3, handled by the caller).
//!
//! Safe by construction: `--target <dir>` isolates the settings write (the real
//! `~/.claude` is just the default, never assumed), and `--dry-run` reports the
//! plan without touching disk. Re-running is idempotent — entries already
//! routing to `kittenscrew hook …` are left untouched, never duplicated.

use crate::config::Config;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The hook membrane: `(Claude Code event, kittenscrew sub-event, matcher)`.
/// ONLY events that `hook::dispatch` actually handles — registering an event
/// with no handler would make every session emit "unknown hook event". As the
/// driver lands, T51 adds `UserPromptSubmit`/`user-prompt` and T52 adds
/// `Stop`/`stop` here, in lockstep with their dispatch arms (V33: init is the
/// single place the membrane is wired).
pub const MEMBRANE: &[(&str, &str, &str)] = &[
    (
        "SessionStart",
        "session-start",
        "startup|resume|clear|compact",
    ),
    ("UserPromptSubmit", "user-prompt", ""),
    ("PreToolUse", "pre-tool", ""),
    ("PostToolUse", "post-tool", ""),
    ("Stop", "stop", ""),
    ("SubagentStop", "subagent-stop", ""),
    ("PreCompact", "pre-compact", ""),
    ("PostCompact", "post-compact", ""),
];

const CONFIG_FILE: &str = "kittenscrew.toml";
const SETTINGS_FILE: &str = "settings.json";

/// What `init` did (or, under `--dry-run`, would do). Pure data — the caller
/// renders it through the kitty voice.
#[derive(Debug, PartialEq)]
pub struct Report {
    pub config_path: PathBuf,
    pub config_written: bool, // false = kept existing (no --force)
    pub settings_path: PathBuf,
    pub registered: Vec<String>, // events newly added this run
    pub already: Vec<String>,    // events already wired (idempotent)
    pub dry_run: bool,
}

/// Why init refused. `SqueezMissing` maps to exit 3 (V6); `Io` to exit 1.
#[derive(Debug)]
pub enum InitError {
    SqueezMissing,
    Io(std::io::Error),
}
impl From<std::io::Error> for InitError {
    fn from(e: std::io::Error) -> Self {
        InitError::Io(e)
    }
}

/// The `kittenscrew.toml` template: the serialized default config (so it can
/// never drift from the schema) under a header pointing at the real docs.
pub fn config_template() -> String {
    let body =
        toml::to_string_pretty(&Config::default()).expect("Config::default always serializes");
    format!(
        "# kittenscrew.toml — per-project config (every field defaults; delete\n\
         # what you don't override). Distinct from .kittenscrew/spec.toml (the\n\
         # plan store). Schema + command reference: README.md.\n\n{body}"
    )
}

/// Run init against `target` (the dir holding `settings.json`; the caller
/// resolves the default to `~/.claude`). `squeez_ok` is the V6 gate: the caller
/// passes whether `squeez` was found. `force` overwrites an existing config.
pub fn run(
    target: &Path,
    squeez_ok: bool,
    dry_run: bool,
    force: bool,
) -> Result<Report, InitError> {
    // V6: never register hooks without a reachable squeez — exit 3.
    if !squeez_ok {
        return Err(InitError::SqueezMissing);
    }

    let cmd_base = exe_invocation();

    // --- kittenscrew.toml (project-local, CWD) ---
    let config_path = PathBuf::from(CONFIG_FILE);
    let config_exists = config_path.is_file();
    let config_written = force || !config_exists;
    if config_written && !dry_run {
        std::fs::write(&config_path, config_template())?;
    }

    // --- <target>/settings.json hook membrane ---
    let settings_path = target.join(SETTINGS_FILE);
    let mut settings = read_settings(&settings_path)?;
    let (registered, already) = merge_membrane(&mut settings, &cmd_base);
    if !registered.is_empty() && !dry_run {
        if let Some(dir) = settings_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let pretty = serde_json::to_string_pretty(&settings).expect("settings serialize");
        std::fs::write(&settings_path, format!("{pretty}\n"))?;
    }

    Ok(Report {
        config_path,
        config_written,
        settings_path,
        registered,
        already,
        dry_run,
    })
}

/// Absolute invocation of THIS binary, so the registered hook command works
/// regardless of `PATH` (Docker arm, dev `target/debug`, real install all fine).
/// ponytail: a path with spaces would need shell quoting — our install paths
/// don't have any; quote here if that ever changes.
fn exe_invocation() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "kittenscrew".into())
}

/// Parse `<target>/settings.json`; an absent file is an empty object (we create
/// it). A present-but-malformed file is a hard IO-class error — we will not
/// silently overwrite a settings file we failed to understand.
fn read_settings(path: &Path) -> Result<Value, InitError> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(json!({})),
        Ok(s) => serde_json::from_str(&s).map_err(|e| {
            InitError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: malformed JSON: {e}", path.display()),
            ))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(InitError::Io(e)),
    }
}

/// Merge our membrane into `settings.hooks`, preserving every existing entry.
/// Returns `(newly_registered, already_present)` event names. Idempotent: an
/// event whose array already contains a `kittenscrew hook <sub>` command is left
/// exactly as-is.
fn merge_membrane(settings: &mut Value, cmd_base: &str) -> (Vec<String>, Vec<String>) {
    let mut registered = Vec::new();
    let mut already = Vec::new();

    let root = settings.as_object_mut().expect("settings is an object");
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("hooks is an object");

    for (event, sub, matcher) in MEMBRANE {
        let command = format!("{cmd_base} hook {sub}");
        let arr = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("event maps to an array");

        if arr.iter().any(|e| routes_to(e, sub)) {
            already.push(event.to_string());
            continue;
        }

        let mut entry = serde_json::Map::new();
        if !matcher.is_empty() {
            entry.insert("matcher".into(), json!(matcher));
        }
        entry.insert(
            "hooks".into(),
            json!([{ "type": "command", "command": command, "timeout": 30 }]),
        );
        arr.push(Value::Object(entry));
        registered.push(event.to_string());
    }

    (registered, already)
}

/// Does this settings entry already route to `kittenscrew hook <sub>`? Marker:
/// any nested hook command containing `hook <sub>` AND `kittenscrew` (so we
/// match our own entry regardless of the absolute exe path, but not a user's
/// unrelated hook on the same event).
fn routes_to(entry: &Value, sub: &str) -> bool {
    let needle = format!("hook {sub}");
    entry
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|cmds| {
            cmds.iter().any(|c| {
                c.get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains(&needle) && s.contains("kittenscrew"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_round_trips_to_default() {
        // The template must always parse back to the default config — proof it
        // can't drift from the schema.
        let parsed = crate::config::parse(&config_template()).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn membrane_covers_every_v33_event() {
        // V33: the control plane intercepts ALL Claude Code events. If CC adds an
        // event we must extend MEMBRANE + its dispatch arm — this guards that.
        let events: Vec<&str> = MEMBRANE.iter().map(|(e, _, _)| *e).collect();
        for e in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "Stop",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
        ] {
            assert!(events.contains(&e), "membrane missing {e}");
        }
        assert_eq!(MEMBRANE.len(), 8, "exactly the 8 V33 events");
    }

    #[test]
    fn merge_registers_all_membrane_events_on_empty() {
        let mut s = json!({});
        let (reg, already) = merge_membrane(&mut s, "/bin/kittenscrew");
        assert_eq!(reg.len(), MEMBRANE.len());
        assert!(already.is_empty());
        // SessionStart entry carries its matcher + our command.
        let ss = &s["hooks"]["SessionStart"][0];
        assert_eq!(ss["matcher"], "startup|resume|clear|compact");
        assert_eq!(
            ss["hooks"][0]["command"],
            "/bin/kittenscrew hook session-start"
        );
        // PreToolUse has no matcher key (applies to all tools).
        assert!(s["hooks"]["PreToolUse"][0].get("matcher").is_none());
    }

    #[test]
    fn merge_is_idempotent() {
        let mut s = json!({});
        merge_membrane(&mut s, "/bin/kittenscrew");
        let (reg, already) = merge_membrane(&mut s, "/bin/kittenscrew");
        assert!(reg.is_empty());
        assert_eq!(already.len(), MEMBRANE.len());
        // No duplicate entries on any event.
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_preserves_unrelated_user_hooks() {
        let mut s = json!({
            "hooks": {
                "PreToolUse": [
                    { "hooks": [{ "type": "command", "command": "my-own-linter" }] }
                ]
            },
            "model": "opus"
        });
        let (reg, _) = merge_membrane(&mut s, "/bin/kittenscrew");
        assert!(reg.contains(&"PreToolUse".to_string()));
        let arr = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "user hook kept, ours appended");
        assert_eq!(arr[0]["hooks"][0]["command"], "my-own-linter");
        assert_eq!(s["model"], "opus", "untouched sibling keys survive");
    }
}
