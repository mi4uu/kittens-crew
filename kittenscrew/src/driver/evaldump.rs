//! T83 — transcript → evaluation capture.
//!
//! When an agent is terminated (deal-breaker) or stopped as a zombie, capture
//! its action transcript and serialize it for OFFLINE evaluation. A downstream
//! evaluator reads the JSON-lines to extract lessons that feed rule/prompt
//! improvements. Pure, deterministic — no model, no network, no real FS in
//! the core API.
//!
//! # Wire format
//! `dump` writes **one JSON object + newline** per call (JSON-L). The real sink
//! is an append-only eval-queue file; opening / rotating that file is a thin
//! caller concern.
//!
// ponytail: real sink is an append-only file, e.g.
//   let f = std::fs::OpenOptions::new().create(true).append(true)
//              .open(".kittenscrew/eval-queue.jsonl")?;
//   evaldump::dump(&transcript, f)?;
// Rotation / drain belongs to a future eval-queue worker, not this module.

use serde::{Deserialize, Serialize};

/// One action the agent took and the result it observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRecord {
    /// Human-readable description of the action attempted.
    pub action: String,
    /// Observed outcome / tool response (truncated by the caller if very long).
    pub result: String,
    /// `true` = action succeeded; `false` = failed / produced an error.
    pub ok: bool,
}

/// Full transcript of an agent run that ended abnormally (deal-breaker kill
/// or zombie stop). Serializes to a single JSON object in `dump`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcript {
    /// Stable agent identifier (e.g. task id such as `"T83"`).
    pub agent_id: String,
    /// Why the agent was terminated: `"dealbreaker"` / `"zombie"` / custom.
    pub reason: String,
    /// Ordered action log: first pushed → first in `records`.
    pub records: Vec<ActionRecord>,
}

impl Transcript {
    /// Create an empty transcript for `agent_id` terminated with `reason`.
    pub fn new(agent_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Transcript {
            agent_id: agent_id.into(),
            reason: reason.into(),
            records: Vec::new(),
        }
    }

    /// Append one (action, result, ok) triple in arrival order.
    pub fn push(&mut self, action: impl Into<String>, result: impl Into<String>, ok: bool) {
        self.records.push(ActionRecord {
            action: action.into(),
            result: result.into(),
            ok,
        });
    }
}

/// Serialize `t` as one JSON object followed by a newline to `w`.
///
/// Designed for append-only JSON-L sinks: each call writes exactly one line.
/// Use a `Vec<u8>` in tests; wrap a `File` (opened with `.append(true)`) in
/// production — see the `// ponytail:` comment at the top of this module.
pub fn dump<W: std::io::Write>(t: &Transcript, mut w: W) -> std::io::Result<()> {
    let json = serde_json::to_string(t)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    w.write_all(json.as_bytes())?;
    w.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three_record_transcript() -> Transcript {
        let mut t = Transcript::new("T83", "dealbreaker");
        t.push("read file foo.rs", "ok: 42 lines", true);
        t.push("write file foo.rs", "error: permission denied", false);
        t.push("report failure", "logged", true);
        t
    }

    /// `dump` produces exactly one line ending in '\n'.
    #[test]
    fn dump_is_one_json_line() {
        let t = three_record_transcript();
        let mut buf = Vec::new();
        dump(&t, &mut buf).unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.ends_with('\n'), "must end with newline");
        assert_eq!(text.lines().count(), 1, "must be exactly one line");
    }

    /// Parsed output equals the original (round-trip identity).
    #[test]
    fn dump_round_trips() {
        let original = three_record_transcript();
        let mut buf = Vec::new();
        dump(&original, &mut buf).unwrap();

        let line = String::from_utf8(buf).unwrap();
        let parsed: Transcript = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(original, parsed);
    }

    /// Records are preserved in push order and ok flags are exact.
    #[test]
    fn records_order_and_ok_flags_preserved() {
        let t = three_record_transcript();
        assert_eq!(t.records.len(), 3);

        assert_eq!(t.records[0].action, "read file foo.rs");
        assert!(t.records[0].ok);

        assert_eq!(t.records[1].action, "write file foo.rs");
        assert!(!t.records[1].ok);

        assert_eq!(t.records[2].action, "report failure");
        assert!(t.records[2].ok);
    }

    /// Empty transcript still serializes / round-trips cleanly.
    #[test]
    fn empty_transcript_round_trips() {
        let t = Transcript::new("T00", "zombie");
        let mut buf = Vec::new();
        dump(&t, &mut buf).unwrap();
        let parsed: Transcript = serde_json::from_str(String::from_utf8(buf).unwrap().trim()).unwrap();
        assert_eq!(t, parsed);
        assert!(parsed.records.is_empty());
    }
}
