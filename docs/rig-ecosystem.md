# rig ecosystem — knowledge treasury for the kittenscrew harness

> Curated 2026-06-25 from [awesome-rig](https://github.com/0xPlaygrounds/awesome-rig) +
> rig core docs. We will NOT use all of it — this is a reference library of solutions to
> mine as the harness (T60–T73) is built. Tags: **[DEP-candidate]** real dependency
> candidate · **[MINE]** steal the idea/pattern · **[REF]** learning/example only ·
> **[SKIP]** crypto/niche/abandoned. rig is pre-1.0 — pin exact versions, expect breaking
> changes.

## rig core — capabilities

**Version:** rig-core `0.39.0` (facade `rig` 0.36.x). Active, production users (St. Jude,
Neon, Nethermind, Dria), explicitly pre-1.0 ("future updates **will** contain breaking
changes" — pin versions). WASM-compatible core. OpenTelemetry GenAI semantic-convention
compliant out of the box.

**Core feature surface**
- 24+ model providers behind one interface (OpenAI, Anthropic, Gemini, AWS Bedrock, Cohere…).
- Multi-modal: completions/chat, embeddings, transcription, audio-gen, image-gen.
- Agents w/ multi-turn, tool-calling, reasoning, streaming.
- RAG + 10+ vector stores (LanceDB, Qdrant, Mongo, Postgres, SQLite, Redis…).
- Structured extraction (`Extractor`), document `loaders`, pluggable conversation memory.
- Pipeline/agent-ops DSL composing operations (`parallel`, agent ops, passthrough).

**Key public abstractions (the Driver-trait surface, T60/T61)**
- `completion::{CompletionModel, CompletionRequest, CompletionClient, Chat, Prompt}` — the
  call we wrap behind kittenscrew's `Driver`. **`CompletionModel` is the seam.**
- `agent::{Agent, AgentBuilder}` — preassembled loop (system prompt + tools + RAG). Likely
  too opinionated for a deterministic harness; build directly on `CompletionModel`.
- `embeddings::{EmbeddingModel, EmbeddingsBuilder}`, `vector_store::VectorStoreIndex` — retrieval.
- `tool::{Tool, ToolSet}` — function-calling registry. `extractor::Extractor` — typed output
  (verification/gate payloads). `pipeline` (agent_ops, `parallel`); `streaming`; `providers::ProviderClient`.

## Ecosystem catalog

### Providers & model backends
- [rig-llama-cpp](https://github.com/camperking/rig-llama-cpp) — local GGUF via llama.cpp; streaming, tool-calling, reasoning, multimodal. **[DEP-candidate]** local tier (T61).
- [rig-dyn](https://github.com/GustavoWidman/rig-dyn) — dynamic client/provider abstraction over rig-core; pick provider at runtime. **[MINE]** failover/routing pattern for the Driver.
- [rig-extra](https://github.com/launcher-rs/rig-extra-project) — lightweight rig-core extensions. **[MINE]** small adapters.

### Agent loops / orchestration / graph / workflow
- [graph-flow](https://github.com/a-agmon/rs-graph-llm) — type-safe LangGraph-style multi-agent graph execution. **[MINE]** drive-loop/graph (T62).
- [weavegraph](https://github.com/Idleness76/weavegraph) — concurrent graph workflow w/ **versioned state + deterministic merges**. **[MINE]** strongest determinism reference (T62/T69). BSP/version-gated core, not a ready-set DAG — steal idioms, don't adopt as engine.
- [metalcraft](https://github.com/rust4ai/metalcraft) — LangGraph-style stateful graph orchestrator. **[MINE]** alt graph pattern (T62).
- [awpak-ai](https://github.com/afuentesan/awpak-tui/tree/main/awpak-ai) — agent/command/URL orchestration via execution graphs. **[REF]** graph-as-config.
- [nika](https://github.com/supernovae-st/nika) — semantic YAML workflow engine, DAG + MCP + multi-provider; read-XOR-write capability security + reproducible traces. **[MINE]** gate/config patterns (T64).
- [rigs](https://github.com/M4n5ter/rigs) — orchestration framework on Rig. **[REF]** small/early.
- [flow-like](https://github.com/Rheosoph/flow-like) — WASM workflow nodes SDK. **[SKIP]** off-axis.
- [reasonkit-core](https://github.com/reasonkit/reasonkit-core) — Rust-native auditable reasoning engine. **[MINE]** auditable-reasoning for T63.
- [dspy-rs](https://github.com/krypticmouse/DSRs) — DSPy rewrite on Rig; programmatic prompt optimization. **[MINE]** prompt-compilation idea.
- [coral-rs](https://github.com/Coral-Protocol/coral-rs) — Rig + RMCP helpers for Coral agents. **[REF]** MCP wiring example.

### Tools / tool-macros / MCP / sandboxing / governance
- [Agent Governance Toolkit](https://github.com/microsoft/agent-governance-toolkit) — MS policy/governance w/ Rust Rig integration for **guarded** tools. **[DEP-candidate]** tripwire-gate (T64).
- [yart](https://github.com/pupplecat/yart) — proc-macro utils incl. `#[rig_tool]`. **[MINE]** tool-macro ergonomics.
- [rig-openapi-tools](https://github.com/skharchikov/rig-openapi-tools) — generate Rig tools from OpenAPI. **[DEP-candidate]** for HTTP-API tools.
- [llm-coding-tools](https://github.com/Sewer56/llm-coding-tools) — lightweight Rig coding-agent tools. **[REF]**.
- [skill](https://github.com/kubiyabot/skill) — runtime/CLI/MCP server for agent skills, Rig-powered. **[MINE]** skill-as-unit governance.
- [unifai-sdk-rs](https://github.com/unifai-network/unifai-sdk-rs) — dynamic tools + agent-to-agent comms. **[SKIP]** network-specific.

### Memory / vector stores / RAG / retrieval
- [rig-memvid](https://github.com/ForeverAngry/rig-memvid) — Memvid-backed persistent memory + lexical store. **[MINE]** durable-memory (T69).
- [rig-redis-vectorstore](https://github.com/daric93/rig-redis-vectorstore) — Redis/RediSearch vector store, KNN + metadata filter. **[DEP-candidate]** if Redis in stack.
- [Cortex Memory](https://github.com/sopaco/cortex-mem) — full memory system: extraction, vector search, MCP/REST/CLI, dashboards. **[MINE]** architecture reference; heavyweight.

### Observability / tracing / eval / testing
- [rig-tap](https://github.com/ForeverAngry/rig-tap) — backend-agnostic observability events + lifecycle taps. **[DEP-candidate]** observability — pairs w/ rig-core OTel.
- [rig-retrieval-evals](https://github.com/ForeverAngry/rig-retrieval-evals) — eval harness for retrieval/KB workflows. **[MINE]** verify/eval (T63).
- [nitpicker](https://github.com/arsenyinfo/nitpicker) — multi-reviewer code-review CLI, **parallel Rig agents + debate mode**. **[MINE]** adversarial-verify (T63).
- [taquba-research](https://github.com/micllam/taquba-research) — durable Rig agent: multi-step runs persisted to object storage, **resumes after crash**. **[MINE]** idempotent checkpoint/resume (T69).

### Integrations (web, data, domain-specific)
- [kumo](https://github.com/wihlarkop/kumo) — async web crawler. **[DEP-candidate]** fetch tool.
- [markitdown-rs](https://github.com/uhobnil/markitdown-rs) — doc→Markdown. **[DEP-candidate]** ingestion tool.
- [syncable-cli](https://github.com/syncable-dev/syncable-cli) — repo analysis → IaC. **[REF]**.
- [riglr](https://github.com/riglr/riglr), [Rig Onchain Kit](https://github.com/0xPlaygrounds/rig-onchain-kit), [solagent](https://github.com/zTgx/solagent.rs), [Listen](https://github.com/piotrostr/listen), [nine](https://github.com/NethermindEth/nine), [Amico](https://github.com/AIMOverse/amico) — crypto/decentralized-AI. **[SKIP]**.

### Templates / examples / learning
- [VT Code](https://github.com/vinhnx/vtcode) — terminal coding agent, Tree-sitter + ast-grep + Rig model selection. **[REF]** model-routing reference.
- [Dirge](https://github.com/dirge-code/dirge) / [Zerostack](https://github.com/gi-dellav/zerostack) — minimal Rust coding agents. **[REF]** smallest end-to-end loop examples.
- [Metalcraft Agent](https://github.com/rust4ai/metalcraft-agent) — personas, skills, **tool approval**. **[MINE]** tool-approval gate UX (T64).
- [deepwiki-rs](https://github.com/sopaco/deepwiki-rs) — codebase→docs. **[REF]**. · [ChatShell](https://github.com/chatshellapp/chatshell-desktop) — agentic desktop on rig-core + Tauri. **[REF]**.
- git/code CLIs: [git-iris](https://github.com/hyperb1iss/git-iris), [rv](https://github.com/gi-dellav/rv), [committor](https://github.com/simonhdickson/committor), [squid](https://github.com/DenysVuika/squid), [probe](https://github.com/buger/probe). **[REF]** small Rig-usage samples.
- Official: [docs.rig.rs](https://docs.rig.rs/), [ECOSYSTEM.md](https://github.com/0xPlaygrounds/rig/blob/main/ECOSYSTEM.md), [guides](https://docs.rig.rs/guides).

## Top mines for kittenscrew

- **Provider backend (T61):** rig-core direct on `CompletionModel`; rig-dyn pattern for runtime routing/failover.
- **Local tier (T61):** rig-llama-cpp — GGUF w/ tool-calling + streaming, the only mature local provider.
- **Drive-loop / graph (T62):** weavegraph (versioned state + deterministic merges) — closest determinism match; steal idioms.
- **Gate / governance (T64):** MS Agent Governance Toolkit (policy enforcement) + nika (DAG+gate config, read-XOR-write capabilities).
- **Verification / eval (T63):** nitpicker (parallel-agent debate) + rig-retrieval-evals + reasonkit-core (auditable reasoning).
- **Observability:** rig-tap lifecycle taps, complementing rig-core's built-in OpenTelemetry GenAI traces.
- **Memory (T69 store):** rig-memvid (durable memory); Cortex Memory (deeper architecture reference).
- **Durable / resume (T69 checkpoint):** taquba-research — object-store-persisted, crash-resumable; canonical idempotent-resume reference.
