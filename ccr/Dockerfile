FROM node:22-slim

# claude-code-router: a local Anthropic-protocol router. We use it purely as a
# header-hygiene passthrough in front of api.oneprovider.dev — the upstream proxy
# force-injects anthropic-beta flags (interleaved-thinking, advanced-tool-use,
# effort) which make the Anthropic backend emit malformed/stalled responses. The
# "Anthropic" transformer reconstructs the request from the parsed body, dropping
# the client's inbound anthropic-beta headers before the outbound call.
RUN npm install -g @musistudio/claude-code-router@latest

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 3456
ENTRYPOINT ["/entrypoint.sh"]
