# SPEC

## §G GOAL
CLI `todo` — add/list/done tasks, persist JSON.

## §C CONSTRAINTS
- lang: TypeScript, runtime bun
- store: single `~/.todo.json`, no DB
- zero network

## §I INTERFACES
- cmd: `todo add <text>` → append task, print new id
- cmd: `todo ls` → stdout table id/text/done
- cmd: `todo done <id>` → mark done
- file: `~/.todo.json` = `[{id,text,done}]`

## §V INVARIANTS
V1: ∀ write → atomic (tmp file + rename)
V2: id monotonic, ⊥ reuse
V3: `done <id>` unknown id → exit 1 + stderr msg

## §T TASKS
id|status|task|cites
T1|x|scaffold bun cli|-
T2|x|impl add + ls|I.cmd
T3|~|impl done + V3 guard|V3,I.cmd
T4|.|atomic write|V1

## §B BUGS
id|date|cause|fix
B1|2026-06-20|partial write on crash → corrupt json|V1
