# SPEC

## §G GOAL
web Obsidian clone. edit `.md` vault from browser & phone. core-plugin parity, no graph.

## §C CONSTRAINTS
- runtime: Bun
- backend: Elysia
- storage: filesystem. vault = dir tree of `.md` + `.media/`
- media path: `<note-dir>/.media/<file>` (same folder as note)
- no graph view
- no Obsidian plugin runtime. only what default Obsidian ships w/o community plugins
- UI ! responsive ≥ 320px (mobile) & desktop
- theme: light & dark. follow `prefers-color-scheme` + manual toggle. theme pref = device-local (localStorage)
- Markdown dialect = Obsidian flavor (CommonMark + GFM + wikilinks + embeds + callouts + math + mermaid + footnotes + tasks + frontmatter + tags + `==hl==` + `%%comments%%`)
- no external DB. index = in-mem + on-disk cache (`.brain/index.json`)
- single-user vault per server instance (auth ⊥ in v1)
- frontend stack: React 18 + CodeMirror 6 + unified/remark/rehype + highlight.js + KaTeX + mermaid (lazy)
- syntax highlighting: highlight.js, theme via CSS variables
- vault-local config = `<VAULT>/.brain/*.json` (settings, index, folder-meta, trash). env = bootstrap default only
- UI primitives: Radix UI (`@radix-ui/react-*`) — menus, dialogs, tooltips, popovers, tabs, toolbar, switch, toast, scroll-area
- UI vibe = desktop app (compact rows, hairline borders, subtle elevation). ref pattern: terax-ai `src/components/ui/context-menu.tsx` (Radix + Tailwind, dark-first)
- default vault + settings paths follow XDG Base Directory Spec on ALL platforms (macOS, Linux, Windows): vault = `${XDG_DATA_HOME:-$HOME/.local/share}/brain.md/vault`, settings = `${XDG_CONFIG_HOME:-$HOME/.config}/brain.md/`. ⊥ implicit `./vault` fallback.
- AI surface: MCP server (HTTP+SSE on `/mcp/*` of same Elysia app) + RAG via LanceDB (local, embedded, per-vault). zero external API key required for default install.
- embedding (default): bge-small-en-v1.5 via @xenova/transformers (ONNX, local, dim=384, ~133MB cache on first use).
- embedding (alternative): any OpenAI-compatible `/v1/embeddings` endpoint (Ollama, LM Studio, OpenAI, etc.). configurable in Settings.
- vector store: LanceDB (`@lancedb/lancedb`) at `<VAULT>/.brain/lance/`.
- chunking: paragraph-based, ≤512-token chunks w/ ~64-token overlap; per-note metadata (path, heading-trail, line range); frontmatter excluded.
- auth: OPTIONAL. default = no auth (auth.json absent → all endpoints open). user sets password via Settings → `<VAULT>/.brain/auth.json` written (argon2id) → bearer token required for HTTP API + MCP. removable via Settings.
- per-folder MCP permissions: each folder may set explicit `{read,write}` in `<VAULT>/.brain/folder-meta.json` under `mcp.<folder-path>`. default = `{read:true, write:true}`; explicit override wins; resolution walks note's parent folder ancestors to root; nearest override applies. applies to MCP tools only, ! HTTP API.

## §I INTERFACES

### web UI
- `/` → app shell. grid: topbar / sidebar / main(2-col panes share toolbar row)
- sidebar = collapsable sections (Radix Accordion, type="multiple"): Bookmarks | Vault tree | Tags | Outline | Backlinks | Related. each section has chevron-toggle header; state per-section persisted in localStorage. header w/ search + new note + new folder above the accordion. Related visible only when path && rag.enabled.
- toolbar = single row above both panes (editor + preview). format actions emit CM6 tx.
- mobile: drawer file tree, swipe to open, edit/preview tab switch
- all interactive controls (menu, dropdown, context-menu, dialog, popover, tooltip, tabs, toolbar, select, switch, toast, scroll-area) ! Radix-based. local wrappers in `web/src/components/ui/<primitive>.tsx`
- Settings dialog tabs: Appearance | Vault | Git | AI / RAG | Security | Editor

### HTTP API (Elysia, JSON)
#### note + folder
- `GET  /api/tree` → `{folders:[…], notes:[…]}` full vault tree
- `GET  /api/note/*path` → `{path, content, mtime}` raw md
- `PUT  /api/note/*path` body `{content}` → `{path, mtime}` upsert
- `DELETE /api/note/*path` → `{ok, trashed}`
- `POST /api/folder/*path` → `{ok}` mkdir
- `DELETE /api/folder/*path` → `{ok, trashed}`
- `POST /api/rename` body `{from, to}` → `{ok, patchedFiles, totalReplacements}` (rename + update inbound wikilinks; flat route — Elysia wildcard can't suffix-match `/rename`)

#### media
- `POST /api/media/*notePath` multipart `file` → `{url, path, name}`
- `GET  /api/media-raw/*path` → binary stream

#### query
- `GET  /api/search?q=…` → `[{path, title, score, snippet, matches}]` top 50
- `GET  /api/backlinks/*path` → `[{from, lineNo, context, embed}]`
- `GET  /api/resolve?name=…` → `{path, matches, source: "path"|"basename"|"alias"|null, ambiguous}` — hierarchy: full path → basename → alias
- `GET  /api/aliases` → `Record<aliasLowercase, paths[]>` (frontmatter aliases)
- `GET  /api/tags` → `[{tag, count}]`
- `GET  /api/tags/notes?tag=X` → `paths[]` (notes containing tag — index.byTag, ! path-substring)
- `GET  /api/tasks` → `[{path, lineNo, done, text}]` aggregate tasks vault-wide

#### trash
- `GET  /api/trash` → `[{path, mtime, isDir}]`
- `POST /api/trash/restore` body `{trashPath}` → `{ok, path}`

#### git
- `GET  /api/git/status` → `{enabled, head, branch, dirty, lastCommit, autocommit:{enabled, debounceMs}}`
- `GET  /api/git/log?path=…&limit=N` → `[{sha, subject, ts, author}]` (vault-wide if no path)
- `GET  /api/git/show?sha=…&path=…` → `{content}`
- `GET  /api/git/diff?sha=…&path=…` → `{patch}` unified diff vs HEAD
- `POST /api/git/commit` body `{message?}` → `{sha|null}`
- `POST /api/git/restore` body `{path, sha}` → `{ok, sha}` (auto-commits "restore X → sha")
- `POST /api/git/checkpoint` body `{message?}` → `{sha, tag:"cp-<ts>"}`
- `POST /api/git/autocommit` body `{enabled?, debounceMs?}` → applies + persists to `.brain/settings.json`
- `POST /api/git/flush` → `{sha|null}` force-fire pending autocommit

#### settings + folder-meta
- `GET  /api/settings` → `{version:1, bookmarks, dailyDir, git:{autocommit, debounceMs}, rag:{…}, editor:{mobileDefaultView}}`
- `PATCH /api/settings` body `{bookmarks?, dailyDir?, git?:{…}, rag?:{…}, editor?:{mobileDefaultView?: "edit"|"preview"}}` → full settings
- `GET  /api/folder-meta` → `{version:1, icons, colors}`
- `POST /api/folder-meta` body `{path, icon?, color?}` → `{ok, meta}`

### CLI / env
- `brain` or `bun start` → serve `:3000`
- `brain --help` / `-h` → print usage + exit 0
- `brain --version` → print version + exit 0
- `brain --vault-dir <path>` / `-v <path>` → override vault location
- `brain --port <n>` / `-p <n>` → override HTTP port
- env (lower precedence than CLI flags):
  - `VAULT_DIR` (default = XDG path above)
  - `PORT` (default `3000`)
  - `XDG_DATA_HOME` / `XDG_CONFIG_HOME` honored on all platforms
  - `GIT_AUTOCOMMIT` = `1`/`0` (bootstrap default if no settings.json)
  - `GIT_AUTOCOMMIT_DEBOUNCE_MS` (bootstrap default)
- precedence: CLI flag > env var > XDG default
- unknown CLI flag → stderr error + exit 2
- `brain --mcp-disabled` → skip mounting MCP routes (default = MCP enabled)

### auth (HTTP) — OPTIONAL
- default: no auth. all endpoints open unless `<VAULT>/.brain/auth.json` exists.
- `GET  /api/auth/status` → `{configured:bool, authenticated:bool}`
- `POST /api/auth/set` body `{newPassword, currentPassword?}` → `{ok}` (set initial OR change; currentPassword required if already configured)
- `POST /api/auth/clear` body `{currentPassword}` → `{ok}` (removes auth.json; endpoints open again)
- `POST /api/auth/login` body `{password}` → `{token, expiresAt}` (24h)
- `POST /api/auth/logout` body `{token}` → `{ok}`
- once auth.json exists: ∀ /api/* (minus /auth/status, /auth/login) + /mcp/* require `Authorization: Bearer <token>`

### MCP server (HTTP+SSE, mounted at `/mcp/*`)
- POST `/mcp` JSON-RPC + GET `/mcp/sse` server→client streaming (MCP 2024-11-05 streamable HTTP)
- auth: same bearer token as HTTP API (only when auth.json exists)
- tools (17 total):
  - core (10): search_notes, similar_notes, read_note, list_notes, get_backlinks, list_tags, get_tasks, current_datetime, write_note, append_note
  - RAG/agent extensions (7): find_related, semantic_outline, context_for_query, find_similar_tasks, find_orphans, weekly_digest, compare_notes
- resources: `vault://tree`, `vault://note/<path>`
- instructions (V68): the `initialize` response carries server-level instructions surfaced to the model. Default is embedded (`DEFAULT_MCP_INSTRUCTIONS`) and owner-overridable per-vault at `<VAULT>/.brain/mcp-prompt.md` (seeded on first run; blank file = no instructions). Read per request, so edits apply without restart.
- ∀ tool call enforces per-folder permissions (V52); read tools need `read:true`, write tools need `write:true` on the affected note's folder chain. RAG/agent extensions gated by V54 (ragEnabled + path-keyed perm filter).

### folder MCP permissions (HTTP)
- `GET  /api/folder-mcp-perms` → `Record<folderPath,{read,write}>`
- `POST /api/folder-mcp-perms` body `{path, read, write}` → upsert one folder
- `DELETE /api/folder-mcp-perms/*path` → drop override (falls back to inherited)

### RAG API (HTTP)
- `GET  /api/similar?q=<text>&k=<n>` → `[{path, score, snippet, lineRange, headingTrail}]` (top-k cosine)
- `POST /api/rag/reindex` → `{ok, indexed, skipped, durationMs}`
- `GET  /api/rag/status` → `{enabled, provider:"local"|"openai-compat", model, dim, chunks, lastIndexedAt, needsReindex}`
- `POST /api/rag/test` body `{provider, config}` → `{ok, dim, sampleEmbedding?}` (dry-run probe before saving openai-compat settings)
- `GET  /api/related/*path?k=<n>` → top-k chunks similar to the given note (used by sidebar Related section)
- `POST /api/context` body `{q, budget?}` → packed chunks under a token budget (default 2000) — `{text, sources:[{path, lineRange, score}]}`
- `GET  /api/orphans?limit=&iso=` → isolated notes (0 backlinks + low max-cosine to anything else)
- `GET  /api/digest?since=7d` → notes from period clustered into topics `[{topic_label, paths, representative}]`

### filesystem layout
```
<VAULT_DIR>/
  Folder/
    Note.md
    .media/
      img.png
  .brain/
    index.json          # cached index (mtime-based, rebuilt incrementally)
    settings.json       # per-vault config: bookmarks, dailyDir, git autocommit
    folder-meta.json    # per-folder icons + colors + MCP perms
    trash/<ts>/...      # recoverable deletes
    auth.json           # argon2id hash (absent → no auth)
    lance/              # LanceDB tables (RAG embeddings), .gitignored
  .git/                  # autocommit history (if GIT_AUTOCOMMIT)
```

## §V INVARIANTS

V1: note path ! end `.md`. server rejects ≠.
V2: vault writes ! confined to `VAULT_DIR`. ∀ path → resolve & check prefix. traversal ⊥.
V3: media uploaded for note `<dir>/note.md` → save `<dir>/.media/<file>`. mkdir `.media` if absent. filename ! contain `/` `\` or start `.`.
V4: wikilink resolve hierarchy: full path `[[Folder/Note]]` → basename `[[Note]]` (case-insensitive) → alias from frontmatter. dup basenames → ambiguous, first wins in render, full path inserted by drag-drop.
V5: rename note → ∀ inbound `[[OldName]]` / `![[OldName]]` / aliased rewritten to `[[NewName]]` atomically (scan + patch all `.md`). section anchors preserved (`#Section`, `^block`).
V6: ∀ API mutation ! atomic. write tmp → rename. partial writes ⊥.
V7: markdown render = Obsidian flavor. ! parse: wikilinks, embeds, callouts, math, mermaid, footnotes, tasks, tags, tables, frontmatter, `==hl==`, `%%cm%%`, image dimensions `![[…|WxH]]`, heading anchors `[[Note#H]]`, raw HTML passthrough, syntax-highlighted code blocks via highlight.js (detect + ignore-missing).
V8: theme toggle persisted in `localStorage` (device-local). initial = system pref. ⊥ persist to vault.
V9: editor saves on debounce 500ms & on blur. unsaved state visible. manual commit/checkpoint ! flush save first.
V10: mobile layout ≥ 320px width, no horizontal scroll, touch targets ≥ 44px.
V11: index rebuild on startup. incremental on write. `mtime`-based. entries hold `aliases` + merged `tags` (inline + frontmatter).
V12: media drag-drop into editor → upload → insert `![[file]]` at caret position (`posAtCoords`).
V13: file ops never lose data. delete → `.brain/trash/<ts>/<path>` (recoverable via `/api/trash/restore`).
V14: search = full-text over title + body + path + tags. case-insensitive. returns top 50.
V15: frontmatter parsed YAML. malformed → show error, do not crash render.
V16: vault dir = git repo (auto `git init` on startup if autocommit enabled & `.git` absent). `.brain/`, `node_modules`, `.DS_Store`, `*.tmp-*` git-ignored.
V17: autocommit ! coalesce: debounce after last vault mutation. ≥1 staged change → commit. ⊥ commits else. flush() ! clearTimeout to prevent post-flush phantom fires.
V18: git ops ! confined to `VAULT_DIR`. ∀ paths passed to git resolved & checked. shell args ! never interpolated — use argv array. GitRepo serialises writes through async mutex to prevent index lock races.
V19: editor toolbar ∀ actions ! map to deterministic CM6 transactions. no DOM mutation. preserve undo.
V20: toolbar tooltips ! show immediately on hover/focus (no browser delay). desktop layout wraps to multi-row when overflow; ⊥ horizontal scroll (mobile fallback = scroll).
V21: editor ↔ preview scroll sync. cursor line in editor → active block in preview (`.active-block`). anchor-aware: active block lands at same viewport Y as cursor when possible. SVG connector path drawn in pane gap, both endpoints visible. loop-safe (debounced, reciprocal-fire suppressed).
V22: note row in file tree ! draggable. drop on editor → insert `[[basename]]` if basename unique vault-wide, else `[[Folder/Subfolder/Name]]`. MIME `application/x-brain-note` + text fallback.
V23: `@<query>` typed in editor → autocomplete w/ note basenames. accepting suggestion replaces `@<query>` w/ `[[<chosen>]]`. coexists w/ `[[` trigger. `startCompletion` force-fired since `@` is not a word char.
V24: embed `![[Note]]` → fetch & inline target body in preview. default-collapsed (max-height ~3em + fade). chevron toggles. recursion guard via visited-set.
V25: click inside `.embed-body` ⊥ move parent editor cursor. clicks on the embed header (outside body) still emit a line-jump for the embed location.
V26: click on preview block w/ `[data-source-line]` (not link/tag/checkbox/embed-body) → editor cursor jumps to that source line. text selection (collapsed=false) suppresses jump.
V27: image / video / audio `src` in standard markdown / raw HTML — if relative (not http/https/data:/abs/anchor) → rewritten to `<note-dir>/.media/<basename>` via `buildMediaUrl`.
V28: rendered headings h1..h6 get slug `id` attr (dedup w/ `-N`). wikilink `[[Note#Heading]]` href = `#/note/<path>#<slug>`. hash listener parses trailing `#<slug>` and scrolls editor + preview to matching heading line.
V29: frontmatter `aliases: [..]` ! resolve sources (in addition to basename). frontmatter `tags: [..]` (or `tag:`) ! merge w/ inline `#tag` into index.tags.
V30: drag from tree → linkTarget = basename if unique vault-wide, else full vault-relative path (no `.md`). Editor drop uses `linkTarget`.
V31: `==text==` (paired, single-line, no `=` inside) ! render `<mark>` in preview. `%%text%%` (inline or multi-line) ! stripped from preview. neither affects raw editor source.
V32: settings persist to `<VAULT>/.brain/settings.json` (atomic write). env vars = bootstrap default only — on load, settings.json overrides env. `bookmarks`, `dailyDir`, `git.autocommit`, `git.debounceMs` persisted per-vault. theme = device-local localStorage.
V33: tag filter view fed by `/api/tags/notes?tag=X` (index.byTag). ! filter by note path substring. tag click in preview navigates to `#/tag/<name>` which loads filtered list.
V34: folder icons selectable from catalog (~30 SVG) or custom emoji `emoji:<char>`. rendered as **badge** over base folder icon (bottom-right corner). picker persists to `.brain/folder-meta.json`.
V35: file tree row supports right-click context menu (note: Open/Rename/Delete; folder: New note / New folder / Set icon / Rename / Delete) + 3-dot button revealing same menu on hover.
V36: active line indicator: editor `.cm-activeLine` + preview `.active-block` share styling (`--bg-hover` + 1px `--accent` bottom box-shadow). SVG path connects both endpoints when both visible.
V37: outline panel = headings tree of current note. click → jump editor + preview to that heading's line.
V38: ∀ interactive overlay/menu/dialog/tooltip/popover/tabs/toolbar/select/toast ! built on Radix Primitives via `web/src/components/ui/*` wrappers. ⊥ ad-hoc DOM widgets, ⊥ raw `contextmenu`/`mousedown`-positioned popups, ⊥ hand-rolled focus traps. a11y (focus mgmt, ARIA, kbd nav, ESC, outside-click) delegated to Radix.
V39: ui wrappers follow terax-ai pattern: `forwardRef` + `cn()` class merge + `data-[state=…]` variants + `data-[side=…]` slide-in. styled w/ Tailwind tokens, theme via CSS vars, dark+light parity verified per primitive.
V40: tooltips instant (delayDuration=0 at provider), match V20. context menu = Radix ContextMenu, not 3rd-party. command palette = Radix Dialog + cmdk inside.
V41: file tree row actions (3-dot menu + folder new-note button) ! visible only on `:hover` / `:focus-within` of the row. inline duplicates of dropdown-menu items ⊥ — ∀ destructive/structural actions live solely in the 3-dot DropdownMenu (T75). label gets full row width minus chevron + icon when row idle, so deep nesting (Journal/aaa/seepdir/deep01.md) stays readable. ref: VSCode + Obsidian tree behaviour.
V42: note + folder basenames ! contain `/`, `\`, `%`, NULL byte, CR, LF, or leading `.`. server rejects 400 INVALID_NAME at create/rename/folder-create/media-upload. client validates the same set in new-note/new-folder/rename prompts before submit (inline error, ! send). vault is POSIX-only: `normalizeRel` ⊥ rewrite `\` to `/` (legitimate `\` in basenames impossible under this rule; rewrite previously silently corrupted such files). Elysia ⊥ auto-decode the `*` wildcard param — ∀ route reading a path-shaped wildcard MUST call `decodeWildcard()` (single `decodeURIComponent`) before passing to vault, so existing pathological files (e.g. `ddd%5C.md`) round-trip correctly: client encodes `%` → `%25`, server decodes once → matches disk.
V43: sidebar sections (Bookmarks, Vault, Tags, Outline, Backlinks) each collapsable independently via Radix Accordion (type="multiple"). open/closed state per-section persisted to localStorage key `brain.sidebar.<id>` (device-local, ⊥ vault). default on first load: Vault open; rest collapsed. Tags section sources from `/api/tags` (I.api), sorted by count desc; click tag → `#/tag/<name>` filter (V33).
V44: default vault location = `${XDG_DATA_HOME:-$HOME/.local/share}/brain.md/vault` on all platforms (macOS, Linux, Windows — same logic, no OS branch). default settings dir = `${XDG_CONFIG_HOME:-$HOME/.config}/brain.md/`. server resolves on startup: CLI > env > XDG default. `mkdir -p` resolved vault dir if missing (first run = empty vault, ! crash).
V45: server entry exposes `--help`/`-h`, `--vault-dir`/`-v <path>`, `--port`/`-p <n>`, `--version`. `--help` prints usage block + exits 0; unknown flag → stderr msg + exit 2.
V46: MCP write tools (write_note, append_note) gated by per-folder permissions (V52) AND CLI flag `--mcp-disabled`. ∀ MCP tool call logged: name + args summary + result code (auditing).
V47: RAG chunks stored in LanceDB at `<VAULT>/.brain/lance/` table `notes_v1` cols: `id` (path#idx), `path`, `chunk_index`, `text`, `embedding` (float32[dim]), `heading_trail`, `line_start`, `line_end`, `mtime`, `model_id`, `provider_id`. incremental upsert on note write; row delete on note delete; rename = delete+reinsert.
V48: chunking deterministic — split body on markdown paragraph boundaries (blank line), accumulate ≤512 tokens (approx via gpt-tokenizer cl100k), 64-token overlap between adjacent chunks. frontmatter excluded.
V49: embedding provider abstraction: `local` = @xenova/transformers (default model = bge-small-en-v1.5, dim=384); `openai-compat` = POST `<baseURL>/embeddings` (or `<baseURL>/v1/embeddings` if baseURL lacks `/v1`) w/ `{input, model}` JSON body + optional `Authorization: Bearer <apiKey>`. config in `.brain/settings.json` under `rag.provider`, `rag.local.{model,dim}`, `rag.openaiCompat.{baseURL, model, apiKey, dim}`. switching provider OR model_id → store rows tagged w/ old `{provider_id, model_id}` → mismatch triggers `needsReindex=true` in `/api/rag/status`.
V50: MCP startup ! block on RAG readiness — `similar_notes` returns `{error:"index building", indexed:N, total:M}` while incomplete. full-text + read tools always available.
V51: RAG state surfaced in `/api/rag/status`; Settings UI tab: enable toggle + provider select (local|openai-compat) + conditional fields (baseURL, model, apiKey) + Test button (calls `/api/rag/test`) + manual Reindex button. ⊥ persist embeddings to git (`.brain/lance/` in `.gitignore`).
V52: per-folder MCP permissions = `{read:bool, write:bool}` per folder path in `<VAULT>/.brain/folder-meta.json` under `mcp.<path>`. resolution: walk note's parent folder ancestors to root; nearest explicit override wins; absent → default `{read:true, write:true}`. applies to MCP tool calls only, ! HTTP API (HTTP gates via V53).
V53: auth OPTIONAL. when `<VAULT>/.brain/auth.json` exists, ∀ /api/* (except `/auth/status` + `/auth/login`) + /mcp/* require `Authorization: Bearer <token>`. argon2id hash (memorycost 19MiB, timecost 2, parallelism 1). password set/changed via `/api/auth/set` (current required if already configured); cleared via `/api/auth/clear` (current required). tokens in-memory, 24h TTL, lost on restart. ⊥ env-based bootstrap — Settings UI is the sole entry point.
V54: ∀ RAG-derived MCP tool (similar_notes, find_related, semantic_outline, context_for_query, find_similar_tasks, find_orphans, weekly_digest, compare_notes) is read-only — never mutates vault. ∀ checks `ragEnabled()` first → `{error:"RAG disabled", code:"RAG_DISABLED"}` when off. ∀ path-keyed result row filtered through V52 folder-perm (read) before returning to caller; cross-folder aggregations (orphans, digest) suppress paths the client can't read.
V55: tasks separately indexed in LanceDB table `tasks_v1` (cols: `id`=`<path>#L<lineNo>`, `path`, `line_no`, `text`, `done:bool`, `embedding` (float32[dim]), `model_id`, `provider_id`, `mtime`). Chunker emits one task-row per `^(\s*[-*+])\s+\[([ xX])\]\s+(.+)$` line of every note. Reindex required after enabling RAG so existing tasks land in the table. `find_similar_tasks` filters by `done` flag (open/done/all).
V56: Related sidebar section (web) queries `/api/related/*path?k=5` on note open (debounce 200ms). Shows path + heading-trail + snippet per hit. Click → openNote. Collapsible per V43, persisted under sidebar id `related`. Hidden when `!path || !rag.enabled`.
V57: ∀ field accepted by `settings.patch()` MUST appear in the `/api/settings` PATCH body schema. Elysia silently strips unknown keys → 200 + no-op. Whenever a top-level Settings field is added, mirror it in `settingsRoutes` body schema and add a round-trip regression test (PATCH → GET, value preserved).
V58: RagPipeline opens on demand. `RagStore.ensureOpen()` is idempotent; `RagPipeline.ensureRunning()` opens store + subscribes to mutations. ∀ public mutation method (indexNote, deleteNote, reindexAll) calls ensureOpen() first. Settings PATCH flipping `rag.enabled` false→true MUST call `pipeline.ensureRunning()` so toggling RAG via UI works without a server restart. Reindex MUST surface per-note skip reasons via console (≥ first 5) — silent skips made the "Indexed 0 / N skipped" mode undiagnosable.
V59: RAG failure visibility. `RagPipeline.probe()` runs a 1-text embed and updates `pipeline.lastProbeError`. Settings PATCH on `rag.enabled` false→true MUST probe right away. `/api/rag/status.lastError` exposes the latest failure. UI MUST render a persistent banner in the RAG settings panel when `lastError !== null`, including a Google search link with a scrubbed (path/PII-free) version of the error so users can self-help. Bulk reindex failures also bump `lastProbeError` so the banner reflects them.
V60: editor settings live in `.brain/settings.json` under `editor.mobileDefaultView: "edit"|"preview"`. Default `"edit"`. Web `openNote()` initializes `mobileView` from this setting when `isMobile === true`. On desktop the setting is read but unused (no preview-only mode there).
V61: Settings dialog has a dedicated "Editor" tab (Radix Tabs trigger value=`editor`). Editor tab hosts editor-side preferences; future editor settings (font size, autosave debounce, etc) extend this tab — they do not leak into AI / RAG or Appearance.
V62: `RagPipeline.applyConfig(cfg)` MUST rebuild the embedder unconditionally — comparing only providerId+modelId misses `baseURL`, `apiKey`, and `dim` changes (B17). The embedder constructor is cheap; warm-up cost is lazy in `.ready()` / first `.embed()`. Same class as V58 (RagStore.ensureOpen) and the implicit V15 "settings PATCH must reach runtime state". Whenever a SettingsStore field has a corresponding live object, the apply-path MUST rebuild aggressively, not diff-skip.
V63: OAuth 2.1 OPTIONAL alternative to V53 password auth. When enabled, the MCP authorization surface follows MCP spec 2025-11-25: PKCE S256 mandatory, audience-bound access tokens (RFC 8707 `resource` parameter), short-lived access tokens (24h) + rotated refresh tokens (7d). Scopes `vault:read`, `vault:write` map onto V52 folder permissions (folder-level perm intersects with scope; both must permit). V53 and V63 coexist: password tokens and OAuth tokens both flow through `TokenStore.validate()`; per-route choice is up to the client.
V64: OAuth discovery surface. MCP server MUST expose `GET /.well-known/oauth-protected-resource` (RFC 9728) returning `{resource, authorization_servers, scopes_supported}` and — when the embedded authorization server is on (same origin) — `GET /.well-known/oauth-authorization-server` (RFC 8414) returning issuer + `authorization_endpoint` + `token_endpoint` + `registration_endpoint` + `code_challenge_methods_supported: ["S256"]` + `client_id_metadata_document_supported: true`. Both endpoints MUST be reachable without auth (the whole point is to bootstrap auth). Resource and AS URLs MUST honour `x-forwarded-{proto,host}` so the advertised origin matches what the client sees through Cloudflare Tunnel or any reverse proxy.
V65: `WWW-Authenticate` challenge. Every 401 response from a protected path (V53 / V63) MUST include `WWW-Authenticate: Bearer resource_metadata="<origin>/.well-known/oauth-protected-resource", scope="vault:read vault:write"` per RFC 9728 §5.1. This lets MCP clients auto-discover the authorization server on a single unauthenticated round trip instead of probing well-known URIs.
V68: server-level MCP instructions. The `initialize` response MUST carry an `instructions` string when one is active. The default (`server/src/mcp/instructions.ts` · `DEFAULT_MCP_INSTRUCTIONS`) is embedded in the binary and states: brain.md is the single source of truth (look up before answering, write durable knowledge back); never guess date/time — call `current_datetime`; file notes under `private/` vs `work/`. Owner override lives at `<VAULT>/.brain/mcp-prompt.md` — seeded with the default on first load, returned verbatim when non-empty, treated as "no instructions" when whitespace-only. Loaded per request so edits apply without a restart; FS errors fall back to the embedded default. Repo-root `mcp-prompt.md` mirrors the embedded default for humans.

## §T TASKS

id|status|task|cites
T1|x|scaffold Bun+Elysia repo, bun workspace (server + web)|-
T2|x|filesystem vault adapter: read/write/list, atomic ops|V2,V6,V13
T3|x|API: tree, note CRUD, folder CRUD|I.api,V1,V2
T4|x|API: media upload + serve, `.media/` co-location|I.api,V3
T5|x|API: search full-text|I.api,V14
T6|x|API: rename w/ inbound wikilink patch|I.api,V5
T7|x|API: backlinks endpoint|I.api
T8|x|API: resolve wikilink by basename|I.api,V4
T9|x|index module: build on start, incremental on write, persist `.brain/index.json`|V11
T10|x|web shell: 3-pane desktop, drawer mobile, responsive ≥320px|I.web,V10
T11|x|file explorer tree component: expand/collapse, create/rename/delete|I.web
T12|x|markdown editor (CodeMirror 6): md syntax highlight, wikilink autocomplete, paste-image|I.web,V9,V12
T13|x|markdown renderer (Obsidian flavor) – core: CommonMark + GFM tables + tasks + footnotes|V7
T14|x|renderer: wikilinks `[[…]]` & aliased `[[x\|y]]` clickable|V4,V7
T15|x|renderer: embeds `![[note]]` (transclude) & `![[img.png]]` (media)|V7
T16|x|renderer: callouts `> [!note]/[!warn]/…` styled blocks|V7
T17|x|renderer: math KaTeX inline & block|V7
T18|x|renderer: mermaid fenced blocks|V7
T19|x|renderer: tags `#tag` clickable → filtered view|V7
T20|x|frontmatter YAML parse + show as property panel|V7,V15
T21|x|backlinks panel (per-note) below editor (later moved to sidebar)|I.api
T22|x|search UI: command bar, results list, keyboard nav|I.web,V14
T23|x|quick switcher (Ctrl/Cmd+O): fuzzy file open|I.web
T24|x|command palette (Ctrl/Cmd+P): actions|I.web
T25|x|outline panel: headings tree, jump to|I.web,V37
T26|x|bookmarks (star notes), persisted in `.brain/settings.json`|V32
T27|x|theme: CSS vars light & dark, system pref + toggle, persist localStorage|V8
T28|x|mobile UX: touch targets ≥44px, swipe drawer, sticky toolbar, no h-scroll|V10
T29|x|media: upload button in toolbar + drag-drop zone over editor|V12
T30|x|delete → trash dir, restore action|V13
T31|x|tag index + `#tag` filtered note list view|V7,V33
T32|x|tasks across vault aggregated view (default core plugin parity)|V7
T33|x|daily notes: open/create `YYYY-MM-DD.md` in configured folder|-
T34|x|settings panel: vault path display, daily notes folder, editor opts|-
T35|x|PWA manifest + service worker (offline shell, mobile install)|I.web,V10
T36|x|e2e smoke: create note, link, embed image, render, mobile viewport|V7,V10
T37|x|editor toolbar: B/I/S/H1-3, lists, tasks, quote, code, link, wikilink, image, table, math, callout|V19,I.web
T38|x|git wrapper module: init, status, add, commit, log, show, diff, restore, tag|V16,V17,V18
T39|x|autocommit pipeline: debounced after vault mutations, staged-only|V17
T40|x|git API routes: status/log/show/diff/commit/restore/checkpoint/flush/autocommit|I.git
T41|x|history panel UI per-note: list commits, click → diff & restore, scope toggle (note/vault)|I.web
T42|x|diff viewer (line-level red/green) in modal|I.web
T43|x|manual commit + checkpoint buttons in topbar (flush save first)|I.web,V9
T44|x|settings toggle: enable/disable autocommit, debounce ms|I.web,V17,V32
T45|x|toolbar UX: instant tooltips + wrap layout (no h-scroll)|V20
T46|x|editor ↔ preview scroll & active-line sync|V21
T47|x|drag note from tree → drop in editor → insert wikilink|V22
T48|x|`@` trigger autocomplete for wikilinks|V23
T49|x|folder icon picker: catalog of icons + emoji, persist to `.brain/folder-meta.json`|V34
T50|x|tag filter view backed by `/api/tags/notes` (index.byTag), not path substring|V33
T51|x|cursor-anchor scroll: active preview block lands at same Y as editor cursor|V21
T52|x|SVG sync connector overlay (bezier between cursor & active block, endpoints)|V21,V36
T53|x|embed transclusion: lazy fetch + render target body in preview, recursion guard|V24
T54|x|embed collapsible: default collapsed, chevron toggle, body max-height + fade|V24
T55|x|click in `.embed-body` ⊥ jump parent editor cursor|V25
T56|x|click preview block (data-source-line) → editor cursor jump (skip if selection)|V26
T57|x|tree context menu (right-click + 3-dot): Open/Rename/Delete + folder New/Set icon|V35
T58|x|folder icon as **badge** over base folder icon (right-bottom)|V34
T59|x|remark plugins: ==highlight== → `<mark>`, %%comment%% → stripped|V31
T60|x|embed dimensions `![[img\|W]]` & `\|WxH`: width/height applied to img/video|V7
T61|x|rehype-headingIds: slug `id` on h1..h6 + dedup. wikilink href `#slug`. hash listener jumps to heading|V28
T62|x|frontmatter aliases + tags integration: index entries hold aliases, resolve uses them; tags merged|V29
T63|x|rehype-relativeMedia: rewrite relative `src` → `<note-dir>/.media/<basename>`|V27
T64|x|drag-drop full path on ambiguous basename (basenameCounts)|V30
T65|x|syntax highlighting in code blocks (rehype-highlight, theme via CSS vars)|V7
T66|x|single shared toolbar above both panes, equal-height pane heads|I.web,V10
T67|x|sidebar header w/ Search + New note + New folder icons|I.web
T68|x|settings.json centralised (bookmarks/dailyDir/git autocommit) w/ atomic persist|V32
T69|x|GitRepo serialise writes via async mutex (prevent autocommit ↔ manual race)|V18
T70|x|IconBare (picker grid) vs FolderIconRender (tree badge) split|V34
T71|x|push initial code to `git@github.com:example-org/brain.md.git`|-
T72|x|add radix deps + tailwind v3 + class-variance-authority + clsx + tailwind-merge; `cn()` util in web/src/lib/utils.ts|V38,V39
T73|x|scaffold `web/src/components/ui/` primitives per terax-ai pattern: context-menu, dropdown-menu, dialog, popover, tooltip, tabs, toolbar, select, switch, toast, scroll-area, separator, accordion|V38,V39
T74|x|tree row right-click → Radix ContextMenu (Open/Rename/Delete; folder: New note/folder, Set icon)|V35,V38,V40
T75|x|tree row 3-dot button → Radix DropdownMenu (same items)|V35,V38
T76|x|toolbar tooltips → Radix Tooltip (delayDuration=0)|V20,V38,V40
T77|x|command palette (Cmd/Ctrl+P) → Radix Dialog + cmdk|T24,V38,V40
T78|x|quick switcher (Cmd/Ctrl+O) → Radix Dialog + cmdk|T23,V38
T79|x|settings panel → Radix Dialog + Tabs (vault / editor / git / appearance)|T34,T44,V38
T80|x|history panel + diff viewer → Radix Dialog + ScrollArea|T41,T42,V38
T81|x|folder icon picker → Radix Dialog + ScrollArea (catalog grid + emoji input). spec said Popover but no anchor available; picker invoked from context menu — Dialog preserves UX. revisit if "Set icon" moves inline.|T49,V34,V38
T82|x|switches (autocommit) → Radix Switch. theme = native select, no boolean override needed yet|T27,T44,V38
T83|x|mobile editor/preview tab switch → Radix Tabs|T28,V38
T84|x|toast notifications → Radix Toast (save errors, restore confirm, etc.)|V38
T85|x|editor toolbar shell → Radix Toolbar (root + groups + separators); actions still emit CM6 tx|T37,V19,V38
T86|x|a11y pass: ARIA labels on all icon-only buttons, Icons get aria-hidden+focusable=false, IconBtn helper wraps topbar icons w/ Tooltip+aria-label, ThemeButton+IconPicker labels. axe smoke deferred (no browser harness here)|V38
T87|x|visual pass: compact density on menu items/tabs (py-1.5 → py-1) for desktop-app feel. hairline borders + shadows already token-driven. dark+light parity unchanged. deeper polish (sidebar/topbar tightening, refined accent intensity) deferred until browser-driven review|V39
T88|x|remove legacy widget code: hand-rolled context menu positioning, custom tooltip, custom modal backdrops, custom focus trap|V38
T89|x|sidebar: 5-section Radix Accordion wrapper (Bookmarks, Vault, Tags, Outline, Backlinks); type="multiple", default Vault open|I.web,V38,V43
T90|x|useSidebarSection hook: localStorage-backed open/closed per section id; read on mount, write on toggle|V43,V8
T91|x|Tags sidebar section: list `[{tag,count}]` from /api/tags sorted by count desc, click → `#/tag/<name>`|I.api,V33,V43
T92|x|section headers match terax-ai pattern: compact row, ChevronDown rotate, hover bg, uppercase label like existing OUTLINE / BACKLINKS|V39,V43
T93|x|XDG path resolver: `getDefaultVaultDir()` + `getDefaultSettingsDir()` in server/src/config/paths.ts using XDG vars w/ HOME fallback, no OS branching|V44
T94|x|CLI parser in server/src/cli.ts: --help/-h, --vault-dir/-v, --port/-p, --version. unknown flag → exit 2|V45,I.cli
T95|x|server entry wires precedence (cli > env > XDG default) for VAULT_DIR + PORT. mkdir -p vault on first run|V44,V45,I.cli
T96|x|update README.md: XDG default paths + CLI usage block|V44,V45
T97|x|add deps: @modelcontextprotocol/sdk + @lancedb/lancedb + @xenova/transformers + gpt-tokenizer (argon2 dropped — using Bun.password built-in)|V46,V47,V49,V53
T98|x|RAG module scaffold: server/src/rag/{chunker,embedder-local,embedder-openai,provider,store,types}.ts|V47,V48,V49
T99|x|chunker.ts: paragraph-based ≤512-token chunks w/ 64-token overlap; preserves heading trail + line range; excludes frontmatter|V48
T100|x|local embedder: bge-small-en-v1.5 via @xenova/transformers; lazy-load model on first call; batch encode|V49
T101|x|openai-compat embedder: POST `<baseURL>/embeddings` (auto-append `/v1` if absent) w/ `{input, model}`, optional Bearer apiKey; handle 4xx/5xx w/ typed errors|V49
T102|x|store.ts: LanceDB wrapper — open/create table notes_v1, upsert(rows), deleteByPath(path), search(vec,k), countAll(), distinctProviderModel()|V47
T103|x|RAG pipeline: indexNote / deleteNote / renameNote hook into vault.onMutation; provider chosen per settings.rag.provider|V47,V48,V49
T104|x|initial index build on startup if rag.enabled && store.count==0; non-blocking|V47,V50
T105|x|GET /api/similar?q=&k= route|I.api,V47
T106|x|GET /api/rag/status route (incl. provider + needsReindex flag)|I.api,V51
T107|x|POST /api/rag/reindex route|I.api,V47
T108|x|POST /api/rag/test route — dry-run embed sample text w/ passed-in config (no save); useful before applying openai-compat settings|I.api,V49,V51
T109|x|settings.json rag schema: `{enabled, provider:"local"|"openai-compat", local:{model,dim}, openaiCompat:{baseURL,model,apiKey,dim}}`|V49
T110|x|Settings UI tab "AI / RAG": enable toggle (Switch), provider select (Select), conditional fields, Test button, Reindex button, status pill|I.web,V49,V51
T111|x|`.brain/lance/` covered by existing `.brain/` pattern in vault `.gitignore` (server/src/git/git.ts:21); no change needed|V51
T112|x|auth module: server/src/auth/{hasher,tokens,store}.ts — argon2id hash via Bun.password, in-memory token map, 24h ttl|V53
T113|x|auth routes: GET /api/auth/status, POST /api/auth/set, POST /api/auth/clear, POST /api/auth/login, POST /api/auth/logout|I.api,V53
T114|x|auth middleware: no-op when auth.json absent; otherwise enforce Bearer on /api/* (except /auth/{status,login}) + /mcp/*|V53
T115|x|client auth: login Dialog when status.configured && ! authenticated; token in localStorage; fetch wrapper adds Authorization header|I.web,V53
T116|x|Settings UI tab "Security": set / change / remove password buttons; status display ("auth off" / "auth on")|I.web,V53
T117|x|folder perms data model: extend folder-meta.json schema `{icons, colors, mcp:{<path>:{read,write}}}` + `resolveFolderPerms(notePath)` helper|V52
T118|x|folder perms routes: GET / POST /api/folder-mcp-perms, DELETE /api/folder-mcp-perms/*path|I.api,V52
T119|x|folder perms UI: folder context-menu item "MCP permissions…" → Radix Dialog w/ read + write Switches + "reset to inherited" button|I.web,V52
T120|x|MCP server entry: server/src/mcp/server.ts using @modelcontextprotocol/sdk HTTP+SSE transport (WebStandardStreamableHTTPServerTransport); mount on Elysia at /mcp + /mcp/sse. File uses @ts-nocheck to dodge SDK generic stack overflow|I.mcp,V46
T121|x|MCP tool impls: 9 tools (search_notes, similar_notes, read_note, list_notes, get_backlinks, list_tags, get_tasks, write_note, append_note); ∀ read/write tool enforces V52 folder perm check before vault op; all calls logged to stderr (audit)|I.mcp,V46,V52
T122|x|MCP resources: vault://tree (filtered by read perm) + vault://note/<+path> via ResourceTemplate|I.mcp,V52
T123|x|CLI `--mcp-disabled` flag wired into cli.ts + AppOptions; default = MCP mounted|I.cli,V46
T124|x|README + docs/mcp.md: Claude Desktop config snippet for HTTP+SSE w/ optional Bearer token|V46,V53
T125|x|chunker: extract task lines (`^(\s*[-*+])\s+\[([ xX])\]\s+(.+)$`) into separate TaskChunk[] alongside body chunks; share frontmatter-strip + line-offset logic|V55,V48
T126|x|store: second table `tasks_v1` w/ schema {id, path, line_no, text, done, embedding[dim], model_id, provider_id, mtime}; openTasks(), upsertTasks(), deleteTasksByPath(), searchTasks(vec, k, doneFilter)|V47,V55
T127|x|pipeline: indexNote() also walks task lines → embed → upsertTasks(); deleteNote() → deleteTasksByPath(); reindexAll() rebuilds both tables|V55,V47
T128|x|MCP tool `find_similar_tasks(query, k?, filter?)`: filter ∈ {open,done,all,default=open}; returns `[{path, lineNo, text, done, score}]`|I.mcp,V54,V55
T129|x|MCP tool `find_related(path, k?)`: read note → embed first chunk (or full body, capped) → store.search excluding rows where path == query path|I.mcp,V54
T130|x|MCP tool `semantic_outline(path)`: chunk note → embed all → cluster by cosine threshold (≥0.7) → return tree {heading_trail, representative_snippet, chunk_count}|I.mcp,V54
T131|x|MCP tool `context_for_query(q, budget_tokens=2000)`: embed q → top-k=20 chunks → greedy-pack (dedupe by path, prefer top score) until gpt-tokenizer count > budget; return concatenated text + source[]|I.mcp,V54
T132|x|MCP tool `find_orphans(limit?=10, minIsolation?=0.35)`: enumerate notes w/ 0 backlinks (via VaultIndex) AND max-cosine-to-others < (1−isolation); surface as "isolated topics"|I.mcp,V54
T133|x|MCP tool `weekly_digest(since="7d")`: filter notes by mtime; embed all chunks; cluster (cosine ≥0.6, agglomerative); return top-N clusters w/ {topic_label, paths, representative_snippet}|I.mcp,V54
T134|x|MCP tool `compare_notes(a, b)`: load both → embed first chunk of each → cosine sim; return {cosine, unifiedDiff, sharedHeadings:[]}|I.mcp,V54
T135|x|HTTP routes: GET /api/related/*path, POST /api/context, GET /api/orphans, GET /api/digest — thin wrappers over the same RAG functions used by the MCP tools|I.api,V54
T136|x|Web: sidebar "Related" Accordion item; uses useSidebarSections("related"); debounced fetch on path change; renders hit list w/ heading-trail breadcrumb|I.web,V43,V56
T137|x|Web: tag suggestion overlay at save — pop a small toast w/ top-3 tags derived from semantic neighbors' frontmatter `tags`; click adds to current note's frontmatter|I.web,V54
T138|x|extract `server/src/rag/queries.ts` shared by MCP tools + HTTP routes (related, context, orphans, digest, compare, outline, tasks); single place for ranking + perm-filtering + budget logic|V54,V52
T139|.|extend `Settings` + `SettingsPatch` types + `mergeEditor()` in `server/src/settings/settings.ts`; default = `{mobileDefaultView:"edit"}`|V60,V57
T140|.|extend `/api/settings` PATCH body schema with `editor: t.Optional(t.Object({mobileDefaultView: t.Optional(t.Union([t.Literal("edit"), t.Literal("preview")]))}))`. Round-trip regression test PATCH→GET|V57,V60
T141|.|Settings dialog — `<TabsTrigger value="editor">Editor</TabsTrigger>` + `<TabsContent value="editor">` with `<Field label="Mobile default view">` select {Edit, Preview}. Saves via PATCH /api/settings|I.web,V61
T142|.|App.tsx `openNote()`: when `isMobile` true, initial `mobileView` = `settings.editor.mobileDefaultView`. Settings fetched on mount alongside other settings|I.web,V60

## §B BUGS
id|date|cause|fix
B1|2026-05-24|Elysia wildcard `/api/note/*/rename` greedy — can't suffix-match|§I split → flat `POST /api/rename` body `{from,to}`
B2|2026-05-24|`git log -50 --follow -- path` filter returns nothing if path absent in HEAD — UI showed only "initial"|HistoryPanel scope toggle "this note / all vault"; manual commit flushes editor save first; restore commits explicitly
B3|2026-05-26|autocommit timer fires concurrently w/ manual commit → git index lock contention → 500|GitRepo writeChain mutex; Autocommit.flush clearTimeout. §V17 + §V18
B4|2026-05-26|scroll sync used `el.offsetTop` (offsetParent=body) — preview scrolled to absolute page Y, not container scrollTop|getBoundingClientRect math: `container.scrollTop + (elRect.top − cRect.top)`
B5|2026-05-26|tag click filtered notes by path substring — "no notes with this tag" even when tag present in body|new `/api/tags/notes?tag=X` using `index.byTag`. §V33
B6|2026-05-26|embed `![[Note]]` rendered only header (title), no body|rehype lazy-fetch + inline render of target. recursion guard. §V24
B7|2026-05-26|`@` autocomplete didn't fire — CM6 only auto-triggers on word chars; validFor excluded `@` so popup closed instantly|update listener inspects last typed char; force `startCompletion`. `validFor` regex includes `@`. §V23
B8|2026-05-26|click inside transcluded embed body jumped parent editor cursor to a wrong line (embed body's data-source-line ≠ parent line)|click handler aborts if ancestor is `.embed-body`. §V25
B9|2026-05-26|deeply-nested folder rows (Journal/aaa/seepdir/deep01.md) truncated labels to "deep…" — always-visible 3-dot + pencil + trash + new-note inline buttons consumed row width. VSCode shows the same vault clean.|hide `.tree-actions` until `:hover` / `:focus-within`; remove inline pencil/trash/new-note (already in 3-dot DropdownMenu since T75) so only the 3-dot button surfaces on hover. §V41
B10|2026-05-27|new-note prompt accepted `\` (or `/`) in basename; client/server passed it through; `normalizeRel` rewrote `\`→`/` so the path became traversal-like (e.g. `ddd\.md` → `ddd/.md`); meanwhile a separate encoding step had landed `%5C` literal on disk as `ddd%5C.md`. Every Open/Rename/Delete failed because the rewritten path no longer matched the on-disk filename. Additionally Elysia did NOT URL-decode the `*` wildcard param, so even after the first patch the server saw `ddd%255C.md` literal instead of `ddd%5C.md` — 404.|drop the `\→/` substitution in `normalizeRel`. Add `assertSafeBasename` (server) + `validateBasename` (client) per V42. Add `decodeWildcard()` helper called in every route reading a path-shaped `*` param (notes, folders, backlinks, media, media-raw) so client-encoded `%25` round-trips back to `%`. §V42
B11|2026-05-28|`/api/settings` PATCH body schema listed only bookmarks/dailyDir/git, so Elysia's validator silently stripped the `rag` field from the payload. Endpoint returned 200 + correct shape, but `settings.patch()` never saw rag.enabled — the UI Switch flipped client-side then snapped back on the next GET. Identical breakage for provider/local/openaiCompat.|Add `rag` to the body schema (optional, full shape). Regression test PATCH→GET for both `rag.enabled` and nested `openaiCompat`. New invariant §V57 forbids future schema-drift between SettingsPatch and the route validator.|§V57
B12|2026-05-28|Toggling Enable RAG ON in Settings showed "Indexed 0 notes (17 skipped) in 10ms" and nothing worked. The RagStore is only opened at startup if `loaded.rag.enabled === true`; flipping the flag at runtime left the store closed, so `indexNote()` threw "RagStore not opened" for every note — caught by reindexAll's try/catch and silently counted as "skipped".|`RagStore.ensureOpen()` + `RagPipeline.ensureRunning()` (both idempotent). reindexAll/indexNote/deleteNote demand-open the store. Settings PATCH calls `ragPipeline.ensureRunning()` when rag.enabled flips false→true and `applyConfig()` on every change. reindexAll now logs per-note skip reasons (≥5). §V58|§V58
B13|2026-05-28|Outline sidebar rendered every heading on top of the others — overlapping unreadable rows. The Outline component used `className="h-${level}"` for level-1..6, which clashes with Tailwind's height utilities (`h-1` = 4px … `h-6` = 24px), crushing every row to ≤24px. The `.outline .h-N` CSS only set `padding-left`, never overrode `height`.|Rename to `lvl-1`..`lvl-6` in both `Outline.tsx` and `panels.css`. Lesson: do NOT pick class names that match Tailwind utility shapes (`h-N`, `w-N`, `m-N`, `p-N`, `text-N`, `bg-N`, etc).|—
B14|2026-05-28|Prebuilt brainmd binary on Linux threw `libonnxruntime.so.1.14.0: cannot open shared object file` for every note on first reindex with the default Local (Xenova) provider. Reason: `@xenova/transformers` depends on `onnxruntime-node`, which loads a platform-specific .so/.dll/.dylib at runtime; `bun --compile` doesn't bundle those native artefacts.|`LocalEmbedder.ready()` now catches the ONNX load error and rethrows with an actionable hint ("switch the RAG provider to OpenAI-compatible and point it at Ollama"). README documents the workaround. Long-term fix is shipping the prebuilt ONNX binding next to each release binary OR defaulting prebuilt binaries to openai-compat; tracked for v0.3.|—
