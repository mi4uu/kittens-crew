# arena run #1 — feedcat × Haiku 4.5

Same brief (`brief.md`, deliberately underspecified Rust feed reader), same model
(claude-haiku-4-5), same terse user steer ("sensible defaults, your call, build
with tests"). Four arms, each in its own disposable container with a clean
`~/.claude` + the same seeded OAuth.

## Quantitative scorecard

A **turn** = one assistant API response (one model generation). **Total tokens** =
the real cost: input(uncached) + cache-creation + cache-read + output, summed over
every turn — dominated by cache-read (≈ context × turns). **LOC** = Rust code incl.
inline `#[cfg(test)]` tests (no separate test files; no markdown docs landed in the
project dir — the "README included" claims were hollow).

| arm | build | tests | LOC | turns | ctx avg | ctx peak | **TOTAL tokens** |
|-----|-------|-------|-----|-------|---------|----------|------------------|
| baseline | ✅ | 4 | 388 | 78 | 33 857 | 41 172 | **2.66 M** |
| ponytail | ✅ | 9 | 629 | 69 | 38 974 | 47 440 | **2.72 M** |
| kittens  | ✅ | 7 | 635 | 84 | 36 784 | 45 549 | **3.12 M** |
| cavekit  | ✅ | 7 | 647 | 129 | 43 739 | 56 330 | **5.86 M** |

**Cost = context-per-turn × turns.** kittens burned MORE than baseline (3.12M vs
2.66M); cavekit 2.2× baseline. A trivial feed reader cost 2.7–5.9 **million**
tokens — the cache-read tax of a long agentic loop. Simplified per-arm stories:
`results/<arm>-story.txt`.

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

## The big finding: kittens-crew behaved like a chat window
The arm with our skillset **never made a plan or a spec** — it read the brief,
asked a couple of questions, and built directly, exactly like baseline. The
membrane was wired (config + 8 hooks present), but **nothing enforces plan-first**.
That's the gap: a weak model left to free-build burns tokens wandering (kittens
3.12M > baseline 2.66M) and overclaims ("ready for daily use", hollow README).
The fix — captured as spec tasks T57–T59 — is **"no plan → no work"**: casual
chatter is distilled into a draft plan the user confirms; building is gated on a
saved plan; small-brained models get the work pre-divided into pieces they can
handle, then the gates (`check done`) verify. Cheap cat-voice subagents (separate
tiny context, cheapest model) handle the "say it nicely" bits. Run starting from
`/spec` / a forced plan would likely tell a very different story — that's run #2.

## Caveats
One task × one model × one run (no repeats). Quantitative only — the qualitative
rubric (plan quality, plan-adherence, decision-making, asks-vs-assumes,
kept-a-plan-to-the-end) needs blind judge scoring over the transcripts
(`results/*-transcript.jsonl`) via `../agency/{judge.py,rubric.md}`. Next: harder
task, repeats, and the judge pass.
