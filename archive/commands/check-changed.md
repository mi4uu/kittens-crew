---
description: Read-only BLOAT hunt on changed/added code (the pre-commit review). Writes nothing.
---

Invoke the **check** skill in BLOAT mode, scope = **changed/added code**.

😼 Entropy Kitty reads the diff against the ladder, in reverse: OVER-ENGINEERED
abstractions (one impl, factory for one product, config that never changes),
REINVENTED stdlib/native, DUPLICATED logic (DRY), and `// kitten:` CEILING-HIT
shortcuts the spec outgrew. Report grouped by severity, file:line evidence, one
remedy hint per item — usually "delete it, drop to the simpler rung." Fast; this
is the pre-commit pass. Write nothing. Main thread reads.
