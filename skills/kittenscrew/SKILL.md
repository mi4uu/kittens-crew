---
name: kittenscrew
description: |
  Use when a project has a `.kittenscrew/spec.toml` store or a SPEC.md and you
  need to pick the next task, read/change the spec, mark a task done, or verify
  delivery before a commit. Trigger on "what's next", "plan the work", "add a
  task/invariant", "amend §V", "mark T12 done", "read the spec", "check the
  spec", "score conformance", "did I actually finish this". Routes to the
  `kittenscrew` binary, which does the deterministic work. Do NOT use for casual
  questions, ad-hoc edits to unrelated files, or repos with no kittenscrew store.
---

# kittenscrew — control-plane router

This project's spec, plan, and quality gates live in the `kittenscrew` binary.
Don't reinvent them in prose — call the verb and act on its JSON.

| Need | Call |
|------|------|
| What to build next | `kittenscrew plan next` (also `ready`, `worth`, `impact <id>`) |
| Read the spec | `kittenscrew spec read [§X] [--plain]` — never `cat SPEC.md` |
| Change the spec | pipe a structured diff to `kittenscrew spec apply` (`{section,op,payload}`); §V-validated, exit 2 on violation |
| Finish a task | `kittenscrew plan done <id>` (re-renders SPEC.md) |
| Verify delivery | `kittenscrew check done` · `check variance` · `score` |
| Wire into a project | `kittenscrew init [--target <dir>] [--dry-run]` |

Rules that matter: the store is authoritative (SPEC.md is a projection); mutate
only through `spec apply` (structured diffs, never freeform prose); edit prose in
SPEC.md → `spec import` → `spec render` before any apply (the sync guard rejects a
divergent SPEC.md). The hook membrane already injects `plan next` + the suggested
kitty role each turn — follow it. The kitties speak from the program
(`kittenscrew kitty says <id> <msg>`); you don't narrate their voice yourself.
