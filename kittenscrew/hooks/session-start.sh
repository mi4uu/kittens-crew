#!/usr/bin/env bash
# kittenscrew SessionStart shim → delegates to kittenscrew hook.
# V7: never bypass kittenscrew for compression (it delegates to squeez).
# V2: graceful degrade if kittenscrew or squeez missing.
set -euo pipefail
KS="${KITTENSCREW_BIN:-$(command -v kittenscrew 2>/dev/null || true)}"
KS="${KS:-$HOME/.claude/kittenscrew/bin/kittenscrew}"
[ -x "$KS" ] || exit 0
exec "$KS" hook session-start