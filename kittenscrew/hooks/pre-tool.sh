#!/usr/bin/env bash
# kittenscrew PreToolUse shim → delegates to kittenscrew hook.
# V7: single entry point. V2: graceful degrade.
set -euo pipefail
KS="${KITTENSCREW_BIN:-$(command -v kittenscrew 2>/dev/null || true)}"
KS="${KS:-$HOME/.claude/kittenscrew/bin/kittenscrew}"
[ -x "$KS" ] || exit 0
exec "$KS" hook pre-tool