# kittens-crew — benchmark-readiness plan (post-context-clear)

State on `main` (732a6ac): engine + control plane complete, 59 tasks, score 100%,
109 tests. Arena harness LIVE + validated (run #1 done on Haiku). T57 plan-gate +
generic skill-nudge SHIPPED. PRs through #15.

## The critical path for run #2 is ALREADY DONE
The two things that actually change run-#2 behaviour are merged:
- **T57 plan-gate** — PreToolUse denies code writes until a plan store exists;
  UserPromptSubmit injects "no plan → plan first". `[gate] enforce_plan` (on).
- **Skill-nudge** — generic, identical `~/.claude/CLAUDE.md` for skill arms only.

→ **Run #2 can go NOW** on current `main` (sloppy `prompt.txt` brief, Haiku, gate+
nudge live). Everything below is polish/enhancement, NOT a blocker.

## Remaining work — and whether it parallelises
The honest constraint: **main.rs is the bottleneck.** Most behavioural work edits
the hook/dispatch in main.rs, so it serialises (parallel edits = merge conflicts).
Clean fan-out only across DISJOINT file sets:

| Track | Touches | Parallel-safe? | Items |
|-------|---------|----------------|-------|
| **A · refactor/hygiene** | `main.rs`, new `hook.rs`/`squeez.rs` | owns main.rs | extract `hook.rs` + `squeez.rs` (main.rs ~1400 lines); sweep the 14 inline `{emoji} [name]` command outputs → framed `kitty::say()` |
| **B · skill/command polish** | `skills/`, `commands/` | yes (disjoint) | sharpen `skills/kittenscrew/SKILL.md` to Use-when/Trigger-on/Do-NOT format; tighten command `.md` routers. NB fairness: only to OUR good-practice level, not juiced past cavekit/ponytail |
| **C · run #2 benchmark** | `benchmarks/arena/` runtime | yes (disjoint, uses current binary) | run kittens + ≥1 comparison arm on `prompt.txt`; capture cost/context/stories; check kittens now plans-first |
| **D · behavioural (serialises after A)** | `main.rs` hook module | NO (conflicts with A) | Helper kitty actually narrates (a hook emits 🐾 narration); T58 cat-roster glue; T59 cheap cat-voice subagent (CC-specific, speculative) |
| **E · subagent fan-out engine** | `main.rs` + spec | NO (conflicts with A) | DAG ready-frontier → parallel isolated subagents (the "no fanout" rule is lifted; T55 already routes roles) |

**Fan-out verdict:** A, B, C are mutually disjoint → safe to run as 3 concurrent
subagents (A in a worktree since it rewrites main.rs). D + E both want main.rs →
do them AFTER A merges, sequentially (or fold into A's agent). T59/E are the big,
speculative ones — design before building.

## Recommended sequence
1. **Run #2 now** (track C) — proves gate+nudge changed behaviour; cheap, high-signal.
   Can run concurrently with A + B.
2. **Fan out A + B** as subagents (worktree for A) while C runs → 3 parallel reports.
3. Merge A, then do **D** (helper-narrate, T58) on the clean main.rs.
4. **E** (subagent fan-out engine) + **T59** last — biggest, design-first.

## Discipline (unchanged)
prose §-edit → `spec import` → `spec render` before `apply`. Each task: build →
clippy → test → dogfood → score → commit. Repo enforces PRs: `gh pr create` +
`gh pr merge --merge`; NEVER `git add -A` (secret-scanning + local provider
scripts). Read SPEC via `spec read`, ⊥ `cat`.

## Crew (10) + voice
🎩 orchestrating · 📐 planning · 🔨 builder(ladder) · 😼 entropy · 🧠 memory ·
🖋️ scribe(style) · 🐾 helper(narrator) · 🔭 explorer(research) · 🎨 style · 🔥 grill
(adversarial). Voice = role-coloured frame ▌ + sentiment emotion + role emoji +
name + message (`--box` for comic frames). `discipline()` carries retired-skill
rules on-demand.
