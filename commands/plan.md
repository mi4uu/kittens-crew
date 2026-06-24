---
description: What to build next — worth-ranked DAG plan from the spec store.
argument-hint: "[next | ready | worth | impact <id> | done <id>]"
---
Run `kittenscrew plan ${ARGUMENTS:-next}` and act on its JSON.

- `next` — single highest-worth ready task (deps satisfied).
- `ready` — the parallelizable batch.
- `worth` — full value-ranked list.
- `impact <id>` / `done <id>` — what a task unblocks / mark it complete (re-renders SPEC.md).

The hook membrane already injects `plan next` each turn; call this to query deeper.
