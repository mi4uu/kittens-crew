#!/usr/bin/env bash
# Start the router, then stay foreground so the container lives. `ccr start` may
# fork a daemon and return, so we don't rely on it blocking — we tail its log
# (or idle) to keep PID 1 alive.
set -uo pipefail

mkdir -p /root/.claude-code-router

# `ccr start` runs in the foreground and blocks — make it PID 1 so the container
# lives exactly as long as the service, and SIGTERM on `docker stop` reaches it.
# Stale pid from a previous boot (image fs is fresh, but be safe):
rm -f /root/.claude-code-router/.claude-code-router.pid 2>/dev/null || true
exec ccr start
