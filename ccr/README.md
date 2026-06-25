# ccr — central Anthropic-protocol router (header hygiene)

A one-container [claude-code-router](https://github.com/musistudio/claude-code-router)
that sits in front of `api.oneprovider.dev`. The upstream proxy force-injects
`anthropic-beta` flags (interleaved-thinking, advanced-tool-use, effort) which make
the Anthropic backend emit malformed or multi-minute-stalled responses. The
`Anthropic` passthrough transformer rebuilds each request from the parsed body, so
the client's inbound `anthropic-beta` headers never reach the upstream.

Always-on, reachable from everywhere on the host.

## Setup

```bash
cp config.example.json config.json     # config.json is gitignored (holds keys)
# edit config.json: set APIKEY (a local shared secret you pick) and the
# oneprovider api_key. Or generate it:
#   ./gen-config.sh                     # pulls the oneprovider key from ../benchmarks/arena/.env
docker compose up -d --build
docker compose logs -f                  # confirm "service listen on 3456"
```

## Use from any Claude Code

```bash
export ANTHROPIC_BASE_URL=http://localhost:3456
export ANTHROPIC_API_KEY=<APIKEY from config.json>
claude --model sonnet-4-6 ...
```

The arena reads these from `benchmarks/arena/.env`.

## Notes

- `Router.default` picks the model. The arena uses one model across all arms
  (fairness), so the default is enough — per-arm `--model` is cosmetic here.
- Change the active model live with `/model oneprovider,<model>` inside Claude Code.
- `config.json` holds both the local APIKEY and the oneprovider key — **gitignored**,
  never commit it.
