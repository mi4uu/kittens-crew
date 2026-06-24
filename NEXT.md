# kittens-crew — what's next (post-compact checkpoint)

State: kittenscrew spec/plan/eval engine COMPLETE on `main` (commit 1c468c7,
pushed). 53 tasks, 35 invariants, conformance score 100%, 81 tests, clippy clean.
Store committed at `.kittenscrew/spec.toml` (authoritative); SPEC.md = projection.

## The engine (built, working)
Write-loop: `spec apply/drift/import/render/read/check`. Plan: `plan
next/ready/resolve/blocking/impact/path/alternatives/worth/graph/done`
(worth-ranked, value/difficulty/risk + γ·forward). Eval loops: `check done`
(fake-delivery) + `check variance` (delivered vs expected). `score` (graded
conformance %). `config show`, `kitty says/list`, `docs task`.

## Next build = the control plane (the "drive wheel"). Engine done, seams missing.
`plan next` = T16. Critical path: T1→T25→T31→T30→T42→T52.
1. **T16 init** (keystone) — register hooks. BUILD SAFE: `--target <dir>` +
   `--dry-run` so it never blindly touches the user's `~/.claude/settings.json`
   (also makes it Docker-isolatable for the benchmark). Verify squeez (V6).
2. **T51 UserPromptSubmit hook** — intake: classify command {clear|ambiguous|
   maps-§T}, inject ONLY targeted context (spec read + plan next), clarify if
   ambiguous (V35). Binary needs `hook user-prompt` dispatch.
3. **T52 Stop hook** = autonomous driver (plays the user): turn-end → check done
   on touched scope → plan done|demote, audit cadence, advance | escalate.
   bounded ⊥ runaway (V34). Binary needs `hook stop` dispatch.
4. **T53** — full event membrane via init (V33).

## Open research (don't adopt on vendor claims — MEASURE via T50)
- **Three compression mechanisms, route per content-class (V32):** squeez =
  LOSSY LLM-prune for verbose UNSTRUCTURED logs (pytest/build/grep); TOON/HEDL =
  lossless for STRUCTURED; caveman = lossless prose; off = code/diffs/errors.
- **squeez reality check:** KRLabsOrg/squeez = task-aware LLM output pruner
  (lossy, ~92%, NO modes/dedup/queue/memory/hooks). Our §G/§C over-attributes —
  RECONCILE, or confirm the running one is a fork (mi4uu/squeez) with added modes.
- **Killer idea:** control-plane knows current task (`plan next`) → feed it as
  task-description to squeez via post-tool hook → task-aware pruning, automatic.
- **TOON/HEDL bake-off (T50):** candidates {JSON baseline, TOON, HEDL}, pluggable
  `--format`. Measure on OUR shapes × Claude HAIKU (north-star) × {tokens,
  retrieval, validation}. Findings so far (vendor, unreproduced): TOON wins on
  weak models (Haiku +2.4) + nested + validation(+10); ~tied on uniform-flat
  (our outputs); HEDL claims beat TOON but tested Mistral/DeepSeek/GLM (NOT
  Claude), no breakdown, no repro. HEDL's real niche = the RELATIONAL
  task-context-injection payload (task+scope+deps+cites+invariant-texts, shared
  refs amortise) — separate content-class to measure, NOT the store (TOML
  adjacency lists are fine) and NOT flat outputs.

## Benchmark (the proof — validates the whole thesis)
Interactive tmux-driven real Claude Code (agency harness, a55786a). Arms:
baseline / kittens-crew / cavekit (Docker-isolated, each sees only its skillset).
NOT single `claude -p` one-shots. Real small projects with decision branches.
Metrics: plan quality, plan-adherence, asks-vs-assumes, automation, knows-when-
to-return, review, cleanup-after-itself, time, tokens. Judges: local Claude +
OpenRouter arbiter + Gemini hand-off (package valuable results, strip target/
/.DS_Store noise). REQUIRES T16+hooks first (kittens-crew must be deployable arm).

## Architecture decisions locked this session
- skills/commands = MINIMAL routing (when-to-use → point at `kittenscrew <cmd>`),
  ⊥ instruction dumps. Voice = PROGRAM output (`kitty says`), ⊥ context prose.
  Old verbose skills/commands/AGENTS.md/CAST.md → `archive/` (reference only).
  README is stale except name+logo. New minimal skills = TODO (deferred).
- SessionStart hook now a ~20-token marker (was `cat AGENTS.md` 892 tok).
- kittenscrew = CONTROL PLANE: every CC event routes through `kittenscrew hook`,
  nothing bypasses. Autonomy bounded + escalates to user on ambiguity/variance.

## Discipline (keep)
- prose §-section edit → `spec import` → `spec render` BEFORE any apply (sync-guard
  rejects otherwise). Each task: build → test → dogfood on real spec → score →
  commit. Read SPEC via `spec read`/Read tool, ⊥ `cat` (squeez compresses output).
- Branch freely, merge straight to main at milestones (no PR ceremony).
