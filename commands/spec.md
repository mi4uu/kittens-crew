---
description: Create / amend / backprop SPEC.md. Sole mutator of the spec. The Keeper.
argument-hint: "[<idea> | from-code | amend §X | bug: <desc>]"
---

Invoke the **spec** skill with `$ARGUMENTS`.

Read `FORMAT.md` at repo root first if not loaded. Caveman encoding on every
write. While drafting §T tasks, climb the ladder (invoke the **ladder** skill):
a speculative "for later" task is born `∅`, not built. `bug:` args route to the
**backprop** skill. Show the full file / diff and wait for user OK before
applying. Main thread writes — always; the only fan-out is read-only scouts when
DISTILL walks a repo too big for one pass (see CAST.md SCOUTS).
