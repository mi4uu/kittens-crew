# arena — interactive Claude Code benchmark, in Docker

Compare skillsets by running the **same** Claude Code, interactively, against the
same underspecified brief — and watching how each one plans, asks, builds, and
cleans up. No `claude -p` one-shots: claude runs in a tmux session inside a
disposable container, and **you play the user** — send a prompt, peek the pane,
say little, expect results.

## Why Docker — and clean isolation
Hard isolation, no host pollution, no cross-arm leftovers. **Nothing is installed
on your machine** — every skillset is installed *inside* its container. The base
image is **neutral**: a generic dev toolchain only (rust, bun, uv/python, node,
tokei, ripgrep, git, tmux) + the `claude` CLI. kittenscrew / squeez / cavekit /
ponytail are **not** baked in — each arm installs ITS OWN skillset at provision
time into a fresh mounted `~/.claude`. So:

- **baseline** starts pristine — pure Claude Code, none of our binaries even on
  PATH, `/repo` not mounted.
- **kittens** self-installs from nothing in-container (`install.sh` cargo-builds
  kittenscrew, installs squeez, drops the plugin, wires the membrane) — which also
  exercises the real install path.
- **cavekit** (v4) installs from GitHub in-container: `npx skills add
  JuliusBrussee/cavekit` (markdown skills), git-clone fallback.
- **ponytail** installs from the Claude Code marketplace in-container: `claude
  plugin marketplace add DietrichGebert/ponytail` + `claude plugin install`.

Auth is the SAME token for every arm, supplied via `.env` (env), never written
into a skillset — so each arm's `~/.claude` is a clean config dir with no session
history or other artifacts carried in. cavekit/ponytail clone from GitHub and
never see `/repo` either.

Each arm runs in its own throwaway container against its OWN copy of the project,
so arms never see each other's work. `--dangerously-skip-permissions` is set
*inside the disposable container only* — never on your host. Auth comes from
`.env`, kept out of every skillset.

## Run — scripted (fair, repeatable; the default)
```bash
cp .env.example .env          # add your key (Anthropic or a proxy base-url)
./arena.sh build              # build the image once (~few min)
for arm in baseline kittens cavekit ponytail; do
  ./arena.sh up  "$arm" ../../path/to/empty-project   # fresh container per arm
  ./arena.sh run "$arm"                                # IDENTICAL drive ← prompt.txt
done
./arena.sh score                                       # one comparable row per arm
./arena.sh artifacts kittens ./out/kittens             # pull the result code out
./arena.sh clean                                       # tear down, keep results/
```
`run` is the representativeness backbone: it sends the one brief (`prompt.txt`,
identical for every arm), then idle-watches the pane and stops when the agent goes
quiet (`ARENA_IDLE`, default 60s) or the equal budget runs out (`ARENA_MAX`,
default 40m). **It answers nothing.** The user "talks little, expects miracles":
if an arm stops to ask, that silence is the autonomy measurement — not a cue to
hand-hold. No human in the loop ⇒ no per-arm contamination.

Run each arm against a **copy** of the project (arena does this automatically under
`state/<arm>/work`), so arms never see each other's work.

## Driving manually (debugging only — NOT for scored runs)
- `send <arm> "<text>"` types a prompt and hits Enter.
- `keys <arm> Enter|Escape|C-c` sends a raw key (approve a prompt, interrupt).
- `peek <arm> [lines]` dumps the pane — poll it to watch.
- Hand-driving makes the comparison unfair (you'll nudge one arm more than
  another). Use it to inspect; score only `run`-driven sessions.

## Scoring — machine vs judge
`./arena.sh score [arm]` emits everything **countable**, per arm, comparable:
- `elapsed_run` — wall-time of the scripted run (equal budget, so this shows who
  finished early vs hit the cap).
- `turns · avg_ctx · peak_ctx` — context the model carried (kittens' thesis: hold
  it small via targeted injection).
- `total_tokens` — the real **cost**: every turn's full billable spend
  (input + both caches + output) summed across the run.
- `loc:` — LOC split by purpose: `code(rust)` vs `tests(rs w/ #[test])` vs
  `docs(md)` — so "200 LOC" isn't ambiguous between solution, tests, and README.
- `build:` / `test:` — does it compile, do its own tests pass.

The **judged** axes (rubric below) still need a human or judge agent reading the
transcript — plan quality, knowing-when-to-ask, plan-adherence, decision quality.
Those can't be counted, only read.

### Known limit (not silently hidden): n = 1
Each arm runs **once**. LLMs are stochastic — a single run is an anecdote, not a
distribution. For a real claim, run the loop K times (vary nothing but the seed of
chance) and report spread, not one number. The harness supports it (just re-`up` +
`run` into a fresh `state/`), but it multiplies a real run's token cost K×, so the
default is one pass. Read single-run deltas as directional, not significant.

## What to measure (the rubric)
Score each arm blind, across arms:

1. **Autonomy / self-sufficiency** — how much did it drive itself vs need hand-holding.
2. **Decision-making** — quality of the choices it made.
3. **Knows when to ask** — did it clarify the underspec, or just guess.
4. **Planning** — did it make a plan, and how good is the plan.
5. **Code quality** — how good is what it built.
6. **Plan adherence** — how closely the built result matches the plan.
7. **Did it all succeed** — does the result actually work end to end.
8. **Kept a plan to the end** — did the participant maintain a plan AND track
   progress in it through to completion (not just at the start).
9. **Time** — wall-clock the task took / effort spent.
10. **Code volume + quality** — how much code generated and of what quality (`tokei`).

Capture per arm: start/end timestamps (wall-time), `tokei` LOC on the final
`/work`, the full tmux transcript, and the artifacts. Score blind with
`../agency/{judge.py,rubric.md}` (local + remote judges).

## Files
`Dockerfile` neutral toolchain+claude · `arena.sh` build/up/**run**/peek/**score**/
context/artifacts/clean · `prompt.txt` the one shared brief · `.env` auth ·
`state/<arm>/` the arm's mounted `~/.claude` + `work` copy + run timestamps.
Judges/rubric: reuse `../agency/{judge.py,rubric.md}`.
