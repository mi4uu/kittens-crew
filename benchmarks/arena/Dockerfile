# arena base — NEUTRAL toolchain only. The same dev environment every arm gets:
# nothing here favours any skillset. kittenscrew, squeez, cavekit, ponytail are
# NOT baked in — each arm installs ITS OWN skillset at provision time, into the
# clean ~/.claude mounted at run time. Baseline therefore stays pristine: pure
# Claude Code with none of our leftovers on PATH.
FROM debian:stable-slim
ENV DEBIAN_FRONTEND=noninteractive

# Generic dev tools any agent (any arm) might reach for — neutral.
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config libssl-dev \
      tmux jq ripgrep procps less unzip \
    && rm -rf /var/lib/apt/lists/*

# uv + python 3.12
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"
RUN uv python install 3.12

# rust (stable, minimal) + tokei (LOC for the judges)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install tokei

# bun (needs unzip, installed above)
RUN curl -fsSL https://bun.sh/install | bash
ENV PATH="/root/.bun/bin:${PATH}"

# node + Claude Code CLI (the one binary every arm runs)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs && rm -rf /var/lib/apt/lists/* \
    && npm install -g @anthropic-ai/claude-code

WORKDIR /work
CMD ["bash"]
