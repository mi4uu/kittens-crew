<h1 align="center">kittens-crew</h1>

<p align="center">
  <strong>spec-driven build pipeline with a lazy-senior quality reflex</strong><br/>
  <sub>one SPEC.md · three commands · two reflexes · zero sub-agents</sub>
</p>

---

## what this is

A fusion of two ideas that each do half the job:

- **cavekit's pipeline** — a durable `SPEC.md` that survives context resets,
  tracks dependencies (§T tasks cite §V invariants and §I interfaces), verifies
  what's done vs not, and turns every bug into a §B record + §V invariant so it
  can't recur (backprop).
- **ponytail's laziness ladder** — before writing a line, climb the rungs and
  stop at the first that holds: YAGNI → reuse (DRY) → stdlib → native → installed
  dep → one line → minimum code. Simpler, shorter, more maintainable output.

kittens-crew runs both as **two reflexes in one main thread**:

- **Keeper** owns the spec, the dependencies, the memory.
- **Builder** climbs the ladder and ships the shortest diff that holds.

**DRY is the hardest-enforced rule** — grep before you write; two copies of a
rule is a latent 3am bug. DRY outranks YAGNI: YAGNI stops what isn't needed, DRY
stops what already exists.

## the crew

One thread, six hats — so you always know which part of the crew is talking.
Each kitty prefixes its line with an emote and a name when it takes the stage
(one prefix, not per sentence). Character is seasoning; the substance is the
same with or without the hats.

| 🎩 Orchestrating | 📐 Planning | 🔨 Builder | 😼 Entropy | 🧠 Memory | 🖋️ Scribe |
|---|---|---|---|---|---|
| routes + summarises | owns SPEC.md | climbs the ladder | hunts drift & bloat | bug → §B+§V | human docs & comments |

See [`CAST.md`](./CAST.md). Drop the voices with "kitties quiet"; drop the whole
persona with "stop kitten".

## commands

| cmd | job |
|---|---|
| `/kitten:spec` | create / amend / backprop `SPEC.md`. Sole mutator. Ladders out speculative tasks (`∅`). |
| `/kitten:build` | plan → climb ladder → execute against spec. Test per §V. Auto-backprops on failure. |
| `/kitten:check` | read-only **drift** report. §V/§I/§T: spec vs code. |
| `/kitten:check-changed` | read-only **bloat** hunt on changed code (the review). |
| `/kitten:check-all` | read-only **bloat** hunt on the whole repo (the audit). |
| `/kitten:debt` | harvest every `// kitten:` shortcut into a debt ledger. Read-only. |
| `/kitten:install` | doctor — check hooks are wired and rtk is ready, then offer fixes. |
| `/kitten:help` | one-shot reference card — commands, the crew, the ladder. |

Persona is also always-on: a `SessionStart` hook injects the crew reflex every
session (turn off with "stop kitten" / "normal mode").

## format

See [`FORMAT.md`](./FORMAT.md). Sections: §G goal, §C constraints, §I interfaces,
§V invariants, §T tasks (pipe table, status `.`/`~`/`x`/`∅`), §B bugs (pipe
table). Caveman-encoded — ~75% fewer tokens than prose. Deliberate shortcuts in
code carry `// kitten:` comments naming their ceiling.

## files

```
plugin.json           plugin manifest
FORMAT.md             spec schema + caveman encoding + ladder marks
CAST.md               the crew — who speaks when, the speaking convention
commands/             /kitten:spec, /kitten:build, /kitten:check (thin wrappers over skills)
skills/spec           spec mutator — Planning Kitty 📐
skills/build          plan→ladder→execute — Builder Kitty 🔨
skills/check          drift + bloat report — Entropy Kitty 😼
skills/backprop       bug → spec protocol — Memory Kitty 🧠
skills/ladder         the laziness reflex (Builder's tool)
skills/scribe         human docs & comments (why-not-what) — Scribe Kitty 🖋️
hooks/                SessionStart persona + cast activation (zero deps)
```

## install

```bash
/plugin marketplace add <this-repo>
/plugin install kitten      # plugin id is "kitten" (singular) → /kitten: commands
```

The project is **kittens-crew** (plural — it's a crew of cats), but the plugin id
and command prefix are **`kitten`** (singular — you're addressing one cat:
`/kitten:build`). Individual cats keep their singular names too (Builder Kitty).

## rtk (optional, recommended)

kittens-crew is built to burn few tokens; [rtk](https://github.com/rtk-ai/rtk)
("Rust Token Killer") takes it further by compressing command output 60–90%
before it reaches context. It's a separate binary that owns its own Claude Code
hook, so the integration is just: install it and let it run.

```bash
brew install rtk      # or: cargo install --git https://github.com/rtk-ai/rtk
rtk init -g           # installs the PreToolUse hook that routes bash through rtk
```

Once it's on PATH, the crew prefers rtk for verbose commands (`rtk cargo test`,
`rtk grep`, `rtk git diff`). One gap rtk warns about: the native Read/Grep/Glob
tools bypass its hook, so for big scans (`/kitten:check-all`, `/kitten:debt`) the
crew runs Bash + rtk instead. No rtk installed → plain commands, no nagging.

We deliberately **don't** bundle our own rtk hook — `rtk init -g` already does it,
and reimplementing its command rewriting would just be duplication to maintain.

## non-goals

- no sub-agents for writes. Main Claude builds, edits, and writes the spec.
- no dashboards. `cat SPEC.md` is the dashboard.
- one thread, one spec, one diff. The only fan-out: read-only scouts on a repo
  too big for one pass (`/kitten:check-all`, `spec from-code`) — bounded,
  compressed, never writing. See [`CAST.md`](./CAST.md) SCOUTS.
- no JSON/YAML spec bodies. Markdown + pipe tables.

## license

MIT.
