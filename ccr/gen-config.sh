#!/usr/bin/env bash
# Generate config.json from config.example.json, injecting:
#   - the oneprovider key pulled from ../benchmarks/arena/.env
#   - a local APIKEY (generated here, or reuse the existing one in config.json)
# Secrets are never printed. config.json is gitignored.
set -euo pipefail
cd "$(dirname "$0")"

# Canonical oneprovider key lives in ../claudeoneprovider.sh (gitignored). The
# arena .env is repurposed to point at THIS router, so don't read it here.
KEY_SRC="../claudeoneprovider.sh"
[ -f "$KEY_SRC" ] || { echo "missing $KEY_SRC (need ANTHROPIC_API_KEY=oneprovider key)"; exit 1; }

ONE_KEY="$(grep -oE '^[[:space:]]*(export[[:space:]]+)?ANTHROPIC_API_KEY=.*' "$KEY_SRC" | head -1 | sed -E 's/.*ANTHROPIC_API_KEY=//; s/^"//; s/"$//')"
[ -n "$ONE_KEY" ] || { echo "no ANTHROPIC_API_KEY in $KEY_SRC"; exit 1; }

# OpenRouter key (free-model fallback) from ../claudeopenrouter.sh
OR_SRC="../claudeopenrouter.sh"
OR_KEY=""
if [ -f "$OR_SRC" ]; then
  OR_KEY="$(grep -oE 'OPENROUTER_API_KEY="?sk-[^"[:space:]]+' "$OR_SRC" | head -1 | sed -E 's/.*OPENROUTER_API_KEY="?//')"
fi
[ -n "$OR_KEY" ] || echo "WARN: no OPENROUTER_API_KEY in $OR_SRC — openrouter provider will not work"

# reuse existing local APIKEY if config.json already has a real one, else mint
if [ -f config.json ] && grep -q '"APIKEY"' config.json; then
  LOCAL_KEY="$(grep '"APIKEY"' config.json | head -1 | sed -E 's/.*"APIKEY"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
fi
if [ -z "${LOCAL_KEY:-}" ] || [ "${LOCAL_KEY:-}" = "REPLACE_WITH_LOCAL_SHARED_SECRET" ]; then
  LOCAL_KEY="ccr-$(head -c 18 /dev/urandom | base64 | tr -dc 'a-z0-9' | head -c 24)"
fi

sed -e "s|REPLACE_WITH_LOCAL_SHARED_SECRET|$LOCAL_KEY|" \
    -e "s|REPLACE_WITH_ONEPROVIDER_KEY|$ONE_KEY|" \
    -e "s|REPLACE_WITH_OPENROUTER_KEY|$OR_KEY|" \
    config.example.json > config.json

echo "wrote config.json (APIKEY hidden). Clients use:"
echo "  ANTHROPIC_BASE_URL=http://localhost:3456"
echo "  ANTHROPIC_API_KEY=<the APIKEY in config.json>"
