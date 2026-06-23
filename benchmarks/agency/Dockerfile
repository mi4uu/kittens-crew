# Lean Debian for isolated agentic runs: rust + uv/python + rtk + claude CLI.
# Auth lives in a mounted volume (/root/.claude) you populate once with `claude /login`.
FROM debian:stable-slim

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config libssl-dev jq \
    && rm -rf /var/lib/apt/lists/*

# uv + python
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"
RUN uv python install 3.12

# rust (stable) + tokei for LOC
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install tokei

# rtk (Rust Token Killer) — present so the kittens arm can actually use it
RUN cargo install --git https://github.com/rtk-ai/rtk || \
    (curl -fsSL https://raw.githubusercontent.com/rtk-ai/rtk/refs/heads/master/install.sh | sh)

# claude code CLI (+ node)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && apt-get install -y nodejs \
    && npm install -g @anthropic-ai/claude-code

WORKDIR /bench
COPY . /bench
# vendor caveman/ponytail skill text in at build time if available (else arms skip)
CMD ["bash"]
