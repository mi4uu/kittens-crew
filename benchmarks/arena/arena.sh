#!/usr/bin/env bash
# arena — drive real, INTERACTIVE Claude Code in Docker, one arm at a time.
# An arm = a clean ~/.claude tree mounted into the container; the agent under test
# is the SAME claude binary, differing only by which skillset it was given. You
# play the user: `send` a prompt, `peek` the pane, expect results. No `claude -p`.
#
#   ./arena.sh build                         # build the image (once)
#   ./arena.sh up   <arm> <project-dir>      # prep arm, run container, claude in tmux
#   ./arena.sh run  <arm> [prompt-file]      # SCRIPTED drive: send brief, idle-wait, stop
#   ./arena.sh send <arm> "<text>"           # type a prompt + Enter into the session
#   ./arena.sh keys <arm> Enter|Escape|C-c   # send a raw key
#   ./arena.sh peek <arm> [lines]            # dump the tmux pane (default 60 lines)
#   ./arena.sh context [arm]                 # turns · avg/peak ctx · TOTAL tokens (cost)
#   ./arena.sh score   [arm]                 # one comparable row: cost·ctx·LOC·build·test
#   ./arena.sh artifacts <arm> <dest>        # copy the arm's /work out
#   ./arena.sh down <arm>                    # stop + remove the container
#   ./arena.sh ls                            # list running arms
#
# REPRESENTATIVENESS: every arm is driven by the IDENTICAL scripted `run` — same
# brief, same idle-based "done" (agent goes quiet = end, we never improvise extra
# nudges), same equal time cap (ARENA_MAX). The user "talks little, expects
# miracles": run sends the brief ONCE and answers nothing — if an arm stops to ask
# a question that silence IS the measurement (autonomy axis). No hand-driving =
# no contamination = a fair, repeatable comparison. Tune: ARENA_IDLE, ARENA_MAX.
#
# arms: baseline (clean) · kittens (this repo's skillset, self-installed) · cavekit
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
IMAGE="arena:latest"
STATE="$HERE/state"
SESSION="cc"
ENV_FILE="$HERE/.env"   # ANTHROPIC_API_KEY=… (+ ANTHROPIC_BASE_URL=… for a proxy)
MODEL="${ARENA_MODEL:-haiku}"   # SAME model for every arm (fairness). Thesis is
                                # sharpest on a small model: does kittens-crew make
                                # it viable where the bare model flails?
PROMPT_DEFAULT="$HERE/prompt.txt"        # the one sloppy brief, identical for all arms
IDLE_SECS="${ARENA_IDLE:-60}"            # pane unchanged this long ⇒ agent done/waiting
MAX_SECS="${ARENA_MAX:-2400}"            # hard equal budget per arm (40 min default)
POLL_SECS="${ARENA_POLL:-10}"            # how often run polls the pane
SYSPROMPT="${ARENA_SYSPROMPT:-0}"        # 1 ⇒ also force the per-arm guide into the
                                         # system prompt (--append-system-prompt-file),
                                         # for when a weak model ignores CLAUDE.md

cname() { echo "arena-$1"; }
need_env() { [ -f "$ENV_FILE" ] || { echo "missing $ENV_FILE (copy .env.example)"; exit 1; }; }

# Transfer the SAME auth from the host's Claude Code into the arm's clean
# ~/.claude — ONLY the OAuth credentials, never history or other artifacts.
# macOS keeps them in the Keychain; Linux/other in a file.
seed_auth() {
  local cdir="$1"
  if security find-generic-password -s "Claude Code-credentials" -w >/dev/null 2>&1; then
    security find-generic-password -s "Claude Code-credentials" -w > "$cdir/.credentials.json"
    echo "  auth: seeded host Keychain OAuth → $cdir/.credentials.json"
  elif [ -f "$HOME/.claude/.credentials.json" ]; then
    cp "$HOME/.claude/.credentials.json" "$cdir/.credentials.json"
    echo "  auth: copied host .credentials.json"
  else
    echo "  auth: WARN no host credentials found — relying on .env"
  fi
}

cmd_build() {
  # Neutral base image — nothing skillset-specific is baked in.
  ( cd "$HERE" && docker build -t "$IMAGE" . )
}

# Only kittens needs the repo mounted (its installer + crate live here). Every
# other arm installs from GitHub and never sees /repo — keeps them clean too.
needs_repo() { case "$1" in kittens) return 0 ;; *) return 1 ;; esac; }

# Provision the arm's clean ~/.claude. Runs INSIDE the container so each arm
# installs its OWN skillset from nothing — no host install, no cross-arm
# leftovers. Auth comes from .env (env), so the dir stays free of history.
provision() {
  local arm="$1" c; c="$(cname "$arm")"
  case "$arm" in
    baseline)
      : ;;  # pure Claude Code, zero skillset, no /repo in sight
    kittens)
      # self-install: cargo-builds kittenscrew in-container, installs squeez,
      # drops the plugin, wires the membrane. Exactly the real install path.
      docker exec "$c" bash /repo/install.sh --target /root/.claude --project /work ;;
    cavekit)
      # cavekit v4 — pure skills + commands, no toggle/activation. The marketplace
      # install enables it; that IS the activation.
      docker exec "$c" bash -lc 'claude plugin marketplace add juliusbrussee/cavekit \
        && claude plugin install ck@cavekit-marketplace' ;;
    ponytail)
      # ponytail — Claude Code marketplace plugin (auto-activates each session).
      docker exec "$c" bash -lc 'claude plugin marketplace add DietrichGebert/ponytail \
        && claude plugin install ponytail@ponytail' ;;
    *) echo "unknown arm: $arm (baseline|kittens|cavekit|ponytail)"; exit 2 ;;
  esac
}

cmd_up() {
  local arm="$1" project="$2"; need_env
  [ -d "$project" ] || { echo "project not found: $project"; exit 1; }
  local c; c="$(cname "$arm")"
  local work="$STATE/$arm/work" cdir="$STATE/$arm/claude"
  rm -rf "$STATE/$arm"; mkdir -p "$work" "$cdir"
  seed_auth "$cdir"                      # same token, every arm; nothing else
  cp -r "$project/." "$work/"            # each arm mutates its OWN copy
  # Each skill arm gets its OWN small guide (guides/<arm>.md): what commands/skills
  # it ships, what they do, when + in what order — transcribed from that skillset's
  # OWN docs, same structure/length for every arm, so it's equal treatment not a
  # juiced advantage. A real plugin being installed ≠ a weak model reaching for it;
  # the guide bridges that. Baseline gets none — it has no skillset to use.
  case "$arm" in
    baseline) : ;;
    *) [ -f "$HERE/guides/$arm.md" ] && cp "$HERE/guides/$arm.md" "$cdir/CLAUDE.md" \
         || echo "  WARN: no guides/$arm.md — arm runs without a usage guide" ;;
  esac
  docker rm -f "$c" >/dev/null 2>&1 || true
  # Baseline never sees /repo → it cannot pick up any of our tooling.
  local repo_mount=(); needs_repo "$arm" && repo_mount=(-v "$REPO:/repo:ro")
  # IS_SANDBOX=1 lets claude run --dangerously-skip-permissions as root in the
  # disposable container (it refuses as root otherwise).
  docker run -d --name "$c" --env-file "$ENV_FILE" -e IS_SANDBOX=1 \
    ${repo_mount[@]+"${repo_mount[@]}"} -v "$cdir:/root/.claude" -v "$work:/work" \
    "$IMAGE" sleep infinity >/dev/null
  provision "$arm"
  # Skip first-run onboarding (theme picker etc.) so the session opens straight to
  # the prompt. ~/.claude.json lives in HOME, beside the mounted ~/.claude dir —
  # ONLY these onboarding flags, never any history/projects.
  docker exec "$c" bash -lc 'cat > /root/.claude.json <<JSON
{"hasCompletedOnboarding":true,"lastOnboardingVersion":"2.1.160","theme":"dark","hasCompletedClaudeInChromeOnboarding":true}
JSON'
  date +%s > "$STATE/$arm/started_at"   # wall-time clock (rubric: time spent)
  # Fallback for a model that ignores CLAUDE.md: force the SAME per-arm guide into
  # the system prompt. The guide is already mounted at /root/.claude/CLAUDE.md.
  local append=""
  if [ "$SYSPROMPT" = 1 ] && [ "$arm" != baseline ] && [ -f "$cdir/CLAUDE.md" ]; then
    append="--append-system-prompt-file /root/.claude/CLAUDE.md"
    echo "  sysprompt: forcing guides/$arm.md into the system prompt"
  fi
  # start claude INTERACTIVELY in a detached tmux session.
  docker exec -d "$c" tmux new-session -d -s "$SESSION" -x 220 -y 50 \
    "cd /work && claude --model $MODEL $append --dangerously-skip-permissions; exec bash"
  # First-launch dialogs (theme is pre-seeded away): trust-folder → Enter;
  # bypass-permissions warning → Down, Enter. Deterministic on first run; an
  # empty Enter on the main prompt is harmless if a dialog isn't shown.
  sleep 6;  docker exec "$c" tmux send-keys -t "$SESSION" Enter
  sleep 3;  docker exec "$c" tmux send-keys -t "$SESSION" Down
  sleep 1;  docker exec "$c" tmux send-keys -t "$SESSION" Enter
  sleep 3
  echo "up: $arm → container $c, claude in tmux '$SESSION'. peek with: ./arena.sh peek $arm"
}

# SCRIPTED, identical drive for every arm — the core of a fair benchmark. Sends
# the brief ONCE, then watches the pane: when it stops changing for IDLE_SECS the
# agent is done (or waiting on a question we deliberately don't answer — the user
# "talks little, expects miracles"). Stops at MAX_SECS so every arm gets the same
# budget. No human in the loop ⇒ no per-arm contamination ⇒ repeatable.
cmd_run() {
  local arm="$1" pf="${2:-$PROMPT_DEFAULT}" c; c="$(cname "$arm")"
  [ -f "$pf" ] || { echo "prompt file not found: $pf"; exit 1; }
  docker ps --format '{{.Names}}' | grep -qx "$c" || { echo "$arm not up — ./arena.sh up $arm <dir> first"; exit 1; }
  local brief; brief="$(cat "$pf")"
  echo "run: $arm ← $(basename "$pf") (idle=${IDLE_SECS}s cap=${MAX_SECS}s)"
  # send the brief as one literal block, then Enter — exactly what a user would type.
  docker exec "$c" tmux send-keys -t "$SESSION" -l -- "$brief"
  docker exec "$c" tmux send-keys -t "$SESSION" Enter
  date +%s > "$STATE/$arm/run_started"
  local start; start="$(date +%s)"
  local last_change="$start" prev="" cur now quiet
  while :; do
    sleep "$POLL_SECS"
    cur="$(docker exec "$c" tmux capture-pane -t "$SESSION" -p 2>/dev/null | cksum 2>/dev/null || echo x)"
    now="$(date +%s)"
    [ "$cur" != "$prev" ] && { last_change="$now"; prev="$cur"; }
    quiet=$(( now - last_change ))
    if [ "$quiet" -ge "$IDLE_SECS" ]; then
      echo "run: $arm done — pane idle ${quiet}s after $(( now - start ))s"
      break
    fi
    if [ $(( now - start )) -ge "$MAX_SECS" ]; then
      echo "run: $arm CAP — hit ${MAX_SECS}s budget, stopping"
      break
    fi
  done
  date +%s > "$STATE/$arm/run_ended"
}

# Elapsed wall-time + generated-code volume (tokei). Pair with the judges' rubric.
cmd_report() {
  local arm="$1" c; c="$(cname "$arm")"
  local started; started="$(cat "$STATE/$arm/started_at" 2>/dev/null || echo 0)"
  local now; now="$(date +%s)"
  echo "arm: $arm"
  [ "$started" != 0 ] && echo "elapsed: $(( now - started ))s"
  echo "--- tokei /work ---"
  docker exec "$c" tokei /work 2>/dev/null || echo "(container down)"
}

cmd_send() { docker exec "$(cname "$1")" tmux send-keys -t "$SESSION" -- "$2"; docker exec "$(cname "$1")" tmux send-keys -t "$SESSION" Enter; }
cmd_keys() { docker exec "$(cname "$1")" tmux send-keys -t "$SESSION" "$2"; }
cmd_peek() { docker exec "$(cname "$1")" tmux capture-pane -t "$SESSION" -p | sed 's/\x1b\[[0-9;]*m//g' | tail -n "${2:-60}"; }
# Context-window size per assistant turn = input + cache_creation + cache_read
# (the full prompt the model saw). Reports avg + peak across the run — the thesis
# metric: kittens-crew should hold context small via targeted injection.
# Per turn it emits two numbers: the context the model SAW (input+both caches) and
# the FULL billable spend (that context + output). avg/peak come from the first;
# total_tokens — the actual COST, what each arm "burned" — sums the second.
cmd_context() {
  local arm="$1"
  local dir="$STATE/$arm/claude/projects"
  [ -d "$dir" ] || { echo "$arm: no session yet"; return; }
  find "$dir" -name '*.jsonl' -exec cat {} + 2>/dev/null \
    | jq -rc 'select(.message.usage) | .message.usage
              | [ (.input_tokens + (.cache_creation_input_tokens // 0) + (.cache_read_input_tokens // 0)),
                  (.input_tokens + (.cache_creation_input_tokens // 0) + (.cache_read_input_tokens // 0) + (.output_tokens // 0)) ]
              | @tsv' 2>/dev/null \
    | awk -v arm="$arm" '{ctx=$1; s+=ctx; if(ctx>mx)mx=ctx; cost+=$2; n++}
        END{ if(n>0) printf "%-9s turns=%-4d avg_ctx=%-7d peak_ctx=%-7d total_tokens=%d\n", arm, n, s/n, mx, cost;
             else printf "%-9s no turns yet\n", arm }'
}

# One comparable scorecard row per arm. Everything machine-measurable in one place:
# turns · context avg/peak · total token cost · LOC split (code vs tests vs docs) ·
# does it build · do its tests pass. Plan-quality / autonomy / plan-conformance stay
# for the human-or-judge pass over the transcript — those need reading, not counting.
cmd_score() {
  local arm="$1" c; c="$(cname "$arm")"
  echo "═══ $arm ═══"
  # time: prefer the scripted-run window, fall back to up→now.
  local rs re; rs="$(cat "$STATE/$arm/run_started" 2>/dev/null || echo 0)"
  re="$(cat "$STATE/$arm/run_ended" 2>/dev/null || date +%s)"
  [ "$rs" != 0 ] && echo "elapsed_run: $(( re - rs ))s"
  cmd_context "$arm"
  # LOC split by purpose — docs(markdown) vs tests vs code, via tokei JSON + path.
  if docker ps --format '{{.Names}}' | grep -qx "$c"; then
    docker exec "$c" bash -lc '
      root="$(dirname "$(find /work -name Cargo.toml -not -path "*/target/*" 2>/dev/null | head -1)")"
      [ -z "$root" ] || [ "$root" = "." ] && root=/work
      code=$(tokei "$root" -t Rust 2>/dev/null | awk "/^ Total/{print \$3}")
      docs=$(tokei "$root" -t Markdown 2>/dev/null | awk "/^ Total/{print \$3}")
      tests=$(grep -rl --include="*.rs" -e "#\[test\]" -e "#\[cfg(test)\]" "$root" 2>/dev/null | xargs cat 2>/dev/null | wc -l | tr -d " ")
      printf "loc: code(rust)=%s tests(rs w/ #[test])=%s docs(md)=%s\n" "${code:-0}" "${tests:-0}" "${docs:-0}"
      cd "$root" 2>/dev/null && {
        cargo build -q 2>/dev/null && echo "build: PASS" || echo "build: FAIL"
        cargo test  -q 2>/dev/null && echo "test:  PASS" || echo "test:  FAIL"
      } || echo "build: n/a (no Cargo.toml)"
    ' 2>/dev/null || echo "(container down — LOC/build from artifacts only)"
  else
    echo "(container down — bring it up or score from copied artifacts)"
  fi
}

cmd_artifacts() { docker cp "$(cname "$1"):/work/." "$2"; echo "copied /work → $2"; }
cmd_down() { docker rm -f "$(cname "$1")" >/dev/null 2>&1 && echo "down: $1"; }

# Auto-cleanup after analysis: tear down every container + drop the per-arm
# ~/.claude configs and work copies (state/), but KEEP results/ (the harvested
# code + transcripts + stories). The result code we want; the claude config we don't.
cmd_clean() {
  for a in baseline kittens cavekit ponytail; do docker rm -f "$(cname "$a")" >/dev/null 2>&1; done
  rm -rf "$STATE"
  echo "cleaned: containers down, state/ (configs+work copies) removed. results/ kept."
}
cmd_ls() { docker ps --filter "name=arena-" --format '{{.Names}}\t{{.Status}}'; }

sub="${1:-}"; shift || true
case "$sub" in
  build) cmd_build ;;
  up) cmd_up "$@" ;;
  run) cmd_run "$@" ;;
  send) cmd_send "$@" ;;
  keys) cmd_keys "$@" ;;
  peek) cmd_peek "$@" ;;
  report) cmd_report "$@" ;;
  context) if [ $# -gt 0 ]; then cmd_context "$1"; else for a in baseline kittens cavekit ponytail; do cmd_context "$a"; done; fi ;;
  score) if [ $# -gt 0 ]; then cmd_score "$1"; else for a in baseline kittens cavekit ponytail; do cmd_score "$a"; done; fi ;;
  artifacts) cmd_artifacts "$@" ;;
  down) cmd_down "$@" ;;
  clean) cmd_clean ;;
  ls) cmd_ls ;;
  *) sed -n '2,30p' "$0"; exit 0 ;;
esac
