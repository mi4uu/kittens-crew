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
