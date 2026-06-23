<p align="center">
  <img src="./kittenslogo.png" alt="kittens-crew" width="640"/>
</p>

<p align="center">
  <strong>spec-driven build pipeline with a lazy-senior quality reflex</strong><br/>
  <sub>one SPEC.md · a crew of kitties · no sub-agents · caveman-compressed</sub>
</p>

---

## what this is

Two proven ideas, welded into one crew:

- **A durable spec** — `SPEC.md` at the repo root survives context resets. Tasks
  cite the invariants and interfaces they touch (`§T → §V, §I`), so the pipeline
  always knows what's done, what's pending, and what depends on what. Every bug
  becomes a `§B` record plus a `§V` invariant, so it can't come back. That last
  step is **backprop** — fix the code *and* edit the spec.
- **A laziness ladder** — before writing a line, climb the rungs and stop at the
  first that holds: YAGNI → reuse (DRY) → stdlib → native → installed dep → one
  line → minimum code. Shorter diffs, fewer dependencies, code that's easier to
  read and maintain.

**DRY is the hardest-enforced rule.** Grep before you write; two copies of a
rule is a latent 3am bug. DRY outranks YAGNI — YAGNI stops what isn't needed,
DRY stops what already exists.

## the crew

One main thread wearing six hats, so you always know which part of the crew is
talking. Each kitty prefixes its line with an emote and a name when it takes the
stage. Character is seasoning; the substance is identical with or without the hats.

| | kitty | role | voice |
|---|---|---|---|
| 🎩 | **Orchestrating** | routes work, writes the closing summary | calm, in charge |
| 📐 | **Planning** | owns `SPEC.md` | thoughtful, precise |
| 🔨 | **Builder** | climbs the ladder, ships the shortest diff | laid-back senior |
| 😼 | **Entropy** | hunts drift, bloat, duplication | the gleeful troublemaker |
| 🧠 | **Memory** | turns bugs into `§B` + `§V` | quiet, never forgets |
| 🖋️ | **Scribe** | human docs & comments (why, not what) | warm, plain-spoken |

See [`CAST.md`](./CAST.md). Drop the voices with "kitties quiet"; drop the whole
persona with "stop kitten".

## commands

The project is **kittens-crew** (plural, a crew of cats). The plugin id and the
command prefix are **`kitten`** (singular — you're addressing one cat).

| command | job |
|---|---|
| `/kitten:spec` | create / amend / backprop `SPEC.md`. Sole mutator. Ladders out speculative tasks (`∅`). |
| `/kitten:build` | plan → climb ladder → execute. Test per `§V`. Auto-backprops on failure. |
| `/kitten:check` | read-only **drift** report — `§V`/`§I`/`§T`: spec vs code. |
| `/kitten:check-changed` | read-only **bloat** hunt on changed code (the review). |
| `/kitten:check-all` | read-only **bloat** hunt on the whole repo (the audit). |
| `/kitten:debt` | harvest every `// kitten:` shortcut into a debt ledger. |
| `/kitten:install` | doctor — check the hooks are wired and rtk is ready. |
| `/kitten:help` | one-shot reference card. |

A runtime-free `SessionStart` hook keeps the crew persona always-on.

## benchmarks

Real and reproducible — run it yourself, no fabricated numbers.

```bash
cd benchmarks && bun install && bun bench
```

Caveman `SPEC.md` vs the **same spec** written as a normal prose PRD, tokenized
with `o200k_base`:

| encoding | tokens | chars | lines |
|---|---:|---:|---:|
| prose PRD | 596 | 2654 | 60 |
| **caveman SPEC.md** | **279** | 725 | 32 |

**53% fewer tokens for the same spec** (596 → 279), and the spec reloads on every
command, so the saving recurs each call. The exact ratio depends on the spec —
run the bench on your own. Agentic pass-rate evals need real model runs; the
method is documented in [`benchmarks/`](./benchmarks/) rather than faked.

## format

See [`FORMAT.md`](./FORMAT.md). Sections: `§G` goal, `§C` constraints, `§I`
interfaces, `§V` invariants, `§T` tasks (status `.`/`~`/`x`/`∅`), `§B` bugs.
Caveman-encoded. Deliberate shortcuts in code carry `// kitten:` comments naming
their ceiling and upgrade path.

## install

```bash
/plugin marketplace add mi4uu/kittens-crew
/plugin install kitten        # plugin id is "kitten" → /kitten: commands
```

## rtk (optional, recommended)

kittens-crew is built to burn few tokens; [rtk](https://github.com/rtk-ai/rtk)
("Rust Token Killer") goes further, compressing command output 60–90% before it
reaches context. It's a separate binary that owns its own Claude Code hook:

```bash
brew install rtk      # or: cargo install --git https://github.com/rtk-ai/rtk
rtk init -g           # installs the PreToolUse hook that routes bash through rtk
```

Once it's on PATH, the crew prefers rtk for verbose commands. One gap rtk warns
about: the native Read/Grep/Glob tools bypass its hook, so for big scans
(`/kitten:check-all`, `/kitten:debt`) the crew uses Bash + rtk. No rtk → plain
commands, no nagging. We deliberately don't bundle our own rtk hook — `rtk init
-g` already does it, and reimplementing it would just be duplication.

## non-goals

- No sub-agents for writes. Main Claude builds, edits, and writes the spec.
- No dashboards. `cat SPEC.md` is the dashboard.
- One thread, one spec, one diff. The only fan-out: read-only scouts on a repo
  too big for one pass (`/kitten:check-all`, `spec from-code`) — bounded,
  compressed, never writing. See [`CAST.md`](./CAST.md) SCOUTS.
- No JSON/YAML spec bodies. Markdown + pipe tables.

## thanks

kittens-crew stands on the shoulders of [**Julius Brussee**](https://github.com/JuliusBrussee).
His work — [cavekit](https://github.com/JuliusBrussee/cavekit) (the spec
pipeline), [caveman](https://github.com/JuliusBrussee/caveman) (token
compression), and [caveman-code](https://github.com/JuliusBrussee/caveman-code) —
was the inspiration and the base this is built on. The laziness ladder is owed to
[ponytail](https://github.com/DietrichGebert/ponytail). Thank you for the
groundwork.

## license

MIT.
