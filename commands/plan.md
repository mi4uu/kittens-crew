---
description: Use when deciding what to build next from the kittenscrew spec store. Trigger on "what's next", "plan the work", "what's ready", "what does T12 unblock", "mark T12 done". Do NOT use to write a plan from scratch with no spec store.
argument-hint: "[next | ready | worth | impact <id> | done <id>]"
---
Run `kittenscrew plan ${ARGUMENTS:-next}` and act on its JSON.

- `next` — single highest-worth ready task (deps satisfied).
- `ready` — the parallelizable batch.
- `worth` — full value-ranked list.
- `impact <id>` / `done <id>` — what a task unblocks / mark it complete (re-renders SPEC.md).

The hook membrane already injects `plan next` each turn; call this to query deeper.
