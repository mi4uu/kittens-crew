#!/usr/bin/env bash
# arena — drive real, INTERACTIVE Claude Code in Docker, one arm at a time.
# An arm = a clean ~/.claude tree mounted into the container; the agent under test
# is the SAME claude binary, differing only by which skillset it was given. You
# play the user: `send` a prompt, `peek` the pane, expect results. No `claude -p`.
#
#   ./arena.sh build                         # build the image (once)
#   ./arena.sh up   <arm> <project-dir>      # prep arm, run container, claude in tmux
#   ./arena.sh send <arm> "<text>"           # type a prompt + Enter into the session
#   ./arena.sh keys <arm> Enter|Escape|C-c   # send a raw key
#   ./arena.sh peek <arm> [lines]            # dump the tmux pane (default 60 lines)
#   ./arena.sh artifacts <arm> <dest>        # copy the arm's /work out
#   ./arena.sh down <arm>                    # stop + remove the container
#   ./arena.sh ls                            # list running arms
#
# arms: baseline (clean) · kittens (this repo's skillset, self-installed) · cavekit
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
IMAGE="arena:latest"
STATE="$HERE/state"
SESSION="cc"
ENV_FILE="$HERE/.env"   # ANTHROPIC_API_KEY=… (+ ANTHROPIC_BASE_URL=… for a proxy)

cname() { echo "arena-$1"; }
need_env() { [ -f "$ENV_FILE" ] || { echo "missing $ENV_FILE (copy .env.example, add your key)"; exit 1; }; }

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
  cp -r "$project/." "$work/"            # each arm mutates its OWN copy
  docker rm -f "$c" >/dev/null 2>&1 || true
  # Baseline never sees /repo → it cannot pick up any of our tooling.
  local repo_mount=(); needs_repo "$arm" && repo_mount=(-v "$REPO:/repo:ro")
  docker run -d --name "$c" --env-file "$ENV_FILE" \
    "${repo_mount[@]}" -v "$cdir:/root/.claude" -v "$work:/work" \
    "$IMAGE" sleep infinity >/dev/null
  provision "$arm"
  date +%s > "$STATE/$arm/started_at"   # wall-time clock (rubric: time spent)
  # start claude INTERACTIVELY in a detached tmux session.
  docker exec -d "$c" tmux new-session -d -s "$SESSION" -x 220 -y 50 \
    'cd /work && claude --dangerously-skip-permissions; exec bash'
  sleep 2
  echo "up: $arm → container $c, claude in tmux '$SESSION'. peek with: ./arena.sh peek $arm"
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
cmd_peek() { docker exec "$(cname "$1")" tmux capture-pane -t "$SESSION" -p | tail -n "${2:-60}"; }
cmd_artifacts() { docker cp "$(cname "$1"):/work/." "$2"; echo "copied /work → $2"; }
cmd_down() { docker rm -f "$(cname "$1")" >/dev/null 2>&1 && echo "down: $1"; }
cmd_ls() { docker ps --filter "name=arena-" --format '{{.Names}}\t{{.Status}}'; }

sub="${1:-}"; shift || true
case "$sub" in
  build) cmd_build ;;
  up) cmd_up "$@" ;;
  send) cmd_send "$@" ;;
  keys) cmd_keys "$@" ;;
  peek) cmd_peek "$@" ;;
  report) cmd_report "$@" ;;
  artifacts) cmd_artifacts "$@" ;;
  down) cmd_down "$@" ;;
  ls) cmd_ls ;;
  *) sed -n '2,30p' "$0"; exit 0 ;;
esac
