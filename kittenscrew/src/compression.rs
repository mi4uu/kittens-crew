//! T49 — compression POLICY (V32, V10). kittenscrew owns *which* squeez level
//! applies to *which* content-class; squeez does the actual compressing. This
//! module is pure policy: a `Class → level` lookup over the configurable
//! `[compression]` table. No content is compressed here (V10: that work is
//! delegated to the squeez binary) — consumers (hooks, the T50 harness) query
//! `level_for` and pass the result to squeez.

use crate::config::CompressionCfg;

/// Content classes (V32). `Structured`/`Diff` are the lossless floor — savings
/// are small and a fidelity slip forces a re-run (net negative). `Prose`/`Dump`
/// take aggressive compression — savings are high and loss is ≈ 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Prose,
    Dump,
    Structured,
    Diff,
}

impl Class {
    pub const ALL: [Class; 4] = [Class::Prose, Class::Dump, Class::Structured, Class::Diff];

    /// Parse a class name (case-insensitive). Unknown → `None` (caller exits 2).
    pub fn parse(s: &str) -> Option<Class> {
        match s.trim().to_ascii_lowercase().as_str() {
            "prose" => Some(Class::Prose),
            "dump" => Some(Class::Dump),
            "structured" => Some(Class::Structured),
            "diff" => Some(Class::Diff),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Prose => "prose",
            Class::Dump => "dump",
            Class::Structured => "structured",
            Class::Diff => "diff",
        }
    }
}

/// The squeez level the policy assigns to `class`. Borrows from the config —
/// the table is the single source of truth (tune it, don't fork this).
pub fn level_for(cfg: &CompressionCfg, class: Class) -> &str {
    match class {
        Class::Prose => &cfg.prose,
        Class::Dump => &cfg.dump,
        Class::Structured => &cfg.structured,
        Class::Diff => &cfg.diff,
    }
}

/// The whole policy as `(class, level)` rows, in a stable order — for
/// `compression policy` and the harness corpus report.
pub fn policy(cfg: &CompressionCfg) -> Vec<(&'static str, &str)> {
    Class::ALL
        .iter()
        .map(|&c| (c.as_str(), level_for(cfg, c)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_v32_floor() {
        let cfg = CompressionCfg::default();
        // Aggressive where loss is harmless...
        assert_eq!(level_for(&cfg, Class::Prose), "full");
        assert_eq!(level_for(&cfg, Class::Dump), "ultra");
        // ...lossless floor where a slip forces a re-run.
        assert_eq!(level_for(&cfg, Class::Structured), "off");
        assert_eq!(level_for(&cfg, Class::Diff), "off");
    }

    #[test]
    fn parse_is_case_insensitive_and_total_over_known() {
        for &c in &Class::ALL {
            assert_eq!(Class::parse(&c.as_str().to_uppercase()), Some(c));
            assert_eq!(Class::parse(c.as_str()), Some(c));
        }
        assert_eq!(Class::parse("binary"), None);
    }

    #[test]
    fn policy_lists_every_class_once() {
        let cfg = CompressionCfg::default();
        let rows = policy(&cfg);
        assert_eq!(rows.len(), Class::ALL.len());
        assert_eq!(rows[0], ("prose", "full"));
    }
}
