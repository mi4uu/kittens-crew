# SPEC

## §G GOAL

`kittenscrew` — Rust CLI. Wraps squeez hooks (fork: `mi4uu/squeez`) w/ own
hooks. Adds spec/plan management, kitty:says() visual wrapper, per-project
config. Deterministic, fast, agent calls it via commands — never writes
SPEC.md directly.
Division of labor: squeez owns compression/dedup/queue/session-memory/
token-tracking → kittenscrew NEVER reimplements those, only wraps. kittenscrew
owns the gap squeez leaves: SPEC/plan/task mgmt, per-project config,
kitty voice, per-task docs. Offload to deterministic Rust what doesn't need
an LLM; leave judgement (intent, prose) to the LLM.
North star: ⊥ just fewer tokens — move work DOWN the model-size ladder w/o quality loss. Offloading deterministic work (plan/validate/diff/scan) to Rust frees a SMALL model's scarce reasoning for genuine judgement → small models become viable. Front-loaded context (the ~50k-on-`hi` bloat) cut via on-demand structured retrieval + caveman + squeez → working room for small models. Safe b/c deterministic gates (spec apply §V-validate, check done, value-variance, drift) = a hard quality FLOOR that catches a small model's slips. Prove via `benchmarks/agency`: {model tier} × {kittenscrew on|off} → quality + tokens + $.

## §C CONSTRAINTS

- Rust ≥ 1.75, single binary, ≤ 2MB stripped, 0 runtime deps except `squeez` binary.
- AGENT-AGNOSTIC: core = CLI contract (stdin/stdout JSON + exit codes V1) callable by ANY agent (Claude Code, Pi, …) — most share the same skills/commands. ⊥ hardcode any single agent's tooling (no CC `Agent`/`Workflow` baked into core). Hook-layer = the ONLY CC-specific adapter, isolated behind `kittenscrew hook <event>`; other agents wire their own trigger → identical deterministic core. Skills/commands authored portable (FORMAT.md/SKILL.md read by all).
- kittenscrew = self-contained cargo workspace (own `[workspace]` in its Cargo.toml) → ⊥ inherit external workspace root (repo isn't a Rust project).
- squeez = binary on PATH (`$HOME/.claude/squeez/bin/squeez` or `which squeez`). Never link library — squeez publishes NO crate/lib API, binary only. Fork `mi4uu/squeez` = backup mirror, ⊥ source of an extracted lib.
- squeez config = global INI `~/.claude/squeez/config.ini`. kittenscrew.toml = per-project, additive, ⊥ overlaps/overrides squeez's INI.
- ⊥ reimplement any squeez feature (compression, dedup, redundancy-queue, session-memory, token-track, nudges). Wrap only.
- `.kittenscrew/spec.toml` = SOLE source of truth (structured, deterministic, `schema=1`). SPEC.md = rendered projection for agents/cavekit compat, regenerated on every mutating cmd. `.kittenscrew/*.json` = ephemeral checkpoint only.
- `kittenscrew.toml` (per-project config) ≠ `.kittenscrew/spec.toml` (plan store) — distinct files, distinct purpose.
- Plan = DAG (task = node, dep = edge). Linear order ⊥ stored — derived via topo-sort on demand. Task may have many deps + many dependents → graph, not tree.
- SPEC.md format = existing `FORMAT.md` (pipe-table, caveman). Backward compat.
- `spec import` ! tolerate real-world variance (FORMAT.md is aspirational, real specs diverge): aligned/code-fenced §T tables, `desc` vs `task` header, unescaped `|`/`||` in task prose (anchor id+status front, cites back), 3 invariant styles (`Vn:` | `Vn \| …` | `- Vn slug:`), continuation sections (`## §V … cont.`) accumulate, non-standard `## §U …` ignored. Validated vs 5 real specs (botm/brainmd/codeHarness/maxijinja/opengraphene).
- Per-project config = `kittenscrew.toml` (TOML). Optional. Defaults if absent.
- Hooks = bash shims → delegate to `kittenscrew` → delegates to `squeez`. Never bypass squeez for compression.
- Wrap ALL 6 squeez hooks: SessionStart, PreToolUse, PostToolUse, SubagentStop, PreCompact, PostCompact.
- `claudeoneprovider.sh` / `claudeopenrouter.sh` keep working — add `PATH` line only.
- skills/commands = MINIMAL routing (frontmatter + 2-3 lines "when to use" → point at `kittenscrew <cmd>`), ⊥ instruction dumps. how-to lives in the binary (`--help`) + spec (`spec read`, on-demand), ⊥ front-loaded prose. Goal: help the LLM CHOOSE, ⊥ pre-load every instruction.
- persona voice = PROGRAM output (`kittenscrew kitty says` → emoji+name+role), ⊥ context decoration. Often omittable — the caller usually knows who's acting. A graphical nicety on output, never a context payload.
- old verbose skills/commands/`AGENTS.md`/`CAST.md` = kept under `archive/` as reference template (what we wanted), ⊥ loaded. Current state ≠ archive; archive is history.

## §I INTERFACES

- cmd: `kittenscrew --version` → `kittenscrew 0.1.0`
- cmd: `kittenscrew kitty says <kitty> <message>` → stdout: `😽📐 [Planning Kitty] message`
- cmd: `kittenscrew kitty list` → stdout: pipe-table of {id, emoji, name, role}
- cmd: `kittenscrew spec read [§<S>] [--plain]` → stdout: section/whole; `--plain` expands caveman symbols → English (legend baked in, ⊥ need FORMAT.md in context)
- cmd: `kittenscrew spec apply` → stdin = STRUCTURED JSON diff `{section:"§T", op:"add|edit|kill|done", payload:{…}}` → exits 0 + updated SPEC.md, exits 2 + message on §V violation. ⊥ parses freeform prose intent — caller (LLM) supplies structure; kittenscrew only validates+writes+orders.
- cmd: `kittenscrew docs task <id>` → write `docs/<id>-<slug>.md` describing what+why for done task. OFF by default (`[docs] auto_generate=false`). Detail level from `[docs] detail` (terse|normal|explain).
- cmd: `kittenscrew spec check` → stdout: list of {id, status, cites} for §V
- cmd: `kittenscrew plan resolve` → stdout: JSON `{tasks:[{id,deps,order}], roots:[…]}` (topo-sort of DAG)
- cmd: `kittenscrew plan ready` → stdout: JSON `[…]` ALL unblocked `.` tasks (deps all `x`), sorted by priority → the parallelizable batch
- cmd: `kittenscrew plan next` → stdout: single next §T.id (ready, lowest priority then id)
- cmd: `kittenscrew plan blocking <id>` → stdout: tasks blocked by `<id>` (reverse-dep query)
- cmd: `kittenscrew check done` → stdout: per `x` task {id, ok|fail, markers, broken_cites}; demotes failed `x`→`~` (V18,V19)
- cmd: `kittenscrew check variance` → stdout: per eval'd done task {id, expected, delivered, variance, direction, flagged}; `[audit] on_variance=halt` → exit 2 (V25)
- cmd: `kittenscrew spec render` → regenerate SPEC.md from `.kittenscrew/spec.toml`
- cmd: `kittenscrew spec import` → parse SPEC.md → `.kittenscrew/spec.toml` (bootstrap / re-sync after manual edit)
- cmd: `kittenscrew spec drift [--apply]` → diff edited SPEC.md vs store projection → report {task_added,removed,changed, prose_changed}; `--apply` reconciles structural + carries toml-only fields + re-renders (V16)
- cmd: `kittenscrew plan graph` → ASCII DAG render of tasks+deps (someday/optional, presentation-only)
- file: `.kittenscrew/spec.toml` → authoritative structured store (`schema=1`): `[goal]`,`[constraints]`,`[[interface]]`,`[[invariant]]`,`[[task]]` (id,status,deps,priority,cites,scope,note),`[[bug]]`
- cmd: `kittenscrew plan done <id>` → flips status → Done in store, re-renders SPEC.md projection
- cmd: `kittenscrew plan impact <id>` → JSON {scope, unblocks[], blocks[]}
- cmd: `kittenscrew plan path [<goal>]` → JSON {path[], length} critical/longest prereq chain
- cmd: `kittenscrew plan alternatives` → JSON [{id, task, scope, unblocks, blocks}] frontier choices
- cmd: `kittenscrew hook <event>` → runs hook logic for SessionStart|UserPromptSubmit|PreToolUse|PostToolUse|PreCompact; `user-prompt` classifies the prompt (maps-§T|clear|ambiguous) + injects targeted context (plan next + referenced task) as `additionalContext` (V33,V35)
- cmd: `kittenscrew score` → JSON graded conformance: {overall, dims:[{name, pct, detail}]} (interface-completeness, check-done-pass, dep-coverage, value-coverage, sync) — convergence metric, ⊥ binary
- cmd: `kittenscrew config show` → resolved `kittenscrew.toml` (defaults if absent) → JSON
- cmd: `kittenscrew init` → writes `kittenscrew.toml` template, registers hooks in `~/.claude/settings.json`
- cmd: `kittenscrew compression policy` → class→squeez-level map as JSON; `compression level <class>` → level for prose|dump|structured|diff (exit 2 if unknown)
- file: `kittenscrew.toml` schema → `[kitty] compression_level`, `[hooks] pre, post, session, compact`, `[docs] auto_generate, detail (terse|normal|explain), target (dev|idiot)`, `[plan] strict_ordering, forward_agg (max|sum|hybrid), discount, portfolio_weight, rank_by (worth|roi|expected)`, `[audit] recheck_every_tasks, recheck_every_iters, variance_threshold, on_variance (report|brainstorm|halt)`, `[guard] blocked_cmds=[…]`, `[compression] prose, dump, structured, diff (→ off|lite|full|ultra)`
- env: `KITTENSCREW_CONFIG` → path to config (default `./kittenscrew.toml`)
- env: `SQUEEZ_BIN` → path to squeez binary (default auto-detect)

## §V INVARIANTS

V1: ∀ kittenscrew cmd → exits 0 on success, 2 on validation fail, 1 on internal error
V2: `kittenscrew hook <event>` ! exits nonzero if squeez missing — log + continue (graceful degrade)
V3: `kittenscrew spec apply` ⊥ writes to SPEC.md if diff violates any §V rule — emits warning + returns diff to caller
V4: `kittenscrew plan resolve` → ∀ §T task ! appears in exactly 1 position in topo-sort
V5: `kittenscrew kitty says` output ! includes kitty emoji + `[Name]` prefix + raw message — no mutation of message
V6: `kittenscrew init` ! registers hooks only after verifying `squeez` reachable — exit 3 if not
V7: hook shims ! invoke `squeez` directly — always via `kittenscrew` (single entry point)
V8: ∀ command output → caveman format w/ symbols (→, ∀, ⊥, ∅, !)
V9: `.kittenscrew/spec.toml` = authoritative store → SPEC.md = deterministic projection, regenerated by `kittenscrew`, ⊥ hand-authored as truth
V10: kittenscrew ⊥ implements compression/dedup/session-memory → ∀ such work delegates to squeez binary
V11: `kittenscrew spec apply` accepts STRUCTURED diff only → ⊥ infers intent from prose; malformed JSON → exit 2
V12: `kittenscrew docs task` ⊥ runs unless `[docs] auto_generate=true` → default silent
V13: plan = DAG; linear order ⊥ stored → topo-sort computed on demand (insert/edit deps → recompute O(V+E), no renumber)
V14: dep edit/insert creating cycle → reject exit 2 + report cycle path
V15: priority = tiebreak among READY (unblocked) tasks only → ⊥ overrides deps
V16: agent edits SPEC.md directly → drift; next hook diffs SPEC.md vs projection → structured change auto-reconciled into store, ambiguous prose → escalate to LLM w/ structured summary
V17: `plan ready` → ALL unblocked `.` tasks (parallel batch); tasks w/ no dep-path between them (same antichain) → MAY run concurrently
V18: `check done` (cyclic eval) → ∀ `x` task: scan `scope` for fake-delivery (`TODO|FIXME|stub|mock|placeholder|todo!()|unimplemented!()`) + cited §V intact → fail → demote `x`→`~` + report
V19: `x` task = sealed → change flipping its `check done` green→red = regression alarm, ⊥ silent
V20: ∀ path/impact/alternatives query → deterministic (same store → same result), O(V+E) graph walk, ⊥ LLM. Reports scope delivered + edges unblocked/blocked per choice
V21: `spec read --plain` expands ONLY unambiguous unicode symbols (→ ∴ ∀ ∃ ⊥ ∅ ≠ ∈ ∉ ≤ ≥) → lossless; ASCII overloads (! ? & |) + table delimiters untouched (collide w/ prose/code). ⊥ store expanded form — derive on demand (single source of truth, V9)
V22: `plan next`/`alternatives` rank by `worth` (value-weighted) → ⊥ pure cheapness/id. zapychacz (value≈0, unblocks=0) → worth≈0 → sinks choćby difficulty=1. low-hanging fruit wins ONLY when worth real
V23: task carries `value`,`difficulty`,`risk` (1-5, authored @ creation) + `[task.eval]` `satisfaction`,`conformance`,`tokens` (@ done). ~free — AI already judges @ plan + self-evals @ done → capture the signal, ⊥ recompute
V24: `worth = value + γ·forward`; `forward = max(worth children) + portfolio_weight·Σ(worth children)` (hybrid, `[plan]` config); `rank = ROI·(1−risk/6)`, `ROI = worth/difficulty`. deterministic O(V+E), ⊥ LLM @ query (V20)
V25: done-eval = 2 loops → `check done` (delivered: fake-delivery scan, V18) + `value-variance` (worth: eval `satisfaction·conformance` vs authored `value` → magnitude+direction+why). cadence per `[audit]` (every N tasks|iters)
V26: deliberation = composable pipe of fixed-size primitives {`brainstorm`,`research`,`evaluate`,`ask`}, configured per project (e.g. `brainstorm|research|brainstorm|ask`). each primitive = bounded brick w/ sane default (brainstorm ≈ 3 agents × 5 turns, config-overridable) → scale by COMPOSITION (`brainstorm|brainstorm` = 2 rounds), ⊥ by raising caps. pipe length = transparent total cost; ⊥ unbounded gadanie
V27: deliberation ! terminates in `ask` = user-choice decision packet {agreed, options[{proposal,cost,risk}], recommend} → ⊥ auto-apply. Rust = orchestrate/bound/scribe/present, LLM kitties = judgement (debate/attack/defend); roster+chair per config
V28: ∀ §I-declared `cmd:` ! resolve to a real binary subcommand (interface-completeness gate) → deterministic test asserts §I cmds ⊆ clap subcommand tree; built-but-undeclared also flagged. catches §I↔code drift that `check`/`drift` can't (they watch store↔SPEC.md, ⊥ §I↔binary). Lesson forged: silent interface debt → hard floor
V29: remote review = OPTIONAL advisory primitive (config roster of remote agents via OpenRouter: {key, model, role-desc}) → gets diff + ONLY the relevant spec fragment (⊥ whole spec) → returns few-sentence notes/suggestions → ⊥ blocks, ⊥ auto-applies; feeds deliberation (eval|brainstorm|ask). 0-runtime-dep (§C) → shell to `curl`, ⊥ link HTTP crate. absent config → silently skipped
V30: render-triggering cmd (`spec apply`/`plan done`/`check done` demote) ! verify SPEC.md ≡ store projection FIRST (`is_synced`) → diverges (manual prose edit pending) → abort exit 2 + suggest `spec drift --apply`, ⊥ silently clobber the edit
V31: conformance = GRADED %, ⊥ binary pass/fail. score dims (§I-completeness, `check done` pass-rate, dep-coverage, sync, invariant-test coverage) → 0-100% each + aggregate. weird spec≠code case → lock w/ specific + generic unit test AND it dents the % until fixed → track convergence over commits, ⊥ expect all-at-once
V32: compression level chosen per content-class by MEASURED net gain, ⊥ uniform ultra. `net = tokens_saved − P(fidelity_loss)·rerun_cost`. structured/actionable (JSON, build/test errors, diffs, paths, numbers) → lossless floor (savings small, loss forces re-run = net negative); prose/large-reads/dumps → aggressive (savings high, loss ≈ 0). measure on labeled corpus, ⊥ assume. NB: real token weight = front-loaded context (cached system prompt), ⊥ tool output — compress the right cost center
V33: kittenscrew = CONTROL PLANE. ∀ Claude Code event (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop, SubagentStop, Pre/PostCompact) → routes through `kittenscrew hook <event>` → nothing in/out happens without passing the rules. hooks = the power (only layer that intercepts), engine = the brain, ⊥ advice-only
V34: `Stop` hook = autonomous driver (plays the user): turn-end → `check done` on touched scope → `plan done` | demote, audit cadence (`check variance`/`drift` every N), then DECIDE: next task → inject "do X" + block-stop (drive on) | plan empty → summarize | ambiguous/flagged/plan-choice → ESCALATE to real user (the `ask` primitive, V27). bounded: hard cap on auto-iterations, ⊥ runaway
V35: `UserPromptSubmit` hook = intake → classify command {clear | ambiguous | maps-to-§T}, inject ONLY targeted context (`spec read` relevant section + `plan next`), ⊥ front-load. ambiguous → signal agent to clarify before acting (⊥ guess)

## §T TASKS

id|status|task|deps|cites
T1|x|scaffold `kittenscrew/` cargo crate w/ clap CLI|-|§I
T2|x|impl `kittenscrew --version` & `kittenscrew kitty list` (static data)|T1|§I
T3|x|impl `kittenscrew kitty says` (parse kitty id → emoji + name → prefix output)|T1|V5
T4|x|write hook shims (`session-start.sh`, `pretooluse.sh`, `posttooluse.sh`, `precompact.sh`) → delegate to `kittenscrew hook <event>`|T1|V7
T5|x|impl `kittenscrew hook session-start` → `squeez init` + verify install + emit `kitty says "system ready"`|T4|V2,V6
T6|x|impl `kittenscrew hook pre-tool` → kittenscrew checks first (blocked cmds) → delegate to `squeez` pretooluse.sh|T4|V7
T7|x|impl `kittenscrew hook post-tool` → delegate to `squeez` posttooluse.sh|T4|V7
T8|x|impl `kittenscrew hook pre-compact` → `squeez` precompact.sh + checkpoint plan to `.kittenscrew/plan.json`|T4|V7
T9|x|impl `kittenscrew spec read` → render section/whole from store|T25|§I
T10|x|impl `kittenscrew spec apply` → accept diff, validate vs §V rules, write SPEC.md or exit 2|T25,T27|V3
T11|x|impl `kittenscrew spec check` → structural: deps/cites resolve, ids unique, cycle DFS|T25|§I,V14
T12|∅|impl `kittenscrew plan resolve` → parse §T table, build DAG, topo-sort|-|§I,V4   (superseded by T28 (plan resolve/topo-sort)
T13|∅|impl `kittenscrew plan next` → filter `.` tasks w/ all deps `x`, return lowest id|-|§I   (superseded by T28 (plan next)
T14|∅|impl `kittenscrew plan done <id>` → flip `.`→`x` in §T row, validate id exists|-|§I   (superseded by plan done cmd)
T15|x|impl `kittenscrew.toml` parser + defaults (compression_level, hooks list, docs.auto_generate)|T1|§I
T16|x|impl `kittenscrew init` → write `kittenscrew.toml` template + register hooks in `~/.claude/settings.json`|T15|V6
T17|x|add `kittenscrew` to PATH in `claudeoneprovider.sh` & `claudeopenrouter.sh`|T1|§C
T18|x|write `kittenscrew/tests/` integration tests (1 per §I command, assert exit codes per V1)|T28|V1
T19|.|write README.md section: install, hook wiring, `kittenscrew.toml` schema, command reference|T16|§I
T20|∅|custom config format (YAML/JSON)|-|-   (ladder: TOML stdlib, no value in own format)
T21|.|wrap `kittenscrew hook subagent-stop` → delegate squeez SubagentStop|T4|§C,V7
T22|.|wrap `kittenscrew hook post-compact` → delegate squeez PostCompact + restore plan checkpoint|T4|§C,V7,V9
T23|x|impl `kittenscrew docs task <id>` → write `docs/<id>-<slug>.md`, gated on `[docs] auto_generate`|T25|V12
T24|.|impl `[guard] blocked_cmds` in `hook pre-tool` → exit 2 if tool cmd matches blocklist|T15,T6|V11,§I
T25|x|impl `.kittenscrew/spec.toml` store (toml crate) — tasks/deps/priority/scope/cites/invariants/bugs + opaque prose for §G/§C/§I|T1|§C,V9
T26|∅|Rust NLP to parse agent prose intent into spec diff|-|-   (ladder: Rust DETECTS+classifies diff (T29); semantic intent = LLM, not Rust)
T27|x|render SPEC.md from spec.toml (caveman pipe-table, FORMAT.md) on every mutating cmd|T25|V9
T28|x|topo-sort (Kahn) + `plan ready`/`next`/`blocking`/`resolve` over DAG; cycle detect|T25|V13,V14,V15,V17
T29|x|drift hook: diff SPEC.md vs projection → reconcile structured changes into store, escalate ambiguous prose|T27|V16
T30|x|`check done`: scan task `scope` for fake-delivery markers + verify cited §V intact → demote failed `x`→`~`|T31|V18,V19
T31|x|`scope` field per task (globs) → defines what `check done` scans; port fake-delivery scanner from agency|T25|V18
T32|x|`kittenscrew plan graph` → ASCII DAG render (someday, optional, presentation-only; `ascii-dag` crate candidate). Consumes store, zero coupling — deferrable. priority=low|T28|§I,V13
T33|x|`kittenscrew spec import` → parse SPEC.md (old 4-col + new 5-col §T) → spec.toml; killed-note round-trip|T25|§C,V9
T34|x|`kittenscrew plan path [<goal>]` → critical path (longest prereq chain) via DAG DP|T28|§I,V13,V20
T35|x|`kittenscrew plan impact <id>` → scope + newly-ready (unblocks) + transitive dependents (blocks)|T28|§I,V13,V20
T36|x|`kittenscrew plan alternatives` → frontier choices each w/ {scope, unblocks, blocks}, ranked by leverage|T28|§I,V20
T37|x|`spec read --plain` → deterministic symbol→English expand (FORMAT.md legend baked in); ⊥ stored, derived on demand|T27|V21
T38|∅|store `text_unrolled` field (expanded copy in toml)|-|-   (ladder: dual-store = drift, V9; expand is pure fn — derive via --plain not persist)
T39|x|extend Task schema: `value`/`difficulty`/`risk` (1-5, @creation) + `[task.eval]` `satisfaction`/`conformance`/`tokens` (@done); serde defaults → backward-compat|T28|V23
T40|x|compute `worth`/`ROI` (V24 formula) + re-rank `plan next`/`alternatives` by worth → ⊥ leverage/id; tiebreak priority|T39|V22,V24
T41|x|`[plan]` config: forward_agg(max\|sum\|hybrid), discount γ, portfolio_weight, rank_by(worth\|roi\|expected)|T15,T39|V24
T42|x|`value-variance` audit cmd + `[audit]` cadence (recheck_every_tasks/iters, variance_threshold, on_variance=report\|brainstorm\|halt)|T30,T39|V25
T43|.|deliberation pipeline engine: primitives {brainstorm,research,evaluate,ask}, config-composed pipe, fixed-size bricks (default ~3 agents×5 turns, scale by composition), Rust referee/orchestrator + ANY-agent LLM roster (Claude/Pi/…, ⊥ CC-specific), ask=user-choice exit packet|T41,T42|V26,V27
T44|.|`kittenscrew review` — assemble diff + ONLY relevant spec fragment + role prompt → call config'd remote agent(s) (OpenRouter via curl, 0-dep) → collect few-sentence notes/suggestions → feed deliberation (eval\|brainstorm\|ask). optional, advisory, absent-config=skip|T41|V26,V27,V29
T45|x|interface-completeness gate: test §I declared cmds ⊆ binary clap subcommand tree (forge §I↔code drift lesson into deterministic floor)|T48|V28
T46|x|persist toml-only fields (value/difficulty/risk/priority/scope/eval) across SPEC.md round-trip — decide: commit store \| render into SPEC.md \| sidecar. currently LOST on reimport (gitignored store + SPEC.md ⊥ carries them), silently|T33|V9,V23
T47|x|render-triggering cmds (spec apply, plan done, check done demote) detect SPEC.md drift vs store FIRST → abort + suggest `spec drift --apply` (prevent silent clobber of manual prose §G/§C/§I/§V edits). Discovered live: hand-edit §I then apply rendered stale store, dropped the edit|T29|V9,V16
T48|x|`kittenscrew score` — GRADED conformance % (V31): dims §I-completeness, check-done pass-rate, dep-coverage, sync, invariant-test-coverage → 0-100 each + aggregate. deterministic. track convergence per commit, ⊥ binary|T28,T30|V31
T49|x|`[compression]` config: per content-class level (prose\|dump\|structured\|diff → off\|full\|ultra). kittenscrew owns the POLICY, squeez does the work (wrap, ⊥ reimpl)|T15|V32,V10
T50|.|compression measurement harness: labeled corpus × squeez levels → per-class {tokens_saved, fidelity (lossless on numbers/paths/errors/JSON), net = saved − P(loss)·rerun_cost} → recommended policy. deterministic, graded like score|T48,T49|V32
T51|x|`UserPromptSubmit` hook → `kittenscrew hook user-prompt`: classify command {clear\|ambiguous\|maps-§T}, inject ONLY targeted context (spec read relevant + plan next), ⊥ front-load. ambiguous → agent clarifies before acting|T16|V35,V33
T52|.|`Stop` hook = autonomous driver → `kittenscrew hook stop`: turn-end → check done on touched scope → plan done\|demote, audit cadence (variance/drift every N), decide next→inject+block-stop \| empty→summarize \| ambiguous/flagged→escalate to user. bounded auto-iters, ⊥ runaway|T16,T42,T51|V34,V33,V27
T53|.|hook dispatch covers ALL CC events (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop, SubagentStop, Pre/PostCompact) → single kittenscrew entry = the membrane; nothing bypasses (V33). wire via init (T16)|T16|V33

## §B BUGS

id|date|cause|fix
B1|2026-06-24|crate inherited `version.workspace`/`edition.workspace` but no `[workspace]` root existed after repo move → `cargo build` failed "failed to find a workspace root"; T1-T8 marked `x` but didn't compile|§C self-contained-workspace constraint + literal versions in Cargo.toml
