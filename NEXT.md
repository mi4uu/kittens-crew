# kittens-crew — what's next

State: spec/plan/eval engine + **control plane COMPLETE** on `main`
(merged via PR #2, commit `78e129c`). 53 tasks, 35 invariants, conformance
score 100%, 100 tests (90 unit + 10 e2e), clippy clean. Store committed at
`.kittenscrew/spec.toml` (authoritative); SPEC.md = projection.

## The engine (built, working)
Write-loop: `spec apply/drift/import/render/read/check`. Plan: `plan
next/ready/resolve/blocking/impact/path/alternatives/worth/graph/done`
(worth-ranked). Eval: `check done` (fake-delivery) + `check variance`. `score`
(graded conformance %). `config show`, `compression policy|level`, `kitty
says/list`, `docs task`.

## The control plane (built this session — the "drive wheel")
- **T16 `init`** — writes `kittenscrew.toml` + registers the 8-event hook
  membrane in `<target>/settings.json`. V6 squeez-gated (exit 3). Safe:
  `--target`/`--dry-run`, idempotent merge preserves user hooks.
- **T49 compression policy** — `[compression]` per content-class → squeez level
  (V32 floor). `compression policy|level <class>`. Policy is kittenscrew's,
  work is squeez's (V10).
- **T51 UserPromptSubmit intake** — classify {maps-§T|clear|ambiguous} + inject
  ONLY targeted context (plan next + referenced task) as additionalContext.
- **T52 Stop driver** — autonomous turn-end: check-done demote → audit variance
  → drive-on (block-stop+inject) | halt | escalate. **Default OFF** (`[driver]
  autonomous=false`), hard-bounded by `max_iters`, escalates on flagged
  variance. `.kittenscrew/driver.json` holds the iter counter; reset on a real
  user turn.
- **T53 full membrane** — all 8 CC events (SessionStart, UserPromptSubmit,
  PreToolUse, PostToolUse, Stop, SubagentStop, Pre/PostCompact) route through
  `kittenscrew hook` (V33). `init::MEMBRANE` is the single wiring point.

## Next (engine's `plan next` = T19, but weigh strategically)
The worth model ranks by ROI, so it surfaces cheap-but-peripheral tasks (T19
README) over expensive keystones. KNOWN GAP: `rank_by=expected` under-weights
high-value/high-difficulty strategic work (it ranked the whole control plane
below README). When the engine and the session's north-star diverge, understand
why, then decide — don't blindly follow ROI. Ready frontier now:
- **T19** README (install, hook wiring, schema, command reference) — user said
  README is low-priority ("only name+logo current"), so this is genuinely
  deferrable despite the engine surfacing it.
- **T50** compression measurement harness — the bake-off (labeled corpus ×
  squeez levels → per-class net gain → recommended policy). Validates T49's
  defaults empirically. Deps T48+T49 done → ready. This is the high-value
  measurement work (north-star: prove gains, don't assume).
- T21/T22/T24/T43/T44 — smaller surface tasks.

Recommended: **T50** (measure compression for real) or the **benchmark** (below)
— both serve the prove-it-don't-assume north star better than README.

## Open research (MEASURE, don't adopt on vendor claims)
- squeez reality: spec treats `mi4uu/squeez` as a fork/backup mirror (§G/§C);
  compression levels off|lite|full|ultra align with squeez personas.
- TOON/HEDL bake-off (part of T50): {JSON, TOON, HEDL} × our shapes × Claude
  Haiku × {tokens, retrieval, validation}. HEDL niche = relational
  task-context-injection payload, not the store/flat outputs.
- Killer idea: control plane knows `plan next` → feed it as task-description to
  squeez (post-tool hook) → task-aware pruning, automatic.

## Benchmark (the proof — needs T16+hooks, now BUILT)
Interactive tmux-driven real Claude Code (agency harness). Arms: baseline /
kittens-crew / cavekit (Docker-isolated). `init --target <docker-dir>` makes
kittens-crew a deployable arm. Metrics: plan quality/adherence, asks-vs-assumes,
automation, knows-when-to-return, cleanup, time, tokens. Judges: local Claude +
OpenRouter + Gemini hand-off.

## Discipline (keep)
- prose §-edit → `spec import` → `spec render` BEFORE any apply (sync-guard).
  Each task: build → clippy → test → dogfood on real spec → score → commit.
- **Git: this repo enforces PRs** — direct push to main is rejected. Land
  milestones via `gh pr create --base main` + `gh pr merge --merge` (no review
  ceremony, but the PR is the mechanism). NEVER `git add -A` blindly: secret-
  scanning push protection + local provider scripts. Stage explicit paths.
- Read SPEC via `spec read`/Read tool, ⊥ `cat`.
