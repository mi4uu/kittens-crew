#!/usr/bin/env bash
# kittens-crew self-install. Idempotent. Installs everything the skillset needs:
#   1. kittenscrew binary (build from this repo if not already on PATH)
#   2. squeez (the compression engine it wraps) if missing
#   3. the kitten plugin (commands + skills + hook membrane) into <claude-dir>/plugins
#   4. wires the 8-event membrane + kittenscrew.toml via `kittenscrew init`
#
# Usage:  ./install.sh [--target <claude-dir>] [--project <dir>] [--no-build]
#   --target   the ~/.claude to install into (default: $HOME/.claude). Isolates
#              an arm / a clean test env.
#   --project  run `kittenscrew init` against this project dir (default: cwd).
#   --no-build use an existing `kittenscrew` on PATH, never cargo-build.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="${HOME}/.claude"
PROJECT="$(pwd)"
BUILD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --target)  CLAUDE_DIR="$2"; shift 2 ;;
    --project) PROJECT="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

say() { printf '🎩 [install] %s\n' "$1"; }

# --- 1. kittenscrew binary ---
if command -v kittenscrew >/dev/null 2>&1; then
  say "kittenscrew already on PATH ($(command -v kittenscrew))"
elif [ "$BUILD" = 1 ]; then
  say "building kittenscrew (cargo --release)…"
  # Build in a writable temp so the repo can be mounted read-only (Docker arm).
  BTMP="$(mktemp -d)"
  cp -r "$REPO/kittenscrew" "$BTMP/k"
  ( cd "$BTMP/k" && cargo build --release )
  mkdir -p "$HOME/.local/bin"
  install -m755 "$BTMP/k/target/release/kittenscrew" "$HOME/.local/bin/kittenscrew"
  rm -rf "$BTMP"
  export PATH="$HOME/.local/bin:$PATH"
  say "installed kittenscrew → $HOME/.local/bin"
else
  echo "kittenscrew not on PATH and --no-build set" >&2; exit 1
fi

# --- 2. squeez (dep) ---
if command -v squeez >/dev/null 2>&1 || [ -x "$CLAUDE_DIR/squeez/bin/squeez" ]; then
  say "squeez present"
else
  say "installing squeez…"
  curl -fsSL https://raw.githubusercontent.com/claudioemmanuel/squeez/main/install.sh | sh \
    || say "WARN: squeez install failed — compression hooks degrade gracefully (V2)"
fi

# --- 3. plugin into <claude-dir>/plugins/kitten ---
PLUG="$CLAUDE_DIR/plugins/kitten"
mkdir -p "$PLUG"
cp "$REPO/plugin.json" "$PLUG/"
cp -r "$REPO/commands" "$REPO/skills" "$REPO/hooks" "$PLUG/"
say "plugin → $PLUG"

# --- 4. wire membrane + config ---
say "wiring hook membrane into $CLAUDE_DIR (project $PROJECT)…"
# init writes kittenscrew.toml in CWD → run it inside the project.
( cd "$PROJECT" && kittenscrew init --target "$CLAUDE_DIR" ) || {
  rc=$?
  [ "$rc" = 3 ] && say "init exit 3: squeez unreachable — install it, then re-run" || true
  exit "$rc"
}

say "done. arm ready at $CLAUDE_DIR (project: $PROJECT)"
