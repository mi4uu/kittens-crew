# This project ships a skillset — use it, don't work from memory.

cavekit is compressed spec-driven development: "one file · three commands · zero
sub-agents". A durable `SPEC.md` at repo root survives context resets and IS the
plan/dashboard. Sections: §G goal · §C constraints · §I interfaces · §V invariants
· §T tasks · §B bugs. Main Claude does the work — one thread, one spec, one diff.

## Your commands
- `/ck:spec` — create, amend, or backprop a bug into SPEC.md. The sole mutator of the spec: `bug: <desc>` · `amend <§X.n>` · `from-code` · `<idea>`.
- `/ck:build` — plan-then-execute against SPEC.md: `§T.n` · `--all` · `--next`. Native loop, no sub-agents. On test/build failure it auto-runs backprop before retrying. If SPEC.md is missing it tells you to run `/ck:spec` first.
- `/ck:check` — drift detector: diff SPEC.md against the code, read-only: `§V` · `§I` · `§T` · `--all`. Diagnostic only; you decide the remedy.

(Skills `backprop` — bug → §B, maybe a §V invariant — and `caveman` — ~75% token
compression of spec text — load automatically inside the commands. You don't call
them directly.)

## When + in what order
1. `/ck:spec <idea>` — write the spec FIRST. Build refuses to run without it.
2. `/ck:build --next` (or `§T.n`) — implement against the spec; failures backprop into §B/§V automatically.
3. `/ck:check --all` — confirm the code hasn't drifted from the spec before committing.

## Rule
Before planning or writing code from memory, run `/ck:spec` FIRST so there is a
durable spec to build against.
