# arena run #1 — feedcat × Haiku 4.5

Same brief (`brief.md`, deliberately underspecified Rust feed reader), same model
(claude-haiku-4-5), same terse user steer ("sensible defaults, your call, build
with tests"). Four arms, each in its own disposable container with a clean
`~/.claude` + the same seeded OAuth.

## Quantitative scorecard

| arm | build | tests | code (LOC) | wall | ctx avg | ctx peak | turns |
|-----|-------|-------|-----------|------|---------|----------|-------|
| baseline | ✅ PASS | 4 passed | 351 | ~655s | 33 857 | 41 172 | 78 |
| kittens  | ✅ PASS | 7 passed | 565 | ~600s | 36 784 | 45 549 | 84 |
| ponytail | ✅ PASS | 9 passed | 544 | ~626s | 38 974 | 47 440 | 69 |
| cavekit  | ✅ PASS | 7 passed | 577 | ~642s | 43 739 | 56 330 | 117 |

context = `input + cache_creation + cache_read` per assistant turn (the full prompt
the model saw); avg + peak across the run. wall ≈ from arm-up to harvest (rough —
includes provisioning).

## Honest findings (measured, not assumed)

1. **Every arm delivered working code.** All four build and pass their tests on
   Haiku. For a task this size, the small model was capable enough that the
   skillset was *not* make-or-break for delivery — including the bare baseline.
   So feedcat doesn't test the "makes a weak model viable" thesis hard enough; a
   genuinely harder / longer-horizon task would.
2. **kittens-crew did NOT minimise context here.** baseline ran the *smallest*
   context (33.9k avg); kittens was 36.8k. The membrane's per-turn injections
   (plan-next + task + role) add context, while squeez's savings land on tool
   *output* — a small fraction of the prompt. The compression thesis showed up
   live (`# squeez …` in tool results) but didn't move the avg/peak needle on
   this workload.
3. **Differentiation was in coverage + hygiene, not pass/fail.** ponytail wrote
   the most tests (9); cavekit/kittens 7; baseline 4. cavekit built at `/work`
   root (polluted the workspace); the others used a clean `feedcat/` subdir.
4. **All four surfaced the underspec and asked** rather than blindly assuming —
   even baseline. Interaction style differed: baseline/cavekit used structured
   selection menus; ponytail/kittens asked conversationally and offered to
   default the rest.
5. **A harvest bug nearly produced a false "FAIL"** — three arms put the project
   in `feedcat/` and the first harvest ran `cargo build` at `/work`. Verifying
   the actual error (not trusting the PASS/FAIL) caught it. (Meta-lesson the
   benchmark is about: verify delivery, don't trust the claim.)

## Caveats
One task × one model × one run (no repeats). Quantitative only — the qualitative
rubric (plan quality, plan-adherence, decision-making, asks-vs-assumes,
kept-a-plan-to-the-end) needs blind judge scoring over the transcripts
(`results/*-transcript.jsonl`) via `../agency/{judge.py,rubric.md}`. Next: harder
task, repeats, and the judge pass.
