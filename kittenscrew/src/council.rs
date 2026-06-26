//! P4 — the council view: one folded, ordered feed of everything the crew is doing.
//!
//! A live run scatters its state across two append-only NDJSON logs under
//! `.kittenscrew/`: the driver's liveness `events.jsonl` (T68 heartbeats — `status`
//! / `node` / `detail`) and the governance `board.jsonl` (opinions + the verdicts
//! the council tallies). A human watching a run does not want two tail -f windows;
//! they want ONE stream they can glance at, split into the three things that
//! actually matter:
//!
//!   - `Board`    — what the crew is *deciding* (opinions + council verdicts).
//!   - `ToMe`     — what the user is *watched for*: a node done / failed / blocked,
//!                  the run converged or halted. The "stop and look" line.
//!   - `Thoughts` — the rest: internal heartbeats / narration the user can ignore
//!                  but may want to expand into for context.
//!
//! This module is the PURE fold: it reads the two logs and classifies each record
//! into one of those streams, producing a list (`summary`) + preview (`detail`)
//! pair per line. The fzf wiring + the `Council` command live in `commands.rs`;
//! everything here is IO-light and unit-testable. Like `board::load`, a missing log
//! is simply an empty source — never a panic — so the council degrades to "nothing
//! yet" rather than crashing a glance.

use crate::board;
use crate::kitty;
use serde::Deserialize;
use std::path::Path;

/// On-disk feeds the council folds. Same `.kittenscrew/` home as the board.
const EVENTS_PATH: &str = ".kittenscrew/events.jsonl";
const BOARD_PATH: &str = ".kittenscrew/board.jsonl";

/// The three toggleable streams a watcher splits the feed into. See module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Opinions + council verdicts — what the crew is deciding.
    Board,
    /// Progress addressed to the user: done / failed / blocked / converged / halted.
    ToMe,
    /// Internal narration / heartbeats — context, not a call to action.
    Thoughts,
}

impl Stream {
    /// The stream's banner icon, prefixed on every compact line so the eye can
    /// triage by column even before reading the text.
    fn icon(self) -> &'static str {
        match self {
            Stream::Board => "🗳",   // ballot box — the council deciding
            Stream::ToMe => "📣",    // megaphone — addressed to you
            Stream::Thoughts => "💭", // thought bubble — internal narration
        }
    }

    /// Parse a `--stream` CLI value (`board|to-me|thoughts`). Lenient on case and
    /// the hyphen so `tome`/`to_me`/`ToMe` all resolve; unknown → `None`.
    pub fn parse(s: &str) -> Option<Stream> {
        match s.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
            "board" => Some(Stream::Board),
            "tome" => Some(Stream::ToMe),
            "thoughts" | "thought" => Some(Stream::Thoughts),
            _ => None,
        }
    }
}

/// One folded feed entry. `summary` is the compact list row (stream icon + kitty
/// emoji + short text); `detail` is the fuller text shown in a preview pane. `kitty`
/// carries the speaker id when known, so the row can be tinted by `kitty::color`.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub stream: Stream,
    pub icon: String,
    pub kitty: Option<String>,
    pub summary: String,
    pub detail: String,
}

/// A raw driver heartbeat as it lands in `events.jsonl`. Mirrors
/// `driver::status::Heartbeat` but is decoded here independently so the council
/// never depends on the writer's internals — only the on-wire shape (`status` is
/// the lowercase liveness string, `node` the plan node, `detail` free text). All
/// fields default so a sparse / older line still parses rather than being dropped.
#[derive(Debug, Deserialize, Default)]
struct RawEvent {
    #[serde(default)]
    status: String,
    #[serde(default)]
    node: String,
    #[serde(default)]
    detail: String,
}

/// Build the folded council feed from the live project logs. Missing log → that
/// source is simply absent (never an error), mirroring `board::load`'s tolerance.
pub fn lines() -> Vec<Line> {
    lines_from(Path::new(EVENTS_PATH), Path::new(BOARD_PATH))
}

/// Path-injectable core of [`lines`] so tests can fold throwaway fixtures instead
/// of the project logs (same `post_to`/`load_from` split `board.rs` uses).
///
/// Board records come first (the standing decisions), then the driver heartbeats
/// in append order — the board has no shared clock with the event log, so we keep
/// each source internally ordered and concatenate rather than invent a merge key.
fn lines_from(events_path: &Path, board_path: &Path) -> Vec<Line> {
    let mut out = Vec::new();

    // ── Board: opinions become Board-stream lines, voiced by their kitty. ──
    let opinions = board::load_path(board_path);
    for o in &opinions {
        let emoji = kitty::lookup(&o.kitty).map(|k| k.emoji).unwrap_or("🐾");
        let summary = format!(
            "{} {emoji} {}: {} ({:.2}×{:.2})",
            Stream::Board.icon(),
            o.topic,
            o.stance,
            o.confidence,
            o.competence,
        );
        let detail = format!(
            "[board] {} on '{}': {}  (confidence {:.2}, competence {:.2}, seq {})",
            o.kitty, o.topic, o.stance, o.confidence, o.competence, o.seq,
        );
        out.push(Line {
            stream: Stream::Board,
            icon: Stream::Board.icon().to_string(),
            kitty: Some(o.kitty.clone()),
            summary,
            detail,
        });
    }

    // ── Events: each heartbeat is classified ToMe vs Thoughts. ──
    let raw = std::fs::read_to_string(events_path).unwrap_or_default();
    for ev in raw
        .lines()
        .filter_map(|l| serde_json::from_str::<RawEvent>(l).ok())
    {
        let stream = classify_event(&ev);
        // Heartbeats aren't voiced by a named cat; the Helper 🐾 narrates progress
        // and the Orchestrator 🎩 owns run-level verdicts (converged/halted).
        let emoji = if is_run_level(&ev) { "🎩" } else { "🐾" };
        let node = if ev.node.is_empty() { "—" } else { &ev.node };
        let summary = format!(
            "{} {emoji} {} {}{}",
            stream.icon(),
            node,
            ev.status,
            if ev.detail.is_empty() {
                String::new()
            } else {
                format!(" — {}", ev.detail)
            },
        );
        let detail = format!(
            "[event] status={} node={} detail={}",
            ev.status,
            if ev.node.is_empty() { "—" } else { &ev.node },
            if ev.detail.is_empty() { "—" } else { &ev.detail },
        );
        out.push(Line {
            stream,
            icon: stream.icon().to_string(),
            kitty: None,
            summary,
            detail,
        });
    }

    out
}

/// Heuristic split of a heartbeat into ToMe vs Thoughts.
///
/// ToMe = the things a human stops and looks at: a node that finished, failed, or
/// got blocked, and the run-level verdicts (converged / halted / stopped). We test
/// the structured `status` first (the reliable signal), then fall back to substring
/// sniffing on the free-text `detail` for the `run` convention where a blocked node
/// arrives as a `✗ …` model slot.
///
/// NOTE: this is intentionally a loose substring heuristic — the heartbeat schema is
/// still just `status`/`node`/`detail`, so "what the user watches" can only be
/// inferred from text. Tighten this the moment the event record grows a real
/// `audience`/`kind` field; the classification is isolated here precisely so that
/// future change is a one-function edit.
fn classify_event(ev: &RawEvent) -> Stream {
    // Structured liveness that a user watches for: a block or a stop is a "look" line;
    // a plain `done` on a node is the progress they're waiting on. `running`/
    // `starting` are mid-flight churn → Thoughts.
    let status_to_me = matches!(ev.status.as_str(), "done" | "blocked" | "stopped");

    let hay = format!("{} {}", ev.status, ev.detail).to_ascii_lowercase();
    let detail_to_me = ["✗", "fail", "halt", "converged", "block", "error", "stop"]
        .iter()
        .any(|kw| hay.contains(kw));

    if status_to_me || detail_to_me {
        Stream::ToMe
    } else {
        Stream::Thoughts
    }
}

/// Run-level (vs node-level) heartbeats — these are the Orchestrator's 🎩 verdicts
/// on the whole run, so they get the Big Boss emoji rather than the Helper's 🐾.
/// Detected by an empty node slot or a converged/halted detail.
fn is_run_level(ev: &RawEvent) -> bool {
    let hay = ev.detail.to_ascii_lowercase();
    ev.node.is_empty() || hay.contains("converged") || hay.contains("halt")
}

/// Render the feed as plain coloured text, one line per item, optionally filtered to
/// a single stream. This is the no-fzf fallback dashboard (and the source the fzf
/// path pipes in). Each row is tinted by its speaker's `kitty::color`; event rows
/// with no named cat fall back to the Helper 🐾 colour for ToMe and a dim default
/// for Thoughts, so the three streams stay visually distinct even unfiltered.
pub fn render_compact(lines: &[Line], filter: Option<Stream>) -> String {
    let mut out = String::new();
    for l in lines {
        if let Some(f) = filter {
            if l.stream != f {
                continue;
            }
        }
        let color = match &l.kitty {
            Some(id) => kitty::color(id),
            None => match l.stream {
                Stream::ToMe => kitty::color("helper"),
                _ => "90", // bright-black / dim — background narration
            },
        };
        out.push_str(&format!("\x1b[{color}m{}\x1b[0m\n", l.summary));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A throwaway `.kittenscrew`-shaped dir under the OS temp root, unique per
    /// process + tag (mirrors the temp-dir style in `board.rs` / `driver`).
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ks_council_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write `contents` to `dir/name` and return the path.
    fn write_file(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        p
    }

    /// A board opinion lands in the Board stream; a `✗ failed` / `converged` event
    /// lands in ToMe; a plain `running` heartbeat is Thoughts.
    #[test]
    fn classifies_into_three_streams() {
        let dir = temp_dir("classify");
        let board = write_file(
            &dir,
            "board.jsonl",
            // One real opinion line (Opinion's serde shape).
            "{\"kitty\":\"builder\",\"topic\":\"approach\",\"stance\":\"ship\",\"confidence\":0.8,\"competence\":0.9,\"seq\":0}\n",
        );
        let events = write_file(
            &dir,
            "events.jsonl",
            // running → Thoughts; blocked ✗ → ToMe; converged run-level → ToMe.
            "{\"status\":\"running\",\"node\":\"T1\",\"detail\":\"dispatch\"}\n\
             {\"status\":\"blocked\",\"node\":\"T2\",\"detail\":\"✗ failed to compile\"}\n\
             {\"status\":\"done\",\"node\":\"\",\"detail\":\"converged — 3 green\"}\n",
        );

        let lines = lines_from(&events, &board);
        assert_eq!(lines.len(), 4, "1 opinion + 3 events");

        // The opinion is a Board line voiced by builder.
        assert_eq!(lines[0].stream, Stream::Board);
        assert_eq!(lines[0].kitty.as_deref(), Some("builder"));

        // running heartbeat → Thoughts.
        assert_eq!(lines[1].stream, Stream::Thoughts);
        // blocked + ✗ failed → ToMe.
        assert_eq!(lines[2].stream, Stream::ToMe);
        // converged run-level → ToMe.
        assert_eq!(lines[3].stream, Stream::ToMe);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `render_compact` with a filter shows ONLY that stream's lines.
    #[test]
    fn render_compact_filter_isolates_one_stream() {
        let dir = temp_dir("filter");
        let board = write_file(
            &dir,
            "board.jsonl",
            "{\"kitty\":\"grill\",\"topic\":\"risk\",\"stance\":\"wait\",\"confidence\":0.6,\"competence\":0.7,\"seq\":0}\n",
        );
        let events = write_file(
            &dir,
            "events.jsonl",
            "{\"status\":\"running\",\"node\":\"T1\",\"detail\":\"work\"}\n\
             {\"status\":\"blocked\",\"node\":\"T2\",\"detail\":\"halt\"}\n",
        );
        let lines = lines_from(&events, &board);

        // Board filter → only the grill opinion row.
        let board_only = render_compact(&lines, Some(Stream::Board));
        assert_eq!(board_only.lines().count(), 1);
        assert!(board_only.contains("risk"));
        assert!(!board_only.contains("halt"));

        // ToMe filter → only the blocked/halt event.
        let tome_only = render_compact(&lines, Some(Stream::ToMe));
        assert_eq!(tome_only.lines().count(), 1);
        assert!(tome_only.contains("halt"));

        // Thoughts filter → only the running heartbeat.
        let thoughts_only = render_compact(&lines, Some(Stream::Thoughts));
        assert_eq!(thoughts_only.lines().count(), 1);

        // No filter → all three.
        assert_eq!(render_compact(&lines, None).lines().count(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Missing / empty logs fold to an empty feed — never a panic.
    #[test]
    fn empty_or_missing_sources_yield_empty() {
        // Both paths absent.
        let absent = std::env::temp_dir().join(format!("ks_council_absent_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&absent);
        assert!(lines_from(&absent.join("events.jsonl"), &absent.join("board.jsonl")).is_empty());

        // Both present but empty.
        let dir = temp_dir("empty");
        let board = write_file(&dir, "board.jsonl", "");
        let events = write_file(&dir, "events.jsonl", "");
        assert!(lines_from(&events, &board).is_empty());

        // render_compact on an empty feed is the empty string.
        assert_eq!(render_compact(&[], None), "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `Stream::parse` accepts the CLI spellings and rejects garbage.
    #[test]
    fn stream_parse_is_lenient() {
        assert_eq!(Stream::parse("board"), Some(Stream::Board));
        assert_eq!(Stream::parse("to-me"), Some(Stream::ToMe));
        assert_eq!(Stream::parse("ToMe"), Some(Stream::ToMe));
        assert_eq!(Stream::parse("thoughts"), Some(Stream::Thoughts));
        assert_eq!(Stream::parse("nonsense"), None);
    }
}
