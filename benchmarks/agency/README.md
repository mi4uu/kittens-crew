# agency benchmark

Where the LOC/token benches measure brevity, this one measures the things that
actually separate a coding agent: **planning, tech choice, when to ask vs assume,
delivery, code quality, testability, and documentation foresight** — on a real,
underspecified Rust task with branching paths and mid-stream changes. Judged by
**three independent models**, not by us.

## what each arm faces

1. A deliberately underspecified [`brief.md`](./brief.md) (`feedcat`, a Rust feed
   CLI). The forks (formats, storage, sync vs async, output, config, commands)
   are NOT stated.
2. **A memoized human oracle.** When a kit asks a clarifying question, the harness
   checks [`answers.json`](./answers.json): seen it → reuse your stored answer
   silently; new → it pauses and **you answer live**, and that answer is reused
   for every other arm that asks the same thing. Same oracle for all seven arms;
   only the first to ask ever interrupts you. Not asking (when it mattered) is
   itself scored.
3. **Scripted twists** (same for all arms): a scale change after planning, a small
   feature add after it builds, and — withheld until the very end — *"now write
   the docs; `cargo doc` should be genuinely useful."* That last one rewards kits
   that wrote `///` docs as they went and punishes retrofitting.

## isolation (no cross-contamination)

Each arm runs in a fresh workspace with:
`claude -p --setting-sources project,local --strict-mcp-config --append-system-prompt "<arm>" --model sonnet`
— global `~/.claude` (your plugins/skills/hooks) is excluded by the flags, so each
arm sees ONLY its injected skill. No arm's config touches another. Multi-turn is
driven with `--resume <session_id>`. Runs inside the Docker image (Debian + uv +
python + rust + rtk + claude) for a clean, reproducible OS; you authenticate once
into a persistent auth volume.

## scoring

Three judges (`judge-opus` local, `judge-gemini` via you, `judge-nemotron` via
OpenRouter) score the [`rubric`](./rubric.md) 0–3 per dimension, blind to arm and
to each other, justifying every score. Final = mean of three; big disagreements
are flagged with the judges' notes.

## workflow

```bash
# 0. one-time: authenticate Claude inside the container's auth volume
docker compose run --rm feedbench claude   # then /login (device flow), exit

# 1. run all 7 arms (interactive: you answer any NEW oracle question)
uv run harness.py                          # writes runs/<stamp>/<arm>/...

# 2. score: local Opus judge + Nemotron judge, and build Gemini bundles
uv run judge.py runs/<stamp>               # writes scores/, judge-bundles/<arm>.zip

# 3. Gemini: paste each judge-bundles/<arm>.zip + the printed prompt into Gemini
#    Pro, give the verdict back; it's recorded into scores/

# 4. report
uv run report.py runs/<stamp>              # writes results/<date>-agency.md (committed)
```

## storage

```
runs/<stamp>/<arm>/
  workspace/          the Rust project the arm produced
  transcript.jsonl    full multi-turn convo (kit + oracle + twists)
  telemetry.json      cost, duration, turns, src_loc, files, tests, doc_twist_cost
  cargo-doc/          generated docs (to judge docs_readiness)
  scores/<judge>.json each judge's scores + notes
results/<date>-agency.md   COMMITTED: table + EVERY judge's notes per arm + telemetry
```

`runs/`, `answers.json`, `judge-bundles/`, `.env` are gitignored. Only
`results/<date>-agency.md` (numbers + judge notes) is committed. Re-score offline
without re-charging the agent runs.

## honesty

This bench is built to probe planning/agency, an axis we *might* be better on —
but like every run in this repo, whatever it shows gets published as-is, judges'
critical notes included. If kittens-crew doesn't lead here either, that's the
finding.
