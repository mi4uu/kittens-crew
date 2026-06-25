//! T68 — driver liveness / status reporter.
//!
//! The driver self-reports its lifecycle — `Starting`, `Running`, `Blocked`,
//! `Done`, `Stopped` — as newline-delimited JSON heartbeats written to any
//! `impl std::io::Write`. No sockets, no threads, no async: one `report()`
//! call = one JSON line. The real unix-socket sink is a thin wrapper here:
//!
//! ```text
//! // ponytail: UnixStream::connect(SOCK_PATH).ok().map(Reporter::new)
//! ```
//!
//! Tests use a `Vec<u8>` so there is zero process-level IO in the test suite.

use serde::{Deserialize, Serialize};

/// Lifecycle signal emitted by the driver for any attached supervisor.
///
/// Serializes lowercase (`starting`, `running`, …) to keep the wire format
/// stable and human-readable in logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Liveness {
    /// Process has started, not yet entered the drive loop.
    Starting,
    /// Drive loop is actively executing a node.
    Running,
    /// Node is blocked (unmet deps or failed verify); loop is waiting.
    Blocked,
    /// All nodes converged — plan complete.
    Done,
    /// Driver was explicitly stopped (cap reached, user halt, error).
    Stopped,
}

/// A single liveness heartbeat. Serialized to one JSON line per `report()`.
///
/// `node` identifies the current plan node (empty string when not applicable).
/// `detail` carries a short human-readable explanation (reason for block, model
/// used, error summary, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    /// Current driver liveness.
    pub status: Liveness,
    /// Plan node in scope, or `""` when between nodes.
    pub node: String,
    /// Free-text detail — reason, model, short error. May be empty.
    pub detail: String,
}

/// Writes [`Heartbeat`] values as newline-delimited JSON to any sink.
///
/// The real deployment sink is a unix socket (or a pipe to a supervisor);
/// unit tests use `Vec<u8>`. Constructing a `Reporter` is infallible — only
/// individual `report()` calls propagate IO errors.
///
/// ```rust
/// use kittenscrew::driver::status::{Heartbeat, Liveness, Reporter};
///
/// let mut out: Vec<u8> = Vec::new();
/// let mut r = Reporter::new(&mut out);
/// r.report(&Heartbeat { status: Liveness::Running, node: "T68".into(), detail: "ok".into() }).unwrap();
/// assert!(out.starts_with(b"{"));
/// ```
pub struct Reporter<W: std::io::Write> {
    writer: W,
}

impl<W: std::io::Write> Reporter<W> {
    /// Wrap `w`; all `report()` calls write to it.
    pub fn new(w: W) -> Self {
        Self { writer: w }
    }

    /// Serialize `hb` as a single JSON object followed by `'\n'` and flush.
    /// Returns the first IO error encountered, if any.
    pub fn report(&mut self, hb: &Heartbeat) -> std::io::Result<()> {
        // serde_json::to_string never fails for this type (no maps with
        // non-string keys, no unencodable floats).
        let line = serde_json::to_string(hb)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hb(status: Liveness, node: &str, detail: &str) -> Heartbeat {
        Heartbeat {
            status,
            node: node.into(),
            detail: detail.into(),
        }
    }

    /// One `report()` call → exactly one JSON line in the sink.
    #[test]
    fn single_report_produces_one_line() {
        let mut buf: Vec<u8> = Vec::new();
        let mut r = Reporter::new(&mut buf);
        r.report(&hb(Liveness::Running, "T68", "nominal")).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1, "expected exactly one line, got: {text:?}");
    }

    /// The written JSON round-trips back to an identical `Heartbeat`.
    #[test]
    fn json_round_trips() {
        let original = hb(Liveness::Blocked, "T42", "dep T41 not done");
        let mut buf: Vec<u8> = Vec::new();
        Reporter::new(&mut buf).report(&original).unwrap();
        let parsed: Heartbeat = serde_json::from_slice(&buf).unwrap();
        assert_eq!(parsed, original);
    }

    /// Multiple `report()` calls produce the same number of newline-terminated lines.
    #[test]
    fn multiple_reports_produce_multiple_lines() {
        let statuses = [
            hb(Liveness::Starting, "", "boot"),
            hb(Liveness::Running, "T1", "dispatch"),
            hb(Liveness::Done, "", "converged"),
        ];
        let mut buf: Vec<u8> = Vec::new();
        let mut r = Reporter::new(&mut buf);
        for s in &statuses {
            r.report(s).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), statuses.len());
        // Each line must independently parse.
        for (line, expected) in lines.iter().zip(statuses.iter()) {
            let parsed: Heartbeat = serde_json::from_str(line).unwrap();
            assert_eq!(&parsed, expected);
        }
    }

    /// Status variants serialize to lowercase strings in JSON.
    #[test]
    fn status_serializes_lowercase() {
        let cases = [
            (Liveness::Starting, "starting"),
            (Liveness::Running, "running"),
            (Liveness::Blocked, "blocked"),
            (Liveness::Done, "done"),
            (Liveness::Stopped, "stopped"),
        ];
        for (variant, expected) in cases {
            let hb = hb(variant, "", "");
            let json = serde_json::to_string(&hb).unwrap();
            assert!(
                json.contains(&format!("\"status\":\"{expected}\"")),
                "expected status={expected:?} in {json}"
            );
        }
    }
}
