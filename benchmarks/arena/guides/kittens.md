# This project ships a skillset — use it, don't work from memory.

kittens-crew is a control plane: a deterministic `kittenscrew` binary holds the
spec/plan, and a hook membrane injects the next task + the right "kitty" role each
turn. The store (`.kittenscrew/spec.toml`) is authoritative; `SPEC.md` is its
rendered projection. Follow what the membrane tells you.

## Your commands
- `/init` — wire kittens-crew into a project that lacks it (writes the store, registers hooks). Run once.
- `/plan` — pick what to build next from the spec: `next` · `ready` · `worth` · `impact <id>` · `done <id>`.
- `/spec` — read or change the spec: `read §X` · `apply` (structured diff, §V-validated) · `check` · `drift`. Never `cat SPEC.md` — use `spec read`.
- `/check` — verify delivery before committing: `done` · `variance` · `score`.

## When + in what order
1. `/init` if the project has no kittenscrew store.
2. `/plan next` — let it tell you the next ready task. No plan → no product code (the gate enforces this).
3. Build that task. Read the spec with `/spec read`, amend with `/spec apply`.
4. `/plan done <id>` when the task is finished — re-renders SPEC.md, tracks progress.
5. `/check done` and `/check score` before you commit.

## Rule
The hook membrane already surfaces `plan next` + the suggested kitty role every
turn — follow it. Before planning or writing code from memory, run `/plan next`
FIRST.
