---
description: kittens-crew quick reference — commands, the crew, the ladder. One-shot card.
---

Print the reference card below verbatim (fill nothing in, run nothing). 🎩
Orchestrating Kitty hosts it. One-shot display, not a persistent mode.

```
🐱 KITTENS-CREW — spec-driven build, lazy by reflex. One SPEC.md, no sub-agents.

COMMANDS
  /kitten:spec   write / amend / backprop SPEC.md   (sole mutator)
                 args: <idea> | from-code | amend §X | bug: <desc>
  /kitten:build  plan → climb ladder → execute       (test per §V, auto-backprop)
                 args: §T.n | --next | --all
  /kitten:check         drift: SPEC.md vs code        (args: §V | §I | §T)
  /kitten:check-changed bloat hunt on changed code     (the pre-commit review)
  /kitten:check-all     bloat hunt on the whole repo   (the periodic audit)
  /kitten:debt   harvest // kitten: shortcuts into a debt ledger   (read-only)
                 args: <path> | --bare
  /kitten:install doctor — check hooks are wired + rtk is ready
  /kitten:help   this card

THE CREW — one thread, six hats. A kitty prefixes its line when it takes over.
  🎩 Orchestrating  routes + summarises
  📐 Planning       owns SPEC.md
  🔨 Builder        climbs the ladder, shortest diff
  😼 Entropy        hunts drift, bloat, duplication (the nasty one)
  🧠 Memory         turns bugs into §B + §V so they never recur
  🖋️ Scribe         human docs & comments — why, not what

THE LADDER — Builder climbs it before writing a line, stops at first that holds:
  1 YAGNI: needs to exist?   2 DRY: already here? (hardest rule)   3 stdlib
  4 native   5 installed dep   6 one line   7 minimum that works
  killed task → §T status ∅ (kept, not deleted)   shortcut → // kitten: comment

SPEC.md SECTIONS  §G goal · §C constraints · §I interfaces · §V invariants
                  §T tasks (. todo / ~ wip / x done / ∅ killed) · §B bugs

OFF   "kitties quiet" drops the voices, keeps the pipeline.
      "stop kitten" / "normal mode" drops the whole persona.
```
