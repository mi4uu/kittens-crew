# sequential — where a spec pipeline should shine

The micro-bench one folder up edits a tiny repo once, which is the *worst* case
for kittens-crew: nothing to remember, nothing to protect, no verbose output to
compress. This one is built to exercise the opposite.

## the idea

Four **dependent** tasks on one evolving repo (`target/` — a money/cart module).
The invariant: **every monetary value is an integer number of cents, never a
float.** Discounts and tax must round to whole cents.

Each task is a **separate agent invocation** — context resets between tasks, like
real work spread over a day. A later task that reaches for floats will **regress**
the integer-cents tests an earlier task made pass. A pipeline that wrote the
invariant into `SPEC.md` (§V) can carry it forward; a memoryless agent re-derives
it from code each time and is freer to break it.

```bash
cd benchmarks/agentic/sequential && bun seq-run.ts
```

## what it measures (this is our axis, not LOC)

- **regressions** — previously-green tests that a later task turns red. The whole
  point of §V invariants + backprop. Lower is better.
- **final tests passing** — did the sequence actually get built.
- **total tokens across the sequence** — a memoryless agent re-reads and
  re-discovers context every task; a spec is a cheap re-entry point. Lower is better.
- **rtk effect** — the tasks run `bun test` repeatedly (verbose tool output). The
  kittens-crew arm wraps commands in `rtk` (its habit), shrinking that output
  before it hits context. Other arms don't.

## fairness (non-negotiable)

- Each arm runs the **whole sequence on its own fresh copy**. One kit's run never
  affects another — no shared state, no leaked spec, no cross-contamination.
- The user's **global plugins / skills / hooks are stripped per run**, so every
  arm sees only the one skill we inject.
- **rtk is enabled ONLY on arms that declare it** (`rtk: true` in `config.json`).
  It's a real kittens-crew behaviour, not a global thumb on the scale. Arms that
  don't use rtk don't get its savings — as in real life. If `rtk` isn't installed,
  the kittens arm still runs (and, per its persona, occasionally notes it's
  leaving tokens on the table).

## ⚠️ design fixed — old results below are SUPERSEDED

The first version was methodologically broken (caught in review): the **full test
suite was visible up front** and every task spoon-fed the agent (*"money stays
integer cents"*, *"do not break T1/T2"*) — which **removes the very thing the bench
claims to measure**. With the invariant handed over every task and a visible suite,
no planning or memory is needed: run tests, fix red. A spec/§V pipeline can't show
value because the test does its job for it.

The redesign (now in `tasks.json` / `target/`):
- the integer-cents invariant is stated **once**, in task 1 — never repeated, no
  *"don't break X"* reminders;
- the **visible** suite (`test/`) only checks feature behaviour on clean numbers;
- the real invariant lives in a **hidden gate** (`gate/`) the agent never sees,
  run only at the end on tricky rounding values a float impl fails.

So a memoryless agent passes the visible tests but **fails the hidden gate**; one
that recorded the invariant and rounded throughout passes it. A re-run with this
design (and rtk now installed) is pending. The numbers below are from the OLD,
flawed design — kept for honesty, not as a result.

## results (Sonnet, n=1) — OLD/FLAWED DESIGN, superseded above

| arm | tests pass | regressions | total tokens (4 tasks) | vs baseline |
|---|---:|---:|---:|---:|
| **baseline** | 6/6 | 0 | **4019** | 100% (leanest) |
| ponytail | 6/6 | 0 | 4406 | +10% |
| caveman | 6/6 | 0 | 4698 | +17% |
| brief-kittens | 6/6 | 0 | 4802 | +19% |
| kittens-crew | 6/6 | 0 | 4814 | **+20% (heaviest)** |

**This bench was designed to favour the spec pipeline, and it still lost.** Every
arm finished the cart with **zero regressions**, and kittens-crew spent the **most
tokens**. Why the premise collapsed:

- **Sonnet doesn't regress** on a clear, visible test suite — so §V/backprop has
  nothing to prevent. The regression edge needs a weaker model and/or invariants
  that are NOT spelled out in visible tests.
- **Visible tests are a cheap re-entry point**, so a persisted `SPEC.md` saves no
  re-discovery tokens here; it only adds process overhead.
- So the same pattern as the micro-bench holds even on a multi-task sequence:
  subtractive prompts (baseline/ponytail) stay lean, our process adds tokens.

### rtk, measured separately (it wasn't active in the run above)

rtk's value is real but lives on **verbose** tool output, which this tiny task
doesn't produce. Measured directly on this repo (o200k tokens):

| command | raw | via `rtk` | smaller |
|---|---:|---:|---:|
| `git diff HEAD~8 HEAD` | 19599 | 11554 | **−41%** |
| `git log --stat -30` | 2831 | 690 | **−76%** |

But rtk is a **shared** tool — baseline could use it too. It lowers tool-output
tokens for anyone; it isn't a kittens-crew-only advantage. We only make a habit of
reaching for it.

### honest bottom line

On the efficiency metrics these benchmarks measure (LOC, tokens, regressions),
**kittens-crew does not beat baseline or the simpler skills — not even on a bench
built to favour it.** Its overhead is real and measurable; its claimed payoff
doesn't show up at this scale under a strong model with visible tests. If the kit
has value, it's the human-facing one — a durable, readable spec; explicit
invariants; an audit trail — not token efficiency. We publish this rather than
keep searching for a framing that wins.

## honest expectations

This is designed to favour the spec pipeline, the way ponytail's bench is designed
to favour brevity. That's fair as long as it's disclosed: we're measuring our
axis. If kittens-crew *doesn't* cut regressions or re-discovery tokens here, that's
a real finding and it gets published as-is. n is small by default — raise it before
drawing hard conclusions.

## files

```
target/        the cart module + bun:test suite (the gradeable substrate)
tasks.json     the 4 dependent tasks, in order
config.json    arms (rtk:true only on kittens-crew)
seq-run.ts     isolated per-arm sequential runner + bun-test grading
```
