---
description: Read-only DRIFT report — SPEC.md vs code. Writes nothing.
argument-hint: "[§V | §I | §T]"
---

Invoke the **check** skill in DRIFT mode with `$ARGUMENTS`.

Read `SPEC.md` (if missing, say so and stop). Diff spec against code: classify
each §V invariant HOLD/VIOLATE/UNVERIFIABLE, each §I interface
MATCH/DRIFT/MISSING/EXTRA, each §T task against evidence (flag STALE `x` rows and
REGROWN `∅` rows). No arg → all drift dimensions; `§V`/`§I`/`§T` narrows to one.
Report grouped by severity with file:line evidence and one remedy hint per item.

This command does drift only. For the over-engineering hunt use
`/kitten:check-changed` (diff) or `/kitten:check-all` (whole repo). Write
nothing. Main thread reads.
