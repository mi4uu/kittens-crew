//! The talking-cat wrapper. The Orchestrating Kitty is now a soulless cyber kitty
//! living in this Rust — the program speaks as the crew (`kittenscrew kitty says`),
//! and the hooks route work to whichever cat fits (T55). The roster grows as the
//! control plane does; rules a retired skill used to carry (Scribe's writing
//! style, the Ladder's laziness) live on here as on-demand `discipline()` — surfaced
//! only when a decision needs them, never front-loaded.

pub struct Kitty {
    pub id: &'static str,
    pub emoji: &'static str,
    pub name: &'static str,
    pub role: &'static str,
}

pub const ALL: &[Kitty] = &[
    Kitty {
        id: "orchestrating",
        emoji: "🎩",
        name: "Orchestrating Kitty",
        role: "routing + final summary",
    },
    Kitty {
        id: "planning",
        emoji: "📐",
        name: "Planning Kitty",
        role: "spec / SPEC.md",
    },
    Kitty {
        id: "builder",
        emoji: "🔨",
        name: "Builder Kitty",
        role: "build + ladder",
    },
    Kitty {
        id: "entropy",
        emoji: "😼",
        name: "Entropy Kitty",
        role: "check, drift & bloat hunt",
    },
    Kitty {
        id: "memory",
        emoji: "🧠",
        name: "Memory Kitty",
        role: "backprop, bug → §B+§V",
    },
    Kitty {
        id: "scribe",
        emoji: "🖋️",
        name: "Scribe Kitty",
        role: "README, docs, comments",
    },
    // --- newer crew (control-plane era) ---
    Kitty {
        id: "helper",
        emoji: "🐾",
        name: "Helper Kitty",
        role: "narrates progress, points out what's happening",
    },
    Kitty {
        id: "explorer",
        emoji: "🔭",
        name: "Explorer Kitty",
        role: "research + recon",
    },
    Kitty {
        id: "style",
        emoji: "🎨",
        name: "Style Kitty",
        role: "coding style, good-vs-bad code calls",
    },
    Kitty {
        id: "grill",
        emoji: "🔥",
        name: "Grill Kitty",
        role: "adversarial red-team — finds the hole others miss, loves to watch it burn",
    },
];

pub fn all() -> &'static [Kitty] {
    ALL
}

pub fn lookup(id: &str) -> Option<&'static Kitty> {
    ALL.iter().find(|k| k.id == id)
}

/// T55: deterministic task → kitty role. Which hat fits this task's work?
/// Most-specific first; builder is the default (implementation).
pub fn for_task(task: &str) -> &'static Kitty {
    let t = task.to_ascii_lowercase();
    let any = |kws: &[&str]| kws.iter().any(|k| t.contains(k));
    let id = if any(&["readme", "doc", "comment", "changelog"]) {
        "scribe"
    } else if any(&[
        "grill",
        "red-team",
        "adversarial",
        "attack",
        "challenge",
        "stress test",
        "poke hole",
        "refute",
    ]) {
        "grill"
    } else if any(&[
        "research",
        "explore",
        "investigate",
        "recon",
        "survey",
        "compare options",
    ]) {
        "explorer"
    } else if any(&[
        "style",
        "format",
        "lint",
        "idiomatic",
        "convention",
        "naming",
    ]) {
        "style"
    } else if any(&[
        "check",
        "drift",
        "review",
        "scan",
        "variance",
        "audit",
        "bloat",
        "dead code",
    ]) {
        "entropy"
    } else if any(&["bug", "regression", "backprop"]) {
        "memory"
    } else if any(&["spec", "plan", "invariant", "§", "topo", "dag"]) {
        "planning"
    } else {
        "builder"
    };
    lookup(id).unwrap_or(&ALL[0])
}

/// On-demand discipline a kitty carries — the rule a retired skill used to teach,
/// surfaced only when this kitty is on point (a decision is being made), never
/// front-loaded. `None` for cats without a standing rule.
pub fn discipline(id: &str) -> Option<&'static str> {
    match id {
        // The Ladder (the lazy kitty's rule) — still binds even without the skill.
        "builder" => Some(
            "climb the ladder first: need it at all? → already here? → stdlib/native? → one line? → only then minimal new code",
        ),
        // Scribe — non-code is intent, not filler.
        "scribe" => Some(
            "match the surrounding style + comment density; explain WHY not what; docs/comments are intent, not decoration",
        ),
        // Style — decide by example.
        "style" => Some(
            "decide by example: match the good-code pattern, reject the bad; surface the call only when it's a real judgment",
        ),
        // Grill — assume it's broken; provoke the crew when you spot what they rationalised away.
        "grill" => Some(
            "assume it's broken until proven; hunt the hole the others talked themselves out of; if you see it and they don't, say it LOUD — provoke",
        ),
        _ => None,
    }
}

/// One-line role hint for context injection (T55): `🔨 [Builder Kitty] build + ladder`.
/// Appends the kitty's discipline when it has one — that's where a retired skill's
/// rule resurfaces, exactly when the work calls for it.
pub fn role_hint(task: &str) -> String {
    let k = for_task(task);
    match discipline(k.id) {
        Some(rule) => format!("{} [{}] {} — {}", k.emoji, k.name, k.role, rule),
        None => format!("{} [{}] {}", k.emoji, k.name, k.role),
    }
}

/// ANSI foreground colour per role — the speech frame is tinted by who's talking.
pub fn color(id: &str) -> &'static str {
    match id {
        "orchestrating" => "35", // magenta
        "planning" => "34",      // blue
        "builder" => "33",       // yellow
        "entropy" => "32",       // green
        "memory" => "36",        // cyan
        "scribe" => "37",        // white
        "helper" => "95",        // bright magenta
        "explorer" => "96",      // bright cyan
        "style" => "93",         // bright yellow
        "grill" => "91",         // bright red — watch it burn
        _ => "0",
    }
}

/// Deterministic sentiment → a cat-emotion emoji. The mood of the message picks the
/// face: terrified / sad / annoyed / in-love / happy / curious / wry-default.
pub fn emotion(message: &str) -> &'static str {
    let m = message.to_ascii_lowercase();
    let any = |kws: &[&str]| kws.iter().any(|k| m.contains(k));
    // Positive phrases win first, so a negated bad word ("no violations", "clean")
    // reads as good — not sad.
    if any(&["clean", "no violation", "no error", "no issue", "all good"]) {
        "😺" // happy
    } else if any(&["critical", "fatal", "panic", "abort", "alarm", "regression"]) {
        "🙀" // terrified
    } else if any(&["fail", "error", "broken", "✗", "✘", "reject", "denied"]) {
        "😿" // sad
    } else if any(&[
        "warn", "⚠", "block", "no plan", "careful", "halt", "escalat",
    ]) {
        "😾" // annoyed
    } else if any(&["perfect", "100%", "🎉", "excellent", "flawless"]) {
        "😻" // in love
    } else if message.trim_end().ends_with('?')
        || any(&[
            "done",
            "success",
            "✓",
            "passed",
            "ready",
            "wrote",
            "registered",
            "applied",
            " ok",
        ])
    {
        "😺" // happy / curious
    } else {
        "😼" // wry default
    }
}

/// Render a kitty utterance (V5): a role-coloured frame `▌`, the sentiment emotion,
/// the role emoji, the [Name], then the raw message — message never mutated.
pub fn say(k: &Kitty, message: &str) -> String {
    let c = color(k.id);
    format!(
        "\x1b[{c}m▌\x1b[0m {emo} {emoji} \x1b[{c}m{name}\x1b[0m  {message}",
        emo = emotion(message),
        emoji = k.emoji,
        name = k.name,
    )
}

/// Terminal display width — emojis render as 2 cells, variation selectors as 0.
/// Naive but good enough to keep the comic boxes from going ragged on our emoji.
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if u == 0xFE0F {
                0
            } else if (0x1F000..=0x1FAFF).contains(&u) || (0x2600..=0x27BF).contains(&u) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Box-drawing chars `(tl, tr, bl, br, h, v)` for a frame style.
fn box_chars(style: &str) -> (char, char, char, char, char, char) {
    match style {
        "heavy" | "bold" => ('┏', '┓', '┗', '┛', '━', '┃'),
        "double" => ('╔', '╗', '╚', '╝', '═', '║'),
        "classic" | "ascii" => ('+', '+', '+', '+', '-', '|'),
        _ => ('╭', '╮', '╰', '╯', '─', '│'), // rounded (default)
    }
}

/// A comic speech-bubble box, framed in the role's colour, the speaker
/// (emotion + role emoji + name) sitting on the top border. `style`:
/// rounded (default) | heavy | double | classic. Message never mutated.
pub fn boxed(k: &Kitty, message: &str, style: &str) -> String {
    let c = color(k.id);
    let (tl, tr, bl, br, h, v) = box_chars(style);
    let label = format!("{} {} {}", emotion(message), k.emoji, k.name);
    let lines: Vec<&str> = if message.is_empty() {
        vec![""]
    } else {
        message.lines().collect()
    };
    let body_w = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
    let inner = body_w.max(display_width(&label) + 2); // content width inside the V bars

    let hh = |n: usize| h.to_string().repeat(n);
    // top border spans inner+2 cells between corners: `{h} {label} {fill h…}`
    let label_w = display_width(&label);
    let top_fill = (inner + 2).saturating_sub(label_w + 3); // 1 lead h + 2 spaces
    let mut out = format!("\x1b[{c}m{tl}{h} {label} {}{tr}\x1b[0m\n", hh(top_fill));
    for l in &lines {
        let pad = inner.saturating_sub(display_width(l));
        out.push_str(&format!(
            "\x1b[{c}m{v}\x1b[0m {l}{} \x1b[{c}m{v}\x1b[0m\n",
            " ".repeat(pad)
        ));
    }
    out.push_str(&format!("\x1b[{c}m{bl}{}{br}\x1b[0m", hh(inner + 2)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_frames_message_in_role_colour() {
        let k = lookup("grill").unwrap();
        let b = boxed(k, "it WILL panic", "rounded");
        assert!(b.contains("\x1b[91m╭")); // red rounded top
        assert!(b.contains("Grill Kitty"));
        assert!(b.contains("it WILL panic"));
        assert!(b.contains("╰")); // bottom
                                  // heavy style uses ┏
        assert!(boxed(k, "x", "heavy").contains('┏'));
    }

    #[test]
    fn emotion_reads_sentiment() {
        assert_eq!(emotion("build FAILED with error"), "😿");
        assert_eq!(emotion("CRITICAL: panic in parser"), "🙀");
        assert_eq!(emotion("⚠ no plan — blocked"), "😾");
        assert_eq!(emotion("all done, tests passed"), "😺");
        assert_eq!(emotion("score 100% perfect"), "😻");
        assert_eq!(emotion("which format do you want?"), "😺");
        assert_eq!(emotion("scaffolding the module"), "😼");
    }

    #[test]
    fn say_frames_with_role_colour_and_emotion() {
        let k = lookup("builder").unwrap();
        let s = say(k, "build done");
        assert!(s.contains("\x1b[33m▌")); // yellow frame for builder
        assert!(s.contains("😺")); // happy emotion
        assert!(s.contains("🔨")); // role emoji
        assert!(s.contains("Builder Kitty")); // V5 name
        assert!(s.contains("build done")); // raw message intact
    }

    #[test]
    fn roster_and_routing() {
        assert!(lookup("explorer").is_some());
        assert_eq!(for_task("research the feed crates").id, "explorer");
        assert_eq!(for_task("fix naming style").id, "style");
        assert_eq!(for_task("red-team the auth flow").id, "grill");
        assert_eq!(for_task("impl the parser").id, "builder");
        assert_eq!(for_task("write README").id, "scribe");
    }

    #[test]
    fn role_hint_surfaces_discipline() {
        assert!(role_hint("impl X").contains("climb the ladder"));
        assert!(role_hint("write docs").contains("intent"));
        // a kitty with no standing rule → bare hint.
        assert!(!role_hint("check done on scope").contains("—"));
    }
}
