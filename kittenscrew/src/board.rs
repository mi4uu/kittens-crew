//! The opinion board (blackboard) + council verdict — the governance layer.
//!
//! Kitties don't decide alone. They POST opinions to a shared, append-only board
//! (`.kittenscrew/board.jsonl`, NDJSON like `events.jsonl`), each opinion carrying
//! two orthogonal weights: `confidence` (how sure THIS cat is) and `competence`
//! (how well its DOMAIN fits the topic — a self-assessed authority weight). The
//! COUNCIL then tallies a verdict by `confidence × competence`, so a high-authority
//! minority can outvote a confident-but-off-topic majority. The Orchestrating Kitty
//! (🎩, the Big Boss) ratifies the winner — the board advises, the Boss has the
//! final word (mirrors `ratified_by` in the kitty role architecture).
//!
//! The persistence half is deliberately thin and crash-tolerant (a garbage line is
//! skipped, a missing file is an empty board — same spirit as `State::load`). The
//! tally half is PURE: `verdict()` is a total function of the opinions slice, so the
//! governance maths is unit-tested in isolation with no IO.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// On-disk board, append-only NDJSON. One `Opinion` JSON object per line.
const BOARD_PATH: &str = ".kittenscrew/board.jsonl";

/// One cat's stance on a topic, with the two weights the council multiplies.
///
/// `confidence` (0.0–1.0) = how sure this kitty is of its own stance.
/// `competence` (0.0–1.0) = how much this kitty's DOMAIN fits the topic — a
/// self-authority weight, so an off-domain cat's loud opinion counts for little.
/// `seq` is the monotonic append index (its line number on the board), assigned at
/// `post()` time so the board stays a totally-ordered log a TUI can replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opinion {
    pub kitty: String,
    pub topic: String,
    pub stance: String,
    pub confidence: f64,
    pub competence: f64,
    pub seq: u64,
}

/// Append `opinion` as one NDJSON line to the project board, creating
/// `.kittenscrew/` and the file on first write. If the caller left `seq == 0`,
/// it is stamped with the current line count so each post gets a fresh, monotonic
/// index without the caller having to track it.
pub fn post(opinion: &Opinion) -> std::io::Result<()> {
    post_to(Path::new(BOARD_PATH), opinion)
}

/// Read every opinion off the project board, skipping any unparseable line and
/// treating a missing file as an empty board — `load()` never errors, so a corrupt
/// or absent board degrades to "no opinions yet" rather than crashing a turn.
pub fn load() -> Vec<Opinion> {
    load_from(Path::new(BOARD_PATH))
}

/// Borrowed view of just the opinions on `topic`, in board (append) order.
pub fn for_topic<'a>(opinions: &'a [Opinion], topic: &str) -> Vec<&'a Opinion> {
    opinions.iter().filter(|o| o.topic == topic).collect()
}

/// The council's ruling on a topic: which stance won, by how much weight, the full
/// per-stance tally (highest first), and who ratified it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub topic: String,
    pub winner: String,
    pub weight: f64,
    pub tally: Vec<(String, f64)>,
    pub ratified_by: String,
}

/// Convene the council on `topic`. Each opinion contributes `confidence × competence`
/// to its stance; stances are summed, and the heaviest wins. Returns `None` when no
/// cat has weighed in on the topic. `ratified_by` is always `"orchestrating"` — the
/// board advises but the Big Boss 🎩 ratifies (final word). The tally is sorted by
/// weight descending, ties broken by stance name so the output is deterministic.
pub fn verdict(opinions: &[Opinion], topic: &str) -> Option<Verdict> {
    let here = for_topic(opinions, topic);
    if here.is_empty() {
        return None;
    }

    // Sum confidence×competence per stance. A Vec keeps it dependency-free and
    // deterministic (HashMap iteration order would not be), and the stance count
    // per topic is tiny.
    let mut tally: Vec<(String, f64)> = Vec::new();
    for o in &here {
        let w = o.confidence * o.competence;
        match tally.iter_mut().find(|(s, _)| *s == o.stance) {
            Some((_, acc)) => *acc += w,
            None => tally.push((o.stance.clone(), w)),
        }
    }

    // Heaviest stance first; ties broken by name so the winner is deterministic
    // even when two stances draw exactly equal weight.
    tally.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let (winner, weight) = tally[0].clone();
    Some(Verdict {
        topic: topic.to_string(),
        winner,
        weight,
        tally,
        ratified_by: "orchestrating".to_string(),
    })
}

// ── persistence internals (path-injectable so tests stay off the project board) ──

/// Append one opinion line to `path`, creating the parent dir + file as needed.
fn post_to(path: &Path, opinion: &Opinion) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    // Stamp an unset (0) seq with the current line count → fresh monotonic index.
    let mut record = opinion.clone();
    if record.seq == 0 {
        record.seq = load_from(path).len() as u64;
    }
    let line = serde_json::to_string(&record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")
}

/// Read every parseable opinion line from `path`; missing file → empty vec.
fn load_from(path: &Path) -> Vec<Opinion> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(kitty: &str, topic: &str, stance: &str, conf: f64, comp: f64) -> Opinion {
        Opinion {
            kitty: kitty.to_string(),
            topic: topic.to_string(),
            stance: stance.to_string(),
            confidence: conf,
            competence: comp,
            seq: 0,
        }
    }

    /// A throwaway board file under the OS temp dir, unique per process.
    fn temp_board(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ks_board_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("board.jsonl")
    }

    /// post → load round-trips every field, and seq is stamped monotonically.
    #[test]
    fn post_load_round_trips() {
        let path = temp_board("roundtrip");
        post_to(&path, &op("builder", "approach", "ship", 0.8, 0.9)).unwrap();
        post_to(&path, &op("grill", "approach", "wait", 0.6, 0.7)).unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].kitty, "builder");
        assert_eq!(loaded[0].stance, "ship");
        assert_eq!(loaded[0].seq, 0, "first post stamps seq 0");
        assert_eq!(loaded[1].kitty, "grill");
        assert_eq!(loaded[1].seq, 1, "second post stamps seq 1");
        // Floats survive the JSON round-trip exactly.
        assert_eq!(loaded[1].confidence, 0.6);
        assert_eq!(loaded[1].competence, 0.7);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A garbage line is skipped, not fatal; the good lines still load.
    #[test]
    fn load_skips_garbage() {
        let path = temp_board("garbage");
        post_to(&path, &op("memory", "design", "cache", 0.5, 0.5)).unwrap();
        // Splice in a junk line between two valid ones.
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(b"not json at all\n").unwrap();
        }
        post_to(&path, &op("style", "design", "inline", 0.5, 0.5)).unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 2, "two valid opinions survive the junk line");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A missing board file loads as an empty vec (never an error).
    #[test]
    fn load_missing_is_empty() {
        let path = std::env::temp_dir().join(format!("ks_board_absent_{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(load_from(&path).is_empty());
    }

    /// The plain-majority winner: most weight wins when weights are uniform.
    #[test]
    fn verdict_picks_heaviest_stance() {
        let ops = vec![
            op("a", "t", "yes", 0.9, 0.9),
            op("b", "t", "yes", 0.8, 0.9),
            op("c", "t", "no", 0.7, 0.7),
        ];
        let v = verdict(&ops, "t").unwrap();
        assert_eq!(v.winner, "yes");
        assert_eq!(v.ratified_by, "orchestrating", "Big Boss ratifies");
        // Tally is sorted heaviest-first.
        assert_eq!(v.tally[0].0, "yes");
        assert_eq!(v.tally[1].0, "no");
        assert!(v.tally[0].1 > v.tally[1].1);
    }

    /// The governance point: a single HIGH-competence cat beats a low-competence
    /// MAJORITY. Three off-domain cats (competence 0.2) lose to one on-domain cat
    /// (competence 1.0) even though they're the numerical majority.
    #[test]
    fn competence_lets_minority_win() {
        let ops = vec![
            op("crowd1", "arch", "rewrite", 0.9, 0.2),
            op("crowd2", "arch", "rewrite", 0.9, 0.2),
            op("crowd3", "arch", "rewrite", 0.9, 0.2),
            op("expert", "arch", "patch", 0.9, 1.0),
        ];
        // Majority weight: 3 × (0.9×0.2) = 0.54. Expert: 0.9×1.0 = 0.9 → patch wins.
        let v = verdict(&ops, "arch").unwrap();
        assert_eq!(
            v.winner, "patch",
            "high-competence minority must outweigh low-competence majority"
        );
    }

    /// No opinions on the topic → no verdict.
    #[test]
    fn verdict_empty_topic_is_none() {
        let ops = vec![op("a", "other", "x", 0.9, 0.9)];
        assert!(verdict(&ops, "missing").is_none());
        assert!(verdict(&[], "anything").is_none());
    }

    /// `for_topic` filters to just the requested topic, in board order.
    #[test]
    fn for_topic_filters() {
        let ops = vec![
            op("a", "t1", "x", 0.5, 0.5),
            op("b", "t2", "y", 0.5, 0.5),
            op("c", "t1", "z", 0.5, 0.5),
        ];
        let got = for_topic(&ops, "t1");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].kitty, "a");
        assert_eq!(got[1].kitty, "c");
    }
}
