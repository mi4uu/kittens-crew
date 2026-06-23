---
description: Harvest every // kitten: shortcut into a debt ledger. One-shot, read-only.
argument-hint: "[path | --bare]"
---

🧠 **Memory Kitty** hosts this — never forgets what was deferred. One-shot
report, writes nothing.

Grep the repo (or `$ARGUMENTS` path) for `kitten:` markers in comments — every
comment syntax: `// kitten:`, `# kitten:`, `/* kitten:`, `-- kitten:`,
`<!-- kitten:`. These are the deliberate shortcuts the Builder left behind; this
command makes sure "later" doesn't rot into "never".

For each hit, pull: file:line, the shortcut, the named ceiling, the upgrade path.
A good kitten comment names both (`global lock, per-account locks if throughput
matters`); a bare one (`// kitten: this exists`) has no ceiling — flag it so it
either earns a ceiling or gets removed.

## REPORT — the ledger

Caveman. Group by whether a ceiling is named. Sort named ones by how close the
upgrade trigger looks.

```
## kitten debt — 4 shortcuts

CEILING NAMED (upgrade path known)
  cache/lock.go:8   global lock → per-account locks if throughput matters
  api/page.go:40    O(n²) scan → index when list > ~1k
  jobs/retry.go:12  cap 3 retries → backoff+jitter if upstream flaps

BARE (no ceiling — fix the comment or cut the shortcut)
  util/date.go:5    "// kitten: this exists"  ← name the ceiling or delete

## summary
3 with upgrade path, 1 bare. cross-check: /kitten:check-changed flags any whose
ceiling the spec already outgrew (CEILING-HIT).
```

`--bare` arg → list only the ceiling-less markers, nothing else.

## NOT THIS

- Write nothing. No code edits, no SPEC.md edits. Report only.
- Don't fix the shortcuts — that's `/kitten:build` on a real §T task.
- No sub-agents. Main thread greps.
