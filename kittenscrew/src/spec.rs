//! T27/T33/T34 — SPEC.md ↔ store bridge.
//!
//! - `import`: parse SPEC.md (FORMAT.md pipe-tables + prose) → Store. Bridges
//!   old 4-col §T (`id|status|task|cites`) and new 5-col (`…|deps|cites`).
//! - `render`: Store → SPEC.md projection. `priority`/`scope` stay toml-only.
//! - `validate`: structural — deps/cites resolve, ids unique, no cycle (V14).

use crate::store::{Bug, Invariant, Status, Store, Task};
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("§T row {0}: bad status symbol")]
    BadStatus(String),
}

// ---- import ---------------------------------------------------------------

/// Parse SPEC.md text into a Store.
pub fn import(md: &str) -> Result<Store, SpecError> {
    let mut store = Store::default();
    let mut cur: Option<char> = None;
    let mut buf: Vec<&str> = Vec::new();
    let mut sections: Vec<(char, Vec<&str>)> = Vec::new();

    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("## §") {
            if let Some(c) = cur.take() {
                sections.push((c, std::mem::take(&mut buf)));
            }
            cur = rest.chars().next();
        } else if cur.is_some() {
            buf.push(line);
        }
    }
    if let Some(c) = cur.take() {
        sections.push((c, buf));
    }

    // Accumulate, never overwrite: real specs scatter a section across
    // continuation blocks (`## §V (short-side, cont.)`) interleaved with
    // non-standard sections (`## §U …`, ignored).
    for (letter, lines) in sections {
        match letter {
            'G' => {
                if !store.goal.is_empty() {
                    store.goal.push('\n');
                }
                store.goal.push_str(&join_prose(&lines));
            }
            'C' => store.constraints.extend(bullets(&lines)),
            'I' => store.interfaces.extend(bullets(&lines)),
            'V' => store.invariants.extend(parse_invariants(&lines)),
            'T' => store.tasks.extend(parse_tasks(&lines)?),
            'B' => store.bugs.extend(parse_bugs(&lines)),
            _ => {}
        }
    }
    Ok(store)
}

fn join_prose(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn bullets(lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.trim())
        .filter_map(|l| l.strip_prefix("- "))
        .map(|s| s.trim().to_string())
        .collect()
}

fn parse_invariants(lines: &[&str]) -> Vec<Invariant> {
    let mut out = Vec::new();
    for l in lines {
        let l = l.trim();
        let l = l.strip_prefix("- ").map(str::trim).unwrap_or(l);
        if let Some((id, text)) = split_invariant(l) {
            out.push(Invariant { id, text });
        }
    }
    out
}

/// Match a leading `V<digits>` id followed by a separator (`:` `|` `.` space).
/// Tolerates the three real-world styles: `V1: …`, `V1 | …`, `- V1 slug: …`.
fn split_invariant(l: &str) -> Option<(String, String)> {
    if !l.starts_with('V') {
        return None;
    }
    let after_v = &l[1..];
    let digit_len = after_v.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_len == 0 {
        return None; // "Verify…", not an id
    }
    let id_end = 1 + digit_len;
    let id = l[..id_end].to_string();
    let rest = l[id_end..]
        .trim_start_matches([':', '|', '.', ' '])
        .trim()
        .to_string();
    Some((id, rest))
}

fn parse_tasks(lines: &[&str]) -> Result<Vec<Task>, SpecError> {
    let mut out = Vec::new();
    // Layout learned from the header row. 5-col = header has a `deps` column.
    // Real specs vary: aligned headers (`id | st | …`), code-fenced tables,
    // `desc` instead of `task`. Anchor id+status from the front and cites
    // (+deps) from the back so unescaped `|`/`||` in task prose survives.
    let mut has_deps = false;
    for l in lines {
        let l = l.trim();
        if l.is_empty() || !l.contains('|') {
            continue; // blank / code-fence / prose
        }
        let first = l.split('|').next().unwrap_or("").trim();
        if first.eq_ignore_ascii_case("id") {
            has_deps = l.split('|').any(|c| c.trim().eq_ignore_ascii_case("deps"));
            continue; // header
        }

        let mut front = l.splitn(3, '|');
        let id = unescape(front.next().unwrap_or("").trim());
        let status_cell = front.next().unwrap_or("").trim();
        let rest = front.next().unwrap_or(""); // task [| deps] | cites

        let sym = status_cell.chars().next().unwrap_or('?');
        let status = Status::from_symbol(sym).ok_or_else(|| SpecError::BadStatus(id.clone()))?;

        let (task, deps_cell, cites_cell) = split_tail(rest, has_deps);
        let (cites, note) = parse_cites_cell(&cites_cell);
        out.push(Task {
            id,
            status,
            task: unescape(&task),
            deps: split_list(&deps_cell),
            priority: 0,
            cites,
            scope: Vec::new(),
            note,
            ..Default::default()
        });
    }
    Ok(out)
}

/// Carve `task [| deps] | cites` off the right, leaving any in-prose `|` in task.
fn split_tail(rest: &str, has_deps: bool) -> (String, String, String) {
    if has_deps {
        let parts: Vec<&str> = rest.rsplitn(3, '|').collect(); // [cites, deps, task]
        match parts.as_slice() {
            [cites, deps, task] => (task.trim().into(), deps.trim().into(), cites.trim().into()),
            [cites, task] => (task.trim().into(), String::new(), cites.trim().into()),
            _ => (rest.trim().into(), String::new(), String::new()),
        }
    } else {
        match rest.rsplit_once('|') {
            Some((task, cites)) => (task.trim().into(), String::new(), cites.trim().into()),
            None => (rest.trim().into(), String::new(), String::new()),
        }
    }
}

fn unescape(s: &str) -> String {
    s.replace("\\|", "|")
}

fn parse_bugs(lines: &[&str]) -> Vec<Bug> {
    let mut out = Vec::new();
    for l in lines {
        let l = l.trim();
        if l.is_empty() || l.starts_with("id|") || !l.contains('|') {
            continue;
        }
        let c = split_row(l);
        if c.len() >= 4 {
            out.push(Bug {
                id: c[0].clone(),
                date: c[1].clone(),
                cause: c[2].clone(),
                fix: c[3].clone(),
            });
        }
    }
    out
}

/// Split a pipe-table row into trimmed cells, honoring `\|` escape.
fn split_row(row: &str) -> Vec<String> {
    row.replace("\\|", "\u{0}")
        .split('|')
        .map(|c| c.replace('\u{0}', "|").trim().to_string())
        .collect()
}

/// Comma-separated cell → list, dropping `-`/empty.
fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty() && *x != "-")
        .map(String::from)
        .collect()
}

/// §T cites cell: refs + optional trailing `(note)` (ladder-kill reason).
fn parse_cites_cell(cell: &str) -> (Vec<String>, String) {
    match cell.split_once('(') {
        Some((before, rest)) => {
            let note = rest.trim_end().trim_end_matches(')').trim().to_string();
            (split_list(before), note)
        }
        None => (split_list(cell), String::new()),
    }
}

// ---- render ---------------------------------------------------------------

/// Store → SPEC.md projection (FORMAT.md). `priority`/`scope` are toml-only,
/// not emitted. §T uses 5-col `id|status|task|deps|cites`.
pub fn render(s: &Store) -> String {
    let mut o = String::from("# SPEC\n\n## §G GOAL\n\n");
    o.push_str(&s.goal);
    o.push_str("\n\n## §C CONSTRAINTS\n\n");
    for c in &s.constraints {
        o.push_str("- ");
        o.push_str(c);
        o.push('\n');
    }
    o.push_str("\n## §I INTERFACES\n\n");
    for i in &s.interfaces {
        o.push_str("- ");
        o.push_str(i);
        o.push('\n');
    }
    o.push_str("\n## §V INVARIANTS\n\n");
    for v in &s.invariants {
        o.push_str(&v.id);
        o.push_str(": ");
        o.push_str(&v.text);
        o.push('\n');
    }
    o.push_str("\n## §T TASKS\n\nid|status|task|deps|cites\n");
    for t in &s.tasks {
        o.push_str(&format!(
            "{}|{}|{}|{}|{}\n",
            t.id,
            t.status.symbol(),
            escape(&t.task),
            render_list(&t.deps),
            render_cites(&t.cites, &t.note),
        ));
    }
    o.push_str("\n## §B BUGS\n\nid|date|cause|fix\n");
    for b in &s.bugs {
        o.push_str(&format!(
            "{}|{}|{}|{}\n",
            b.id,
            escape(&b.date),
            escape(&b.cause),
            escape(&b.fix)
        ));
    }
    o
}

fn escape(s: &str) -> String {
    s.replace('|', "\\|")
}

fn render_list(v: &[String]) -> String {
    if v.is_empty() {
        "-".to_string()
    } else {
        v.join(",")
    }
}

fn render_cites(cites: &[String], note: &str) -> String {
    let base = render_list(cites);
    if note.is_empty() {
        base
    } else {
        format!("{base}   ({note})")
    }
}

/// Symbol → English, FORMAT.md legend. ONLY unambiguous unicode symbols —
/// ASCII overloads (`!`=must, `?`=may, `&`=and, `|`=or) are skipped: they
/// collide with prose/code/table delimiters, so expanding them deterministically
/// would corrupt text. `expand` is lossless for the symbols it touches.
const LEGEND: &[(&str, &str)] = &[
    ("→", "leads to"),
    ("∴", "therefore"),
    ("∀", "for all"),
    ("∃", "exists"),
    ("⊥", "never"),
    ("∅", "killed"),
    ("≠", "not equal"),
    ("∉", "not in"),
    ("∈", "in"),
    ("≤", "at most"),
    ("≥", "at least"),
];

/// Expand caveman symbols to English for human/agent reading (`spec read --plain`).
/// Structure-safe: leaves `|` table delimiters and ASCII overloads untouched.
pub fn expand(text: &str) -> String {
    let mut s = text.to_string();
    for (sym, word) in LEGEND {
        s = s.replace(sym, word);
    }
    s
}

/// Extract a single `## §X …` block from rendered SPEC.md (for `spec read §X`).
pub fn section(md: &str, letter: char) -> Option<String> {
    let mut out: Option<String> = None;
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("## §") {
            if rest.starts_with(letter) {
                out = Some(format!("{line}\n"));
            } else if out.is_some() {
                break;
            }
        } else if let Some(acc) = out.as_mut() {
            acc.push_str(line);
            acc.push('\n');
        }
    }
    out
}

// ---- validate -------------------------------------------------------------

/// Structural validation. Empty Vec = clean. Each entry = one caveman violation.
pub fn validate(s: &Store) -> Vec<String> {
    let mut v = Vec::new();

    let mut seen = HashSet::new();
    for t in &s.tasks {
        if !seen.insert(t.id.as_str()) {
            v.push(format!("dup task id {}", t.id));
        }
    }
    let ids: HashSet<&str> = s.tasks.iter().map(|t| t.id.as_str()).collect();
    let inv: HashSet<&str> = s.invariants.iter().map(|i| i.id.as_str()).collect();

    for t in &s.tasks {
        for d in &t.deps {
            if !ids.contains(d.as_str()) {
                v.push(format!("{} dep → missing task {}", t.id, d));
            }
        }
        for c in &t.cites {
            if is_invariant_ref(c) && !inv.contains(c.as_str()) {
                v.push(format!("{} cites → missing {}", t.id, c));
            }
        }
    }

    if let Some(cycle) = find_cycle(s) {
        v.push(format!("cycle: {}", cycle.join(" → ")));
    }
    v
}

fn is_invariant_ref(c: &str) -> bool {
    c.starts_with('V') && c.len() > 1 && c[1..].chars().all(|x| x.is_ascii_digit())
}

/// DFS cycle detection over depends-on edges. Returns the cycle path if any.
fn find_cycle(s: &Store) -> Option<Vec<String>> {
    let adj: HashMap<&str, &Vec<String>> =
        s.tasks.iter().map(|t| (t.id.as_str(), &t.deps)).collect();
    let mut color: HashMap<&str, u8> = HashMap::new(); // 0 white, 1 gray, 2 black
    let mut stack: Vec<String> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adj: &HashMap<&'a str, &'a Vec<String>>,
        color: &mut HashMap<&'a str, u8>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        color.insert(node, 1);
        stack.push(node.to_string());
        if let Some(deps) = adj.get(node) {
            for d in deps.iter() {
                let ds = d.as_str();
                match color.get(ds).copied().unwrap_or(0) {
                    1 => {
                        let mut cyc = stack.clone();
                        cyc.push(d.clone());
                        return Some(cyc);
                    }
                    0 if adj.contains_key(ds) => {
                        if let Some(c) = dfs(ds, adj, color, stack) {
                            return Some(c);
                        }
                    }
                    _ => {}
                }
            }
        }
        stack.pop();
        color.insert(node, 2);
        None
    }

    for t in &s.tasks {
        if color.get(t.id.as_str()).copied().unwrap_or(0) == 0 {
            if let Some(c) = dfs(t.id.as_str(), &adj, &mut color, &mut stack) {
                return Some(c);
            }
            stack.clear();
        }
    }
    None
}

// ---- apply (T10) ----------------------------------------------------------

/// A structured spec diff supplied by the caller (LLM). kittenscrew never
/// parses freeform prose intent — the caller hands us structure; we
/// validate+write+order (§I, V3). Ops are §T task-lifecycle only.
#[derive(Debug, serde::Deserialize)]
pub struct Diff {
    pub section: String,
    pub op: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Apply one diff to the store in place. Returns Err on a malformed diff
/// (unknown section/op, missing/duplicate id). This only mutates — §V *rule*
/// validation is the caller's separate `validate` gate (V3).
pub fn apply(store: &mut Store, diff: &Diff) -> Result<(), String> {
    let sec = diff.section.trim_start_matches('§');
    if sec != "T" {
        return Err(format!("unsupported section §{sec} (only §T)"));
    }
    match diff.op.as_str() {
        "add" => apply_add(store, &diff.payload),
        "edit" => apply_edit(store, &diff.payload),
        "kill" => apply_status(store, &diff.payload, Status::Killed),
        "done" => apply_status(store, &diff.payload, Status::Done),
        other => Err(format!("unknown op '{other}' (add|edit|kill|done)")),
    }
}

fn pstr(p: &serde_json::Value, key: &str) -> Option<String> {
    p.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn pvec(p: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    p.get(key).map(|v| {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn pint(p: &serde_json::Value, key: &str) -> Option<i64> {
    p.get(key).and_then(|v| v.as_i64())
}

/// Next free `T<n>` id (max numeric suffix + 1).
fn next_task_id(store: &Store) -> String {
    let max = store
        .tasks
        .iter()
        .filter_map(|t| t.id.strip_prefix('T').and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    format!("T{}", max + 1)
}

fn apply_add(store: &mut Store, p: &serde_json::Value) -> Result<(), String> {
    let task = pstr(p, "task").ok_or("add: payload.task required")?;
    let id = match pstr(p, "id") {
        Some(id) => {
            if store.tasks.iter().any(|t| t.id == id) {
                return Err(format!("add: id {id} already exists"));
            }
            id
        }
        None => next_task_id(store),
    };
    store.tasks.push(Task {
        id,
        status: Status::Todo,
        task,
        deps: pvec(p, "deps").unwrap_or_default(),
        priority: p.get("priority").and_then(|v| v.as_i64()).unwrap_or(0),
        cites: pvec(p, "cites").unwrap_or_default(),
        scope: pvec(p, "scope").unwrap_or_default(),
        note: pstr(p, "note").unwrap_or_default(),
        value: pint(p, "value").unwrap_or(0),
        difficulty: pint(p, "difficulty").unwrap_or(0),
        risk: pint(p, "risk").unwrap_or(0),
        ..Default::default()
    });
    Ok(())
}

fn task_mut<'a>(store: &'a mut Store, p: &serde_json::Value) -> Result<&'a mut Task, String> {
    let id = pstr(p, "id").ok_or("payload.id required")?;
    store
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("unknown task {id}"))
}

fn apply_edit(store: &mut Store, p: &serde_json::Value) -> Result<(), String> {
    let t = task_mut(store, p)?;
    // Only provided fields change; absent fields stay. Status unchanged via edit
    // (use kill/done for lifecycle).
    if let Some(task) = pstr(p, "task") {
        t.task = task;
    }
    if let Some(deps) = pvec(p, "deps") {
        t.deps = deps;
    }
    if let Some(cites) = pvec(p, "cites") {
        t.cites = cites;
    }
    if let Some(scope) = pvec(p, "scope") {
        t.scope = scope;
    }
    if let Some(note) = pstr(p, "note") {
        t.note = note;
    }
    if let Some(pri) = pint(p, "priority") {
        t.priority = pri;
    }
    if let Some(v) = pint(p, "value") {
        t.value = v;
    }
    if let Some(d) = pint(p, "difficulty") {
        t.difficulty = d;
    }
    if let Some(r) = pint(p, "risk") {
        t.risk = r;
    }
    Ok(())
}

fn apply_status(store: &mut Store, p: &serde_json::Value, status: Status) -> Result<(), String> {
    let t = task_mut(store, p)?;
    t.status = status;
    if status == Status::Killed {
        if let Some(note) = pstr(p, "note") {
            t.note = note;
        }
    }
    Ok(())
}

// ---- sync guard (T47, V30) -------------------------------------------------

/// True when `spec_md` exactly matches the store's projection — i.e. no manual
/// SPEC.md edit is pending. Render-triggering commands check this first so a
/// hand edit isn't silently clobbered by a re-render from a stale store.
pub fn is_synced(store: &Store, spec_md: &str) -> bool {
    render(store).trim_end() == spec_md.trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Status;

    const SAMPLE: &str = "# SPEC\n\n\
## §G GOAL\n\none-line goal.\n\n\
## §C CONSTRAINTS\n\n- Rust ≥ 1.75\n- single binary\n\n\
## §I INTERFACES\n\n- cmd: `foo` → stdout\n\n\
## §V INVARIANTS\n\nV1: ∀ req → auth\nV2: token ≤ exp\n\n\
## §T TASKS\n\nid|status|task|deps|cites\n\
T1|x|scaffold|-|§I\n\
T2|.|impl auth|T1|V1\n\
T3|∅|custom cache|-|-   (ladder: stdlib covers it)\n\n\
## §B BUGS\n\nid|date|cause|fix\nB1|2026-04-20|token < not ≤|V2\n";

    #[test]
    fn import_parses_all_sections() {
        let s = import(SAMPLE).unwrap();
        assert_eq!(s.goal, "one-line goal.");
        assert_eq!(s.constraints.len(), 2);
        assert_eq!(s.interfaces.len(), 1);
        assert_eq!(s.invariants.len(), 2);
        assert_eq!(s.tasks.len(), 3);
        assert_eq!(s.bugs.len(), 1);
        assert_eq!(s.task("T2").unwrap().deps, vec!["T1".to_string()]);
        assert_eq!(s.task("T2").unwrap().status, Status::Todo);
    }

    #[test]
    fn import_old_4col_format() {
        let old = "## §T TASKS\nid|status|task|cites\nT1|x|scaffold|§I\nT2|.|impl|V1\n";
        let s = import(old).unwrap();
        assert_eq!(s.tasks.len(), 2);
        assert!(s.task("T1").unwrap().deps.is_empty());
        assert_eq!(s.task("T1").unwrap().cites, vec!["§I".to_string()]);
    }

    #[test]
    fn killed_task_note_round_trips() {
        let s = import(SAMPLE).unwrap();
        let t3 = s.task("T3").unwrap();
        assert_eq!(t3.status, Status::Killed);
        assert_eq!(t3.note, "ladder: stdlib covers it");
        assert!(t3.cites.is_empty());
    }

    #[test]
    fn render_then_import_is_stable() {
        let original = import(SAMPLE).unwrap();
        let rendered = render(&original);
        let reparsed = import(&rendered).unwrap();
        assert_eq!(original, reparsed);
    }

    #[test]
    fn validate_clean_spec() {
        let s = import(SAMPLE).unwrap();
        assert!(validate(&s).is_empty(), "violations: {:?}", validate(&s));
    }

    #[test]
    fn validate_flags_missing_dep() {
        let mut s = import(SAMPLE).unwrap();
        s.tasks[1].deps = vec!["T999".into()];
        let v = validate(&s);
        assert!(v.iter().any(|x| x.contains("missing task T999")));
    }

    #[test]
    fn validate_flags_missing_cite() {
        let mut s = import(SAMPLE).unwrap();
        s.tasks[1].cites = vec!["V99".into()];
        let v = validate(&s);
        assert!(v.iter().any(|x| x.contains("missing V99")));
    }

    #[test]
    fn validate_detects_cycle() {
        let mut s = import(SAMPLE).unwrap();
        // T1 → T2 → T1
        s.tasks[0].deps = vec!["T2".into()];
        let v = validate(&s);
        assert!(v.iter().any(|x| x.starts_with("cycle:")), "got {v:?}");
    }

    #[test]
    fn aligned_spaced_header_is_skipped() {
        // maxijinja-style: `id  | st | task | cites`, code-fenced.
        let md = "## §T tasks\n```\nid  | st | task              | cites\nT1  | x  | scaffold workspace | C1\nT2  | .  | wire axum          | V1,V6\n```\n";
        let s = import(md).unwrap();
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.task("T1").unwrap().task, "scaffold workspace");
        assert_eq!(
            s.task("T2").unwrap().cites,
            vec!["V1".to_string(), "V6".to_string()]
        );
    }

    #[test]
    fn unescaped_pipes_in_task_prose_survive() {
        // opengraphene-style: `||` inside task text, 4-col.
        let md = "## §T tasks\nid|status|task|cites\nT42|x|key/iv via sha512(nonce || hex(ss)) byte-identical|V6\n";
        let s = import(md).unwrap();
        assert_eq!(s.tasks.len(), 1);
        let t = s.task("T42").unwrap();
        assert_eq!(t.task, "key/iv via sha512(nonce || hex(ss)) byte-identical");
        assert_eq!(t.cites, vec!["V6".to_string()]);
    }

    #[test]
    fn pipe_in_prose_round_trips() {
        let md = "## §T tasks\nid|status|task|cites\nT1|x|a || b | c|V1\n";
        let original = import(md).unwrap();
        let reparsed = import(&render(&original)).unwrap();
        assert_eq!(original, reparsed);
        assert_eq!(reparsed.task("T1").unwrap().task, "a || b | c");
    }

    #[test]
    fn invariant_three_styles() {
        let md = "## §V INVARIANTS\n\
            V1: colon style\n\
            V2 | pipe table style\n\
            - V3 slug: bullet style\n\
            Verify nothing here\n";
        let s = import(md).unwrap();
        let ids: Vec<&str> = s.invariants.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["V1", "V2", "V3"]);
        assert_eq!(s.invariants[1].text, "pipe table style");
    }

    #[test]
    fn continuation_sections_accumulate() {
        // botm-style: §V scattered across blocks, interleaved with §U (ignored).
        let md = "## §V INVARIANTS\nV1: a\nV2: b\n\
            ## §U STRATEGY\nfree prose, V99 not an invariant line here\n\
            ## §V (cont.)\nV3: c\n";
        let s = import(md).unwrap();
        let ids: Vec<&str> = s.invariants.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["V1", "V2", "V3"]);
    }

    #[test]
    fn expand_unicode_symbols_only() {
        // unicode expanded; ASCII overloads + table pipes preserved.
        assert_eq!(
            expand("∀ req → auth ⊥ skip"),
            "for all req leads to auth never skip"
        );
        assert_eq!(expand("a | b ! c & d"), "a | b ! c & d");
        assert_eq!(
            expand("x ≤ 5, y ≥ 3, z ∈ S"),
            "x at most 5, y at least 3, z in S"
        );
    }

    #[test]
    fn section_extracts_block() {
        let rendered = render(&import(SAMPLE).unwrap());
        let t = section(&rendered, 'T').unwrap();
        assert!(t.starts_with("## §T"));
        assert!(t.contains("T1|x|"));
        assert!(!t.contains("## §B"));
    }

    // ---- apply (T10) ----

    fn diff(json: &str) -> Diff {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn apply_add_assigns_next_id() {
        let mut s = import(SAMPLE).unwrap(); // T1..T3
        apply(&mut s, &diff(r#"{"section":"§T","op":"add","payload":{"task":"new","deps":["T1"],"cites":["V1"]}}"#)).unwrap();
        let t = s.task("T4").unwrap();
        assert_eq!(t.task, "new");
        assert_eq!(t.status, Status::Todo);
        assert_eq!(t.deps, vec!["T1".to_string()]);
        assert!(validate(&s).is_empty());
    }

    #[test]
    fn apply_add_explicit_duplicate_id_errors() {
        let mut s = import(SAMPLE).unwrap();
        let e = apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"add","payload":{"id":"T1","task":"x"}}"#),
        )
        .unwrap_err();
        assert!(e.contains("already exists"));
    }

    #[test]
    fn apply_done_and_kill_change_status() {
        let mut s = import(SAMPLE).unwrap();
        apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"done","payload":{"id":"T2"}}"#),
        )
        .unwrap();
        assert_eq!(s.task("T2").unwrap().status, Status::Done);
        apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"kill","payload":{"id":"T2","note":"superseded"}}"#),
        )
        .unwrap();
        assert_eq!(s.task("T2").unwrap().status, Status::Killed);
        assert_eq!(s.task("T2").unwrap().note, "superseded");
    }

    #[test]
    fn apply_edit_changes_only_given_fields() {
        let mut s = import(SAMPLE).unwrap();
        let before = s.task("T2").unwrap().status;
        apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"edit","payload":{"id":"T2","cites":["V2"]}}"#),
        )
        .unwrap();
        let t = s.task("T2").unwrap();
        assert_eq!(t.cites, vec!["V2".to_string()]);
        assert_eq!(t.task, "impl auth"); // untouched
        assert_eq!(t.status, before); // edit never flips lifecycle
    }

    #[test]
    fn apply_rejects_unknown_section_and_op() {
        let mut s = import(SAMPLE).unwrap();
        assert!(apply(&mut s, &diff(r#"{"section":"§V","op":"add","payload":{}}"#)).is_err());
        assert!(apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"frobnicate","payload":{}}"#)
        )
        .is_err());
    }

    #[test]
    fn apply_edit_unknown_id_errors() {
        let mut s = import(SAMPLE).unwrap();
        assert!(apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"edit","payload":{"id":"T999","task":"x"}}"#)
        )
        .unwrap_err()
        .contains("unknown task T999"));
    }

    #[test]
    fn apply_then_validate_catches_bad_dep() {
        // apply succeeds structurally; the §V gate (validate) is what rejects.
        let mut s = import(SAMPLE).unwrap();
        apply(
            &mut s,
            &diff(r#"{"section":"§T","op":"add","payload":{"task":"x","deps":["T999"]}}"#),
        )
        .unwrap();
        assert!(validate(&s).iter().any(|v| v.contains("missing task T999")));
    }

    #[test]
    fn is_synced_true_for_projection_false_after_edit() {
        let s = import(SAMPLE).unwrap();
        let rendered = render(&s);
        assert!(is_synced(&s, &rendered));
        assert!(is_synced(&s, &format!("{rendered}\n\n"))); // trailing ws ignored
        let edited = rendered.replace("one-line goal.", "TAMPERED");
        assert!(!is_synced(&s, &edited)); // hand edit detected
    }
}
