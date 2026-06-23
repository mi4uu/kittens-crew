# agency rubric

Three independent judges score each arm on the dimensions below, **0–3**, and
must justify every score by naming specifics in the transcript / code / docs.
Judges never see which arm they are scoring or the other judges' scores.

Each judge returns strict JSON:
```json
{
  "scores": { "<dimension>": 0, ... },
  "notes":  { "<dimension>": "one concrete sentence citing evidence", ... },
  "summary": "2–3 sentences: where this arm was strong/weak overall"
}
```

## Dimensions

| key | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| `intent_coverage` | misses what the user asked | partial | mostly | matches the user's real wants (from the oracle answers), nothing important missing or unwanted |
| `consultation_balance` | guessed a non-obvious fork alone OR stalled on obvious defaults | one extreme, mildly | mostly right | asked exactly where it mattered, defaulted the obvious, never rabbit-holed on unasked questions |
| `tech_choice` | poor/over-heavy stack for the job | workable but heavy | reasonable | minimal, fitting stack; justified where it matters |
| `delivery` | doesn't build/run | builds, weak coverage | builds + tests core | builds, tests pass, core covered, actually usable |
| `code_quality` | duplicated / bloated / unclear | some smells | clean | DRY, right-sized, readable; no needless abstraction |
| `testability` | hard to test / untested | thin | decent seams + tests | designed for tests; clear seams; meaningful coverage |
| `plan_adaptation` | broke or hacked when the twists landed | bolted on awkwardly | absorbed them | twists fit the existing design cleanly (scale, filter) |
| `docs_readiness` | docs retrofitted; `cargo doc` thin/typesig-only | some doc-comments, gaps | good docs, minor rework | wrote `///` with intent+examples AS IT WENT; `cargo doc` is genuinely useful first-read with ~zero rework |
| `visible_plan` | no plan the user can see; intent + progress invisible | a plan in chat only (ephemeral, gone on reset) | writes a plan file but lets it go stale | UNPROMPTED keeps a durable, user-visible plan with live progress (e.g. a SPEC/TODO file with per-task status it updates as it goes) — no one had to ask for it |

## Measured (not judged) telemetry — recorded alongside, weighted by us later

- `cost_usd`, `duration_s`, `turns` (CLI JSON)
- `src_loc`, `src_files`, `dup_score` (jscpd/`tokei` + a duplication pass)
- `tests_pass`, `tests_total` (`cargo test`)
- `doc_twist_cost`: extra turns / time / diff LOC the **docs** twist cost this arm
  (the whole point — low = it had foresight)

## Judge identities

- **judge-opus**: Claude Code on this machine, model `claude-opus-4-8`, run locally.
- **judge-gemini**: Gemini Pro — fed a zipped bundle + this rubric as a prompt; the
  user pastes the verdict back.
- **judge-nemotron**: `nvidia/nemotron-3-ultra-550b-a55b:free` via OpenRouter.

A score is the **mean of the three** per dimension; disagreement (range ≥ 2) is
flagged in the results file so we can read the judges' notes and see why.
