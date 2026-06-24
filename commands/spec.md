---
description: Use when reading or changing the kittenscrew spec. Trigger on "read the spec", "add a task/invariant", "amend §V", "edit §G", "check for drift". Do NOT use to hand-edit SPEC.md directly or to author specs in a repo with no kittenscrew store.
argument-hint: "[read §X | apply | check | drift]"
---
The spec store (`.kittenscrew/spec.toml`) is authoritative; SPEC.md is its projection.

- READ: `kittenscrew spec read [§X] [--plain]` — never `cat SPEC.md`, read through this.
- MUTATE: pipe a STRUCTURED JSON diff to `kittenscrew spec apply`
  (`{section,op:add|edit|kill|done,payload}`). It §V-validates and writes; a violation → exit 2, nothing written. Don't hand-edit SPEC.md then apply (the sync guard rejects it) — edit prose → `kittenscrew spec import` → `spec render` first.
- VERIFY: `kittenscrew spec check`. A drifted SPEC.md → `kittenscrew spec drift --apply`.
