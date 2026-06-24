---
name: backprop
description: |
  Bug → spec protocol. When a bug is found or a test fails, trace the root cause,
  decide whether a new §V invariant would catch the whole class, append to §B.
  The one thing plain plan-then-execute can't do: fix the code AND edit the spec
  so recurrence is impossible. Triggers on test failure at build verification,
  bug report, post-mortem, or explicit ask.
---

# backprop — bug → spec

🧠 **Memory Kitty** speaks here — quiet, never forgets, faintly ominous ("seen
this shape before"). Prefix the line when you take the stage; one prefix (CAST.md).

Plan-then-execute fixes the code & forgets.
kittens-crew fixes the code AND edits the spec so recurrence is impossible.
That edit is backprop.

## WHEN

- Test failed at build verification.
- User reports a bug.
- Post-mortem after an incident.
- check flagged VIOLATE with root cause found.

## SIX STEPS

### 1. TRACE
Read failure output / bug report. Find the exact file:line. **Grep every caller
of the touched function** — the report names one symptom path; the root cause
usually sits in a shared function every caller routes through. Name the root
cause in one caveman sentence.

### 2. ANALYZE
- Would a new §V invariant catch this class of bug? (most common: yes)
- Is §I wrong — did the spec claim a shape the code cannot deliver? (sometimes)
- Is §T wrong — did we build the wrong thing? (rare but real)

### 3. PROPOSE
Draft the spec change. Never skip §B; §V/§I/§T are case-by-case.
```
§B row: B<next>|<date>|<root cause>|V<N>
§V line: V<next>: <testable rule that would have caught it>
```
Example:
```
§B row: B3|2026-04-20|refund job ran twice on retry|V7
§V line: V7: ∀ refund → idempotency key check before charge reversal
```

### 4. GENERATE TEST
A new invariant without a test is a lie. Add the failing test first. Name it for
the invariant: `TestV7_RefundIdempotent`.

### 5. VERIFY
Fix code **at the shared root, not per caller** (lazy fix = root fix: one guard,
not N). Run the test — must pass. Run full suite — must not regress.

### 6. LOG
Commit spec edit + test + code fix together.
Commit msg: `backprop §B.<n> + §V.<N>: <one-line cause>`.

## GOOD INVARIANT

- Testable in code (grep-able or assert-able).
- Scoped to a behavior, not a file.
- Stated positively when possible (`! hold` over `⊥ forbid`).
- References the §I surface where it applies.

**Bad**: V8: code should be correct.
**Good**: V8: ∀ pg_query ! params via driver, ⊥ string concat.

## WHEN NOT TO ADD §V

- Purely mechanical typo with no class (`i++` vs `i--` in throwaway).
- One-time migration.
- Root cause is an external dep (upgrade it, note in §C).

Still append the §B entry — record that this failure mode was considered. A
future bug with the same smell → §B search shows precedent.

## OUTPUT

Every run: §B entry (always), §V entry (usually), test file (when §V added),
code fix, one commit. No dashboards. SPEC.md + git is the full history.
