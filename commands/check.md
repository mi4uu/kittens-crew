---
description: Use before committing to verify delivery against the kittenscrew spec. Trigger on "did I actually finish this", "check for fake/stub delivery", "score conformance", "value variance". Do NOT use as a general test runner or in a repo with no kittenscrew store.
argument-hint: "[done | variance | score]"
---
Deterministic quality gates. Run before committing.

- `kittenscrew check done` — scan done tasks for fake delivery (`todo!`/`unimplemented!`/mock/stub/placeholder + broken cites) → demote `x`→`~`.
- `kittenscrew check variance` — delivered vs expected value per done task.
- `kittenscrew score` — graded conformance % (interface, check-done, dep/value coverage, sync). A convergence metric, not pass/fail.
