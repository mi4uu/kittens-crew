---
name: kittenscrew
description: |
  Use in any project that has a `.kittenscrew/spec.toml` store or a SPEC.md (the
  kittens-crew control plane). Reach for it when deciding what to build next,
  reading or changing the spec, marking a task done, or verifying delivery before
  a commit. Triggers: "what's next", "plan", "add a task / invariant", "amend §V",
  "mark T12 done", "check the spec", "score", "did I actually finish this". The
  binary `kittenscrew` does the deterministic work — this skill only routes you to
  the right verb; it is NOT an instruction dump.
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
