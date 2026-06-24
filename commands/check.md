---
description: Verify delivery — fake-scan, value-variance, conformance score.
argument-hint: "[done | variance | score]"
---
Deterministic quality gates. Run before committing.

- `kittenscrew check done` — scan done tasks for fake delivery (`todo!`/`unimplemented!`/mock/stub/placeholder + broken cites) → demote `x`→`~`.
- `kittenscrew check variance` — delivered vs expected value per done task.
- `kittenscrew score` — graded conformance % (interface, check-done, dep/value coverage, sync). A convergence metric, not pass/fail.
