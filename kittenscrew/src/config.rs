//! T15 — `kittenscrew.toml` per-project config. Optional; defaults if absent
//! (§C). Distinct from `.kittenscrew/spec.toml` (the plan store). Additive to
//! squeez's global INI, never overlapping it.
//!
//! Consumed by: `init` (T16, writes the template), `hook pre-tool` guard (T24,
//! `blocked_cmds`), worth knobs (T41, `[plan]`). This module only parses +
//! defaults — wiring lives in those tasks.

use serde::{Deserialize, Serialize};

/// Resolved per-project config. Every field defaults, so a missing file or a
/// partial table is always valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub kitty: KittyCfg,
    pub hooks: HooksCfg,
    pub docs: DocsCfg,
    pub plan: PlanCfg,
    pub guard: GuardCfg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KittyCfg {
    /// squeez compression persona to request (lite|full|ultra|adaptive).
    pub compression_level: String,
}
impl Default for KittyCfg {
    fn default() -> Self {
        KittyCfg {
            compression_level: "adaptive".into(),
        }
    }
}

/// Hook delegate commands. Empty = use the built-in `kittenscrew hook <event>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HooksCfg {
    pub pre: String,
    pub post: String,
    pub session: String,
    pub compact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocsCfg {
    /// `docs task` runs only when true (V12).
    pub auto_generate: bool,
    /// terse | normal | explain.
    pub detail: String,
    /// dev | idiot (audience).
    pub target: String,
}
impl Default for DocsCfg {
    fn default() -> Self {
        DocsCfg {
            auto_generate: false,
            detail: "normal".into(),
            target: "dev".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PlanCfg {
    /// Enforce a single linear order instead of the parallel READY frontier.
    pub strict_ordering: bool,
}

/// `pre-tool` guard: commands that must be blocked (T24, exit 2 on match).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GuardCfg {
    pub blocked_cmds: Vec<String>,
}

/// Default config path, honoring `KITTENSCREW_CONFIG`.
pub fn config_path() -> String {
    std::env::var("KITTENSCREW_CONFIG").unwrap_or_else(|_| "kittenscrew.toml".into())
}

/// Load config from `config_path()`. Absent file → defaults (§C). A present but
/// malformed file is a user error → `Err` (⊥ silently fall back, which would
/// hide a broken config).
pub fn load() -> Result<Config, String> {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => parse(&s).map_err(|e| format!("{path}: {e}")),
        Err(_) => Ok(Config::default()),
    }
}

/// Parse config from a TOML string (defaults fill any missing field).
pub fn parse(s: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_all_defaults() {
        let c = parse("").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.kitty.compression_level, "adaptive");
        assert!(!c.docs.auto_generate);
        assert_eq!(c.docs.detail, "normal");
        assert!(c.guard.blocked_cmds.is_empty());
    }

    #[test]
    fn partial_table_keeps_other_defaults() {
        let c = parse("[docs]\nauto_generate = true\n").unwrap();
        assert!(c.docs.auto_generate);
        assert_eq!(c.docs.detail, "normal"); // untouched → default
        assert_eq!(c.kitty.compression_level, "adaptive"); // whole table absent → default
    }

    #[test]
    fn full_config_parses() {
        let toml = r#"
            [kitty]
            compression_level = "ultra"
            [docs]
            auto_generate = true
            detail = "explain"
            target = "idiot"
            [plan]
            strict_ordering = true
            [guard]
            blocked_cmds = ["rm -rf", "git push --force"]
        "#;
        let c = parse(toml).unwrap();
        assert_eq!(c.kitty.compression_level, "ultra");
        assert_eq!(c.docs.detail, "explain");
        assert!(c.plan.strict_ordering);
        assert_eq!(c.guard.blocked_cmds.len(), 2);
    }

    #[test]
    fn malformed_is_error() {
        assert!(parse("[docs]\nauto_generate = not_a_bool\n").is_err());
    }
}
