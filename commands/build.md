---
description: Plan→execute against SPEC.md with the laziness ladder baked in. No sub-agents.
argument-hint: "[§T.n | --next | --all]"
---

Invoke the **build** skill with `$ARGUMENTS`.

Read `SPEC.md` (if missing, defer to the **spec** skill) and `FORMAT.md`. For
each chosen §T task: cite the §V invariants and §I interfaces it touches, then
climb the ladder (invoke the **ladder** skill) before writing a line — YAGNI →
reuse what already exists (DRY) → stdlib → native → installed dep → one line.
A task the ladder kills is marked `∅`, never built. Write the shortest diff that
works; mark deliberate shortcuts with `// kitten:` comments naming the ceiling.
Add one test per touched §V invariant. On test/build failure, invoke the
**backprop** skill — never retry blindly. Commit per task. Main thread only.
