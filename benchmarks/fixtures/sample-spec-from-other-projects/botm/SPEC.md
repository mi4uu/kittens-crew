# SPEC

## §G GOAL
KeyMarley key server + Binance tunnel always-on as service on vps box; Botmarley main instance always-on @ `https://botmarley.example.com`; every instance verifies license vs keymarley over HTTPS.

## §C CONSTRAINTS
- Rust. reuse crates: `keymarley`, `core`, `server`, `shared`. ⊥ rewrite.
- single-tenant, no backward-compat burden. real bug → fix in plan (§T), ⊥ legacy support.
- key project. progress & info tracked in brain.md `work/BotMarley.md` → keep updated on milestones.
- KeyMarley = Axum + SQLite + Ed25519. relocate bob → vps.
- license verify = Ed25519 pubkey `PUBLIC_KEY_BYTES` in `crates/shared/src/crypto.rs`. pubkey verbatim. signing key offline only.
- Binance tunnel built: `SshTunnelManager` in `crates/core/src/ssh_tunnel.rs`, SOCKS5. ⊥ rebuild.
- front both svcs via Cloudflare Tunnel (cloudflared ingress). public host → local port:
  - `keymarley.example.com` → `http://localhost:8000`  (keymarley, already routing)
  - `botmarley.example.com` → `http://localhost:5000`  (main instance)
- vps host: `root@vps01.example-host.net -p 2222`.
- both svcs always-on → survive reboot & crash (systemd | vps equivalent).
- endpoints config-driven (fronted by cloudflared) → host swap ⊥ recompile.
- project repo: `https://github.com/example-org/botmarley`
- docs = separate repo `https://github.com/example-org/botmarley-book`, end-user focused, GitHub Pages book → `https://example-org.github.io/botmarley-book/`. ⊥ in this repo. ! always current.
- trading modes: backtest, paper, live. shared eval core ! identical: `evaluation.rs`, `portfolio.rs`, `executor.rs`, `indicators.rs` (all `crates/core/src/backtest/`).
- exchanges: Binance + Kraken both real exec. `AccountType` enum {Paper,Kraken,Binance} dispatch. strat logic exchange-agnostic.
- strat model today = single-pair trigger (`indicator`/`operator`/`target`/`timeframe`, `max_open_positions`). ⊥ cross-asset rotation.
- both hosts run cloudflared; ingress per supplied config (host→domain→port). host ? resolved.
- demo instance: independent & separate, host `ssh server`, cloudflared → `botmarley-demo.example.com`. public, always-available ∀ (anon, no license gate). own db/state. paper-only ? TBD.
- NEW capability: GEM / dual-momentum (portfolio rotation across asset universe). currently ⊥ supported, ! add.

## §I INTERFACES
- svc: botmarley main → `https://botmarley.example.com` (cloudflared → localhost:5000)
- svc: keymarley → `https://keymarley.example.com` (cloudflared → localhost:8000), always-on
- svc: botmarley-demo → `https://botmarley-demo.example.com` (host `ssh server`, cloudflared), independent instance
- api: client verify → POST keymarley `{date, api_key_hash}` → signed resp `{signature_b64}`
- tunnel: SOCKS5 SSH → `root@vps01.example-host.net:2222` → Binance API (built)
- key: Ed25519 pubkey `PUBLIC_KEY_BYTES`, `crates/shared/src/crypto.rs`
- client: `crates/server/src/license.rs` → verify + local cache + grace period
- docs: book repo `github.com/example-org/botmarley-book` → published `https://example-org.github.io/botmarley-book/` (GitHub Pages) → end-user
- mode: backtest | paper | live → ! same results ∀ {same candles, strat, cfg}
- exchange: Binance | Kraken → same eval & same results
- parity test: `crates/core/tests/parity_evaluation.rs`
- engines: backtest `crates/core/src/backtest/engine.rs`, live `crates/core/src/trading/engine.rs`
- strat types: per-pair trigger (built) + rotation/dual-momentum (NEW, GEM)
- GEM: asset universe + ROC lookback relative rank → hold winner; absolute gate vs cash/stable; monthly rebalance (configurable)

## §V INVARIANTS
V1: ∀ Botmarley instance → license verify vs keymarley before access (gate mw)
V2: keymarley reachable over public HTTPS (cloudflared) ∴ verify ⊥ depend on SSH tunnel up
V3: license resp Ed25519-signed → client verify vs `PUBLIC_KEY_BYTES` ! pass
V4: keymarley svc & binance tunnel ! auto-restart on crash & reboot
V5: Ed25519 signing key ⊥ on any deployed host (offline gen only)
V6: cached license + grace → instance survives transient keymarley outage
V7: Binance key requests ! route via SSH tunnel to vps (IP stability)
V8: keymarley url & tunnel target ! config-driven (⊥ hardcode) → host swap behind cloudflared ⊥ recompile
V9: ∀ shipped feature | infra change → docs repo updated same change ∴ docs ! always current
V10: backtest = paper = live → identical results ∀ {same candles, strat, cfg}
V11: exchange-agnostic → Binance | Kraken same eval & same results (exec decoupled from strat)
V12: portfolio state {balance, positions, PnL, fees} tracked accurate ∀ mode & exchange
V13: divergences D1-D6 closed | explicit cfg (fee, slippage, cross-pair, LOT_SIZE)
V14: rotation strat → ∀ rebalance pick max relative momentum (ROC lookback) over universe
V15: absolute gate → winner momentum ≤ risk-free|0 → hold cash/stable (⊥ forced into market)
V16: rebalance cadence configurable (default monthly), low turnover; GEM result obeys V10 (backtest=paper=live)
V17: demo instance independent → own state/db, ⊥ shares data with prod, always-on on `ssh server`
V18: demo public reachable ∀ (anon, no auth/license) → uptime check ! green; documented in book
V19: demo mode → ⊥ accept real exchange api_key/api_secret; attempt → ⊥ save + UI demo notice
V20: demo mode → ∀ settings writes (settings save, password set/remove) blocked; page visible read-only + notice
V21: backtest ⊥ look-ahead → indicator/value at tick T uses only data closed ≤ T (multi-TF = last closed bar). [fixed: expand_to_1m_ticks]
V22: backtest fills ≡ live fills → same price (next-candle open, ⊥ signal-close), same fee base, same LOT_SIZE/step_size/min_qty/min_notional rounding+rejection, same quote→USD for cross-pair. backtest ⊥ more favourable than live.
V23: metrics honest → ⊥ result < -100% for spot long-only; grade on REALISED (⊥ mark-to-market paper); benchmark pays same fees; displayed equity = engine's actual equity. ⊥ clairvoyant/paper metric shown as real.
V24: trigger threshold may be ATR-scaled → value `±k*atr[_period]` (sign=direction, period default 14, tf from trigger's timeframe). resolved ONCE at position entry: threshold% = k · ATR(closed bars ≤ entry tick) / entry_price · 100, then FROZEN for position life (⊥ recompute per tick → stable target). warmup (< period closed bars) → trigger inert (⊥ fire). reference point unchanged (pos_price_change from entry/avg, follow from peak/trough; ATR sets magnitude only). obeys V21 (⊥ look-ahead) + V22 (backtest ≡ live). fixed `%` triggers unchanged (full back-compat). edge claim valid only if ONE shared multiplier holds positive out-of-sample across ≥4 pairs (walk-forward) — else ATR = relocated overfit, reject. OPT-IN `_live` token (e.g. `-3*atr_1h_live`) → recompute ATR at CURRENT tick even on a position trigger (DCA/TP adapt to current pace) instead of entry-freeze; default = frozen.
V25: portfolio multi-pair mode (shared pool) — opt-in (≠ isolated equal-weight run_multipair, which stays). ONE shared cash + ONE positions list, each position pair-tagged. all universe pairs evaluated on a unified 1m timeline; a pair is evaluated only on ticks where it HAS a candle (own IndicatorCache, no look-ahead V21). opens draw from the SHARED cash, sized by the action's `amount` exactly as single-pair (`20 USDC` or `% of shared cash`). `max_open_positions` caps TOTAL open positions across ALL pairs; optional `max_per_pair` caps per pair. contention = first-come: at a tick, pairs processed in universe order; the first to claim the last free slot/cash wins → deterministic (V22). DCA/sell/follow triggers act ONLY on that pair's own positions; fills at that pair's next open. obeys V21 + V22 + V10. universe of 1 ≡ single-pair engine (parity).
V27: multi-pair backtest metrics honest (V23 for universe runs) — a multi-pair run (`pair` = `A+B+C…`) MUST show a Buy&Hold benchmark + Max Drawdown + CAGR/Sharpe, not n/a / 0.00. B&H = EQUAL-WEIGHT hold of the universe: split initial capital 1/N across the N pairs at the window's first close, pay the same per-fill fee as the strategy, mark each pair at its own close, net a round-trip fee (fair alpha, V23). Equity curve reconstructed HONESTLY over a daily timeline from the stored actions: per-pair crypto = Σ signed `amount_crypto` (open/buy +, sell −) of that pair up to t; shared usd = latest action's `usd_balance` ≤ t; equity(t) = usd + Σ_pair crypto_pair·close_pair(t) (each pair at ITS OWN price — never one pair's price applied to all). terminal equity anchored to the engine's `final_balance_usd` (T54). MDD/CAGR/alpha/grade then come from this curve via `metrics::compute`. Retroactive: computed at render from stored actions + reloaded candles, no migration, works on existing runs. Single-pair path unchanged.
V26: preemptive eviction (shared-pool only, opt-in via `[eviction]`) — capital-velocity policy: a fresh open blocked ONLY by the GLOBAL `max_open_positions` may evict an existing open position to free its slot, so fast-cycling pairs aren't starved by long-held bags. Trigger point = a would-open (open_long triggers fire) rejected by the global cap; that blocked open IS the contention signal (no new TOML trigger). Eligibility: a victim must be held ≥ `min_hold` (duration). Victim choice = `policy`: `smallest_loss` (max mark-to-market P&L, min booked loss — WARN: keeps deep bags), `oldest` (min opened_at_tick), `worst_laggard` (most-underwater per day held = min unrealised%/hold_days). Victim may be ANY pair (shared pool); forced full-close fills at the VICTIM pair's next open with the same fee/rounding as a normal sell (V22), books REALISED P&L, emits a pair-tagged sell action (status_reason "evicted: slot contention"). Per-pair cap (`max_per_pair`) is NEVER preempted (eviction frees a global slot only; if the contender is at its own per-pair cap, no eviction). Default OFF → existing shared-pool behaviour unchanged. universe=1 has no contention → single-pair path byte-identical (parity V10/V22 holds). cash conserved (no money created); global cap still never exceeded post-eviction.

## §T TASKS
id|status|task|cites
T1|x|provision vps `vps01.example-host.net:2222` always-on host|-
T2|x|relocate keymarley svc bob → vps, persist & auto-restart|V4,I.svc
T3|x|expose keymarley via cloudflared `keymarley.example.com` → localhost:8000|V2,I.svc
T4|x|repoint `crates/server/src/license.rs` client → `https://keymarley.example.com`|V1,V3,I.api
T5|x|verify Binance SSH tunnel → vps 2222 live|V7,I.tunnel
T6|x|main instance always-on @ `https://botmarley.example.com` (cloudflared → localhost:5000)|V4,I.svc
T7|x|e2e: instance verify vs keymarley + grace fallback on outage|V1,V6
T8|x|make keymarley url & tunnel endpoint config-driven (no hardcode)|V8
T9|x|wire docs publish: `botmarley-book` repo → GitHub Pages `example-org.github.io/botmarley-book/`|V9,I.docs
T10|x|doc infra in book: keymarley setup, cloudflared, tunnel, license, install|V9,I.docs
T11|x|audit: map done & verify works — backtest/paper/live × Binance/Kraken|V10,V11,V12
T12|x|fix D3 cross-pair (CRITICAL): quote-currency-aware pricing in backtest executor|V10,V13
T13|x|fix D1 fee (HIGH): per-trade fee deduct in backtest, cfg rate default 0.1% `engine.rs:213`|V10,V13
T14|x|fix D2 slippage cfg (bps) + D4 LOT_SIZE/step_size/min_qty cfg in backtest|V10,V13
T15|x|portfolio state correctness ∀ mode & exchange (balance, positions, PnL, fees)|V12
T16|x|extend parity test `parity_evaluation.rs` → cover D1-D6 & Binance≡Kraken|V10,V11,V13
T17|x|design rotation/portfolio strat type (multi-asset universe) ≠ per-pair trigger|V14,I.strat
T18|x|impl relative momentum rank (ROC lookback) + absolute cash gate + cfg|V14,V15
T19|x|impl rebalance cadence (monthly/configurable) in backtest engine; live engine = follow-on|V16
T20|x|GEM example strat toml + backtest validate (synthetic multi-asset; real crypto fixtures = follow-on)|V14,V15,V16,V10
T21|x|doc GEM/dual-momentum strat in book|V9,I.docs
T22|x|deploy independent demo instance on `ssh server` → `botmarley-demo.example.com`|V17,I.svc
T23|x|demo public, anon access (no license gate); doc in book + link from landing|V18,I.docs
T24|x|uptime check on demo (health endpoint + monitor) → verify always-available|V18
T25|x|demo mode blocks real exchange keys (create/update account) + UI demo notice|V19,V18
T26|x|demo mode: all settings read-only (block save + password); UI notice; demo url in README|V20,V18,I.docs
T27|x|gem_backtest CLI: load arrow universe, align, run rotation, report CAGR/MDD/equity|V14,V15,V16,V10
T28|x|core rotation_service: run_rotation_report (load+align+run+metrics+benchmarks+equity ts)|V14,V15,V16
T29|x|server: /rotation list + POST /rotation/{id}/backtest (blocking run → render)|V14,I.svc
T30|x|UI: rotation list + results page (metrics + benchmark table + equity chart)|V14,I.docs
T31|x|built-in GEM strategies: multi-pair universes + varied windows (strats/builtin)|V14,V16
T32|x|backtest result: split realised vs unrealised PnL + buy&hold benchmark + alpha + open-pos warning|V12,V10
T33|x|backtest list: realised + unrealised columns, sortable (SQL LATERAL, no backfill)|V12,V10
T34|x|clear A–F strategy rating (beat buy&hold + realised) on list + detail; GEM data on demo|V12,V10
T35|x|rotation editor: create/edit GEM in UI; trigger editor redirects rotation (no [rotation] strip)|V14,I.svc

## §B BUGS
id|date|cause|fix
B1|2026-06-08|deploy copied only the server binary; runtime tree (templates/static/strats) loaded from WorkingDirectory via minijinja path_loader was never synced → prod/demo served Mar-23 templates, hiding GEM, realized columns, ratings, universe editor|bin/deploy_native.sh always rsyncs templates/static/public/strats into ~/Services/botmarley before restart; V-note: a deploy is incomplete unless the runtime tree is synced, not just the binary
B2|2026-06-11|live/paper sessions never traded (candles_processed=0 for ~19h): the engine waits on the DataIngestionService broadcast for its pair, but the pair was never registered for ingestion. `merge_active_sessions` (meant to register active-session pairs at boot) ALWAYS errored — it queried `config->>'pair'` (no such column; flat `pair` column) WHERE `status IN ('Running','Paused')` (status stored lower-case 'running'). Error swallowed (warn) → registry only had seeded BTC/ETH → engine starved on a different pair (ADA/USDC). Also no runtime registration: a session started after the service was live could never be fed.|(1) fixed query → `SELECT DISTINCT pair FROM trading_sessions WHERE lower(status) IN ('running','paused')`. (2) DataIngestionService.registry now RwLock + `ensure_pair()`; task_executor calls it on session start so candles flow within one tick, no restart. V-note: starting a session MUST register its pair for live ingestion (boot-merge for recovered sessions + runtime ensure_pair for new ones)
B5|2026-06-15|SILENT no-op #2 (same session): paper sessions on `_USDC` / `_BNB` pairs (the whole All-Weather basket + drift ratios like XRP/BNB) routed live candle fetching to KRAKEN via `CandleFetcher::is_binance_pair`, which only inspected the BASE asset against a hardcoded list and ignored the QUOTE. Kraken has no USDC/BNB markets → self-fetch returned nothing → no live candle ever arrived → main loop never ran → candles_processed=0, UI "waiting for indicator data" forever. Note: the ingestion service WAS pulling XRP/USDC from Binance; only the engine's own fetcher was mis-routed.|`is_binance_pair` now routes by quote first: quote ∈ {USDC, BNB} ⇒ Binance (Kraken quotes USD/USDT/EUR/XBT), then the existing base-asset list. V-note: paper exchange routing must consider the quote, not just the base; a session that can't source candles must surface, not idle silently.
B6|2026-06-16|SILENT no-op (recurrence of B4 class): live/paper session `0393016a` (LINK_USDT, Avalanche, BB(30)@4h needs ~7440 1m candles) stuck on "waiting for indicator data" — `Failed to build indicator cache ... Bollinger Bands: need 30, have 6` every ~60s. TWO causes: (1) the re-bootstrap guard was `if buffer.len() < MIN_CANDLE_BUFFER` (500); a high-TF strat loads a buffer that clears 500 yet is far below its warm-up need (7440) → bootstrap skipped → BB/RSI never compute. (2) the RECOVERY spawn path never `ensure_pair(own_pair)` for ingestion (only the entry-filter ref), so a recovered session could fall to self-fetch and stall on a thin buffer.|(1) guard now `buffer.len() < strategy.warmup_1m_candles().clamp(MIN, MAX)` → bootstraps until the buffer can actually warm the indicators (or hits the cap). (2) recovery now `ensure_pair(&pair)` like task_executor. Verified live: 0393016a bootstrapped a 30-day buffer, 0 indicator failures after. V-note: the live bootstrap/re-bootstrap threshold MUST be the strategy's warm-up need, not a flat floor; every recovered session MUST register its own pair for ingestion.
B4|2026-06-15|SILENT no-op: a live/paper session on a high-timeframe long-lookback strategy (All-Weather = EMA-200 @4h, needs ~33 days warm-up) on a pair WITHOUT local Arrow history showed status "running" but candles_processed=0 forever — "waiting for indicator data". Cause: the bootstrap fetched a FIXED 30-day window with intervals capped at 1h, strategy-agnostic, so EMA-200@4h (needs >30 days of 4h bars) never had enough data. `exceeds_buffer_limit` even detected it but only WARNED. Worst kind of bug: pretends to work.|Bootstrap depth now DERIVED from `strategy.warmup_1m_candles()` (+50% margin, floor 30d); fetch a resolution ladder (1d/4h/1h/15m/5m/1m up to the strategy's `coarsest_timeframe_minutes()`, never coarser — aggregation can't split a coarse candle), each interval pulled from its own recent window so all reach now. V-note: live bootstrap MUST cover the strategy's actual indicator warm-up span, not a fixed window; a strategy that can't warm up must be surfaced, never silently idle. Refactored the 3 duplicated warm-up loops into `warmup_1m_candles()`.
B3|2026-06-15|backtests of any shorting strategy (e.g. drift_ratio_meanrev) failed with a swallowed "db error". The §U6 short side added the `open_short` action type but the `backtest_actions.valid_action_type` CHECK still only allowed ('open_long','buy','sell'), so inserting an open_short action violated the constraint. Hidden because `format!("{}", tokio_postgres::Error)` prints just "db error" and drops the DbError source detail.|Widened the CHECK to include 'open_short' (schema.sql + idempotent DROP/ADD migration in create_schema for existing DBs); surfaced the PG error via `.source()` in the insert map_err. V-note: any new ActionType variant that can be persisted MUST be added to every action_type CHECK constraint (backtest_actions today). Verified: drift_ratio_meanrev XRP_BNB backtest now completes.

## §U UNIFIED STRATEGY (target architecture)

Goal: ONE strategy model, flexible + cohesive. Multi-pair first-class for ALL strategies; GEM a special case. One list, one editor.

Strategy = three optional layers:
- universe: pairs:[..] (default 1 = today's single-pair) + max_open_positions.
- signals: existing [[actions]] triggers (per-pair). optional.
- allocation: mode=none|rotation + rank_by/momentum_period/rebalance_every/min_momentum. optional.

Cases: universe=1+signals+no-alloc -> today's trigger strat (no behaviour change, V10 hold). universe=N+signals+max_positions -> signals across N pairs (NEW multi-pair for all). universe=N+allocation=rotation -> GEM.

UV1: universe absent|len=1 & alloc none -> identical to legacy single-pair (V10 parity)
UV2: legacy trigger TOML (no [universe]) -> universe=1, no alloc (back-compat)
UV3: legacy [rotation] TOML -> universe=pairs + allocation=rotation (back-compat)
UV4: one Strategies list + one editor for all (universe + signals + allocation)

T36|x|unified schema in shared: optional Universe+Allocation on Strategy; parse legacy trigger+rotation; tests|UV2,UV3
T37|x|unified engine: dispatch single-pair(legacy)|multi-pair-signals|rotation; reuse evaluate_tick+run_rotation; V10 parity hold|UV1
T38|x|unified backtest service + result (multi-pair) + metrics scorecard for all|UV1
T39|x|strategy editor gains a universe (multi-pair) section → multi-pair first-class for ALL trigger strategies; emits [universe]; live preview + draft. NOTE: two editor UIs remain by design (trigger visual builder vs rotation momentum form), both now on the unified data model + format. Full single-form merge = deferred (T43)|UV4
T40|x|unified Strategies list: all strategies one page, feature badges|UV4
T41|x|migrate built-in GEM + examples to unified TOML|UV3,V9
T42|x|rotation no longer a separate user-facing format: editor WRITES unified [universe]/[allocation], engine READS both via RotationStrategy::from_content bridge. RotationStrategy demoted to internal engine adapter. GEM backtest routes to rich /rotation report|UV4
T43|x|ONE unified editor: Allocation section (none|rotation) in the strategy editor; rotation hides Actions + shows momentum params; emits [universe]+[allocation]. strat_editor parses via parse_unified (loads both formats). New GEM → /strats/new?alloc=rotation; GEM edit → unified editor; backtest → rich /rotation report. /rotation/new + /rotation/edit now redirect in. Verified live (create→save→backtest→reload)|UV4
T44|x|docs (V9): book updated for unified model — editor.md gains Universe + Allocation; gem.md uses [universe]/[allocation] + one-editor flow. Pushed to botmarley-book → GitHub Pages|V9,I.docs

# Backtest correctness audit (2026-06-08). Done this session: T45,T46,T47. Backlog: T48-T55.
T45|x|fix multi-TF look-ahead: expand_to_1m_ticks uses last CLOSED bar (b-1)|V21
T46|x|wire slippage into fills: SimulatedExecutor::fill_price, gated by BACKTEST_SLIPPAGE_BPS (default 0)|V22
T47|x|fix metrics: stop double-counting fees (final_result=net, bounded -100%); win-rate over closing trades|V23
T48|x|CRITICAL: route backtest fills through SimulatedExecutor → apply LOT_SIZE/step_size/min_qty/min_notional rounding+rejection; closes dust/sub-min over-fill|V22
T49|x|CRITICAL: cross-pair quote→USD via PriceOracle on fill path (BNB/BTC etc.); backtest portfolio in consistent USD base|V22
T50|.|CRITICAL: fill at next candle OPEN, not signal-candle close (kill signal-on-close optimism); keep both backtest paths consistent|V22,V21
T51|x|HIGH: open_long/buy percent reserves fee (size = balance/(1+fee)*pct), clamp usd_balance ≥ 0|V22
T52|.|HIGH: grade/headline on REALISED not mark-to-market (apply realised_share demotion to detail grade; label mtm)|V23
T53|x|HIGH: buy&hold benchmark pays same entry/exit fee as strategy (fair alpha)|V23
T54|x|HIGH: display engine's actual equity curve (store it) instead of re-simulating from current Arrow data|V23
T55|x|HIGH: real backtest=live parity test — drive executor fill path, assert qty/commission/slippage match SimulatedExecutor (extend parity_evaluation.rs)|V22
T56|x|HIGH: consecutive_candles multi-TF anchors to last CLOSED bucket (not forming bar)|V21
T57|x|MED: CAGR clamp/annotate sub-year; Calmar guard mdd==0; drop hardcoded summary maxdd/sharpe=0 from view; pnl_at_last_trade rename; slippage default policy|V23

## §U2 ATR-SCALED THRESHOLDS (volatility-normalised triggers)
goal: ONE strategy self-adapts across pairs (ADA jumps, BTC calm) instead of N hand-tuned %-variants. v4.6.4 family passed walk-forward (4 pairs, both halves, realised≈final) but fixed +3.1% TP barely fires on calm BTC (+0.2% TEST) → ATR unlocks it.
T58|x|shared: parse ATR value `±k*atr[_period]` — parse_atr_value in evaluation.rs (sign=dir, period dflt 14); fixed-% path unchanged; unit tests pass|V24
T59|x|core indicators: extract_unique_indicators_with_timeframe scans PriceChange value+tolerance for atr refs → registers ATR(period)@tf in cache (key atr_{p}[_tf])|V24,V21
T60|x|core evaluation: resolve_atr_threshold frozen at entry tick (mult·ATR(opened_at_tick)/entry-tick close·100); applied to pos_price_change, follow activation, trailing_stop; warmup→None→inert; None-safe (no panic)|V24,V22,V10
T63|x|snowballv4.6.4-atr built+saved (demo+prod); tuned: atr_tight {TP+0.6,DCA-0.8,allin-1.0,rebuy-2.0}×ATR(14,1d) ONLY set positive out-of-sample on all 4 pairs (ADA+6.8 BTC+0.8 ETH+5.3 XRP+6.4); base/wide failed BTC/ETH → test has teeth, not relocated overfit|V24
Tlive|x|V22 parity: live engine (trading/engine.rs:839,863) reuses IndicatorCache::build + evaluation::evaluate_tick → ATR resolution shared backtest↔live, zero extra code. Verified empirically (smoke BTC 272 trades via ATR triggers)|V22
T61|x|editor: value unit selector (% \| ×ATR) + ATR period + live-recompute toggle; composes `k*atr_N[_live]` (tf from trigger Timeframe), parses k*atr_N[_tf][_live] back on load; selector hidden for non-numeric types (duration/pattern); per-trigger so mixing %/×ATR allowed; tooltip. Core parse_atr_value accepts emitted format; render test + node round-trip verified|V24,I.svc
T62|x|editor: tolerance-sign warning on pos_price_change_follow — activation≥0+positive tol OR activation<0+negative tol → inline warn "trailing OFF, fires immediately on activation". Live on input + on type change. Render test + node logic check|I.svc
T64|x|docs (V9): book editor.md — new ×ATR section (% vs ×ATR unit, k multiplier, ATR period, entry-freeze + warmup, recompute-live opt-in, mixing) + tolerance-sign note. Also fixed stale multi-pair copy → shared pool/first-come + max_per_pair (overlaps T71 editor portion). Pushed to botmarley-book main (6380207)|V9,I.docs
T65|x|trigger_status.rs: atr_display_label via parse_atr_value → ×ATR thresholds shown as "volatility-scaled -3×ATR(14) — resolved live at fill" instead of "invalid threshold" (pos_price_change + trailing_stop arms). Cosmetic, ⊥ execution. Unit tests|I.svc

## §U3 PORTFOLIO MULTI-PAIR (shared-pool, first-come — GENERALISE the one engine)
goal: make multi-pair worth more than running pairs separately. ONE shared $ pool scans the whole universe; a selective (ATR-gated) strategy keeps full firepower idle and deploys it wherever the best setup fires first. DECISION (user): ⊥ a second engine — GENERALISE the existing engine to universe 1..N; single-pair = universe of 1 (degenerate). Lower maintenance, single=multi free. Parity-gated: universe=1 ≡ today's single-pair byte-for-byte (parity_evaluation.rs is the guard). The hard part (pair-aware core: evaluate_tick/execute on a per-pair position view + shared cash) is identical either way → unify for free. Isolated equal-weight run_multipair → deleted (subsumed).
T66|x|shared schema: Position gains `pair` tag (done in T67); Universe gains `max_per_pair` (opt) + round-trip. NOTE: no portfolio mode flag — isolated mode was deleted (§U3), a flag would be dead (YAGNI)|V25
T67|x|core: GENERALISE run_with_executor → universe 1..N, ONE shared PortfolioState (pair-tagged positions), per-pair IndicatorCache, unified 1m timeline; per tick × per pair (universe order) evaluate against that pair's position-view + shared cash; open iff total<max_open_positions AND pair<max_per_pair AND cash ok (first-come); DCA/sell/follow on that pair's positions; peak/trough + next-open fill PER PAIR. single-pair = N=1 thin wrapper|V25,V21,V22,V10
Tpar|x|PARITY GATE: universe=1 path ≡ old single-pair output exactly — parity_evaluation.rs green + add a 1-pair-via-multipath vs legacy assertion BEFORE building caps/UI|V10,V22
T68|x|core service + dispatch: universe>1 (non-rotation) routes to run_multipair→run_universe (shared-pool); max_per_pair now read from [universe] (T66); stale "sub-accounts/aggregate" comments fixed. Isolated path already gone (run_multipair IS the shared-pool adapter)|V25
T69|x|editor: max_per_pair field (reveals at 2+ pairs) + tooltip explaining shared cash + global cap; universe copy fixed (shared-pool, first-come — was stale "equal-weight sub-accounts/split equally"). NO portfolio/isolated picker — isolated deleted (§U3), only shared-pool exists. JS buildState/stateToToml/restore wired; save POSTs raw TOML so max_per_pair round-trips. Render smoke test (single=hidden, multi=revealed)|V25,I.svc
T70|x|tests: shared-cash conservation (no money created/lost); global cap never exceeded; max_per_pair respected; universe-order tie-break deterministic; universe=1 ≡ single-pair parity|V25,V10
T71|.|docs (V9): book — portfolio vs isolated multi-pair, shared cash + first-come semantics, max_per_pair|V9,I.docs
T72|x|backtest detail: per-pair statistics card — hold time min/avg/max (position open→last close) + avg realised profit per trade + total realised + open-now count. Reconstructed from stored actions by (pair, position_number); no migration, works retroactively; single + multi pair. compute_pair_stats unit tests + render-block test|V12,I.svc

## §U4 PREEMPTIVE EVICTION (capital velocity — promote fast cyclers, evict slow bags)
goal: in a shared pool, long-held underwater bags squat global slots and starve fast-cycling pairs (observed: DASH held hours @ +$4/trade got 4 trades while CAKE/PAXG bags held 40-62 days hogged both slots → −9%). Let a blocked open evict a chosen victim. User decision: build all 3 victim policies, configurable, and backtest which wins. Engine-level (NOT a per-position trigger — victim choice needs cross-position visibility). Parity: universe=1 unaffected.
T73|x|shared schema: `[eviction]` block on Strategy — `policy` (none\|smallest_loss\|oldest\|worst_laggard, default none) + `min_hold` (duration str like "2d"). parse + round-trip; default none = no behaviour change|V26
T74|x|core: preemptive eviction in run_universe shared-pool path. evaluate_tick emits would-open even when ONLY global cap blocks (new `preempt_global_cap` flag; false for single-pair → parity). run_universe: on a would-open at global cap, pick victim among positions held ≥ min_hold by policy, force-close it (next-open fill of victim's pair, executor fee, realised pnl, pair-tagged sell action w/ reason), then open contender. No victim → open stays blocked. per_pair cap never preempted|V26,V22,V21,V10
Tpar2|x|PARITY GATE: eviction OFF (default) ⇒ run_universe output byte-identical to T67; universe=1 + eviction-on ⇒ still ≡ single-pair (no contention path). parity_evaluation.rs green|V10,V22
T75|x|tests: each policy picks the correct victim (smallest_loss/oldest/worst_laggard); cash conserved across eviction; global cap never exceeded post-eviction; min_hold respected (too-fresh not evicted); no-victim leaves open blocked; eviction-off ≡ T67|V26,V10
T76|x|editor: `[eviction]` UI — policy select + min_hold field (shown for 2+ pairs); tooltip (capital velocity, victim policies, books realised loss). emit/parse. render test|V26,I.svc
T77|x|comparison run done (Dip_v4 9-pair, max∈{2,5}×4): eviction HELPS loose pools (max5 off +16%→evict +76%, cuts −975 unreal to −106) but HURTS tight pools (max2 off +96%→evict +75%). 3 policies ~indistinguishable (worst_laggard marginally best). Verdict: eviction is a wide-universe tool, not a default; policy choice = noise|V26
T78|x|docs (V9): book — preemptive eviction, victim policies, min_hold, capital-velocity rationale|V9,I.docs

## §U5 MULTI-PAIR METRICS (Buy&Hold + Max Drawdown for universe runs)
goal: a multi-pair backtest currently shows B&H n/a + MaxDD 0.00 (handler loads ONE arrow file "A+B+C" → none → all metrics None). Without a benchmark a multi-pair % has no reference. Fix at render time from stored actions (retroactive, works on existing runs).
T79|x|server: multi-pair branch in backtest_detail metrics — reconstruct honest equity curve (per-pair crypto from stored actions × each pair's own close + shared usd) + equal-weight B&H (1/N, same fees) → feed metrics::compute → real MDD/CAGR/Sharpe/alpha/grade for universe runs. Single-pair path unchanged|V27,V23
T80|x|verified on demo: Dip_v4 9-pair run 74877d7a now shows B&H −13.35%, alpha +114.66%, CAGR +34.54%, MaxDD 38.28% (was n/a/0.00). Summary Breakdown MaxDD also uses the real metric|V27
B3|2026-06-12|a "full" sell (amount 100%) could leave a few-wei float/LOT-rounding dust in the position; sell_position only removed on `crypto<=0` or `amount_percent>=100`, so a sub-cent dust remainder survived as a ZOMBIE position (0 value, original entry retained). position-relative buy/DCA/pyramid triggers (pos_price_change) kept firing forever after the exit, spamming $0 "executed" buys (user strat "stupid02": sell #7 closed, #8+ = $0 buys vs stale entry). Affects backtest AND live (shared PortfolioState.sell_position).|sell_position also removes the position when the remainder is worth < $0.01 (DUST_USD) — negligible dust written off like a real account. unit tests: dust remainder removed, meaningful partial kept; parity 19 green

## §V (short-side, cont.)
V28: short-side OPTIONAL + opt-in per strat via `open_short` action. strat w/o any open_short ≡ pre-short engine byte-identical (parity V10). ⊥ global flag, ⊥ every strat shorts. long-only TOML unchanged (no `side` token).
V29: NO LEVERAGE. gross exposure = Σ long cost-basis + Σ |short notional@entry| ! ≤ total_equity at open → else reject open (mirror existing `amount_usd > usd_balance` guard). short open notional X@P → usd_balance += X, crypto_balance −= X/P (negative bal = owed crypto). total_equity = usd + Σ crypto·price (negative crypto = liability) ∴ short P&L correct w/o new equity math. cash conserved (⊥ money created).
V30: short exits DIRECTION-AWARE. for Short position: pos_price_change = (entry − price)/entry (positive when price FALLS = profit); trailing_stop trails the TROUGH (low since entry), fires when price rises k off trough; hard stop fires when price RISES vs entry; time_in_position unchanged; follow inverts peak/trough. close = buy-to-cover (full|%). Long positions ⊥ changed (byte-identical, V10).
V31: ⊥ SILENT LIVE SHORT. real exchange short needs margin/borrow (no leverage = cash-secured, still margin acct). until live short impl'd → strat w/ open_short runs backtest + paper ONLY; live attempt → refuse | paper-fallback + surfaced notice (like Kraken paper fallback). ⊥ silently place a spot order that cannot actually short.

## §U6 SHORT-SIDE (optional, no leverage, opt-in per strategy)
goal: shorts as an OPTION not a main feature — a strat MAY use `open_short` to profit in down-moves; long-only strats untouched & byte-identical. no leverage (cash-secured, gross ≤ equity). backtest+paper first; live gated until margin path built.
T81|x|shared schema: ActionType += `OpenShort` (serde snake_case `open_short`); Position += `side: PositionSide{Long,Short}` (serde default Long → back-compat + parity); validate_toml accepts open_short; round-trip. long-only TOML byte-identical (no side emitted)|V28,I.svc
T82|x|portfolio.rs: `open_short` (crypto_balance negative, usd_balance += notional, NO-LEV gross-exposure guard via new `gross_exposure()`); buy-to-cover via sell-path for Short (reduce |crypto| → 0, usd −= cover cost, realised pnl, dust-close per B3); total_equity/unrealized_pnl correct for negative balances|V29,V12
T83|x|evaluation.rs: direction-aware trigger eval for Short (pos_price_change inverted, trailing_stop off trough, hard stop on rise, follow inverted); evaluate_tick emits open_short executions; execute opens short + covers; Long path byte-identical|V30,V21,V22
T84|x|execution/simulated.rs: short fills — sell-to-open + buy-to-cover same fee base, LOT_SIZE/step/min_qty/min_notional rounding+rejection as long (V22); slippage signed correctly (open-short fills worse = lower, cover worse = higher)|V29,V22
T85|x|PARITY GATE: strat w/o open_short ⇒ run output byte-identical to pre-short engine; parity_evaluation.rs 19 stay green + add short-mirror tests (each long exit has a Short twin: profit-on-drop, trail-off-trough, stop-on-rise)|V10,V28,V30
T86|x|tests: short P&L sign (profit on price drop, loss on rise); no-lev cap rejects over-exposure open; cash conserved across open→cover; each direction-aware exit fires at right side; buy-to-cover full/partial + dust close|V29,V30
T87|x|live guard: live/paper executor refuses REAL short (no margin path) → paper-fallback + surfaced notice; ⊥ silent spot order for a short; backtest/paper unaffected|V31
T88|~|editor DONE: `open_short` selectable action + tooltip (no leverage, cash-secured, sell=cover, backtest/paper-only). Book short-side section PENDING (external repo)|V28,V9,I.svc,I.docs

## §V (cross-asset entry filter, cont.)

V32: CROSS-ASSET ENTRY FILTER (optional, opt-in per strat via `[entry_filter]`). a strat WITHOUT the block ≡ pre-filter engine byte-identical (parity V10, gate = None). filter gates ONLY opens (`open_long`/`open_short`); DCA (`buy`)/`sell`/cover/follow/trailing/stop NEVER gated — once in a position, exits run unconditionally (don't trap capital behind a closed gate). gate from a REFERENCE pair's moving-average momentum: ma_type ∈ {ema (primary), hma}, span N, on the ref pair's `timeframe` (default 1m). condition ∈ {rising: MA(t)>MA(t−1) | velocity: MA(t)/MA(t−1)−1 ≥ threshold | price_above: ref_close(t)>MA(t)}. ⊥ LOOK-AHEAD (V21): gate at timeline tick t uses ONLY the ref bar fully CLOSED ≤ t (close_time = open_ts + tf_dur ≤ t) → ref MA aligned to timeline by TIMESTAMP (binary search), never by index (ref pair ≠ current pair, different 1m arrays/gaps). warmup (< span closed ref bars at t) → gate CLOSED (trend unconfirmed ⇒ no open); bounded by span only. BACKTEST = PAPER = LIVE (V10): same ref candles + MA + alignment ⇒ same gate. ref data MISSING (backtest: no ref Arrow; live: ingestion never broadcast ref) → SURFACE the error, ⊥ silently skip the gate (would over-trade) NOR silently idle. ref pair = current pair allowed (self-momentum filter).

## §U7 CROSS-ASSET ENTRY FILTER (BTC-MA momentum gate — validated, deterministic)

# Research (project memory, 2026-06-15/16): BTC-lead edge REAL on laggy alts (RUNE/CAKE) but tiny & fee-bound as an ML feature; ROBUST shippable expression = deterministic BTC-MA-velocity ENTRY gate. EMA > HMA > LSMA in tests → EMA primary, HMA optional, LSMA dropped. velocity ~0.02%/bar. validated per-pair offline (RUNE +0.058–0.082%, CAKE +0.043–0.060% mean-fwd lift; ETH none; ZEC non-responder).

T89|x|shared schema: `[entry_filter]` block on Strategy — `EntryFilter{ref_pair:String, ma_type:MaType(ema\|hma, default ema), span:usize, timeframe:Option<String>(dflt 1m), condition:FilterCondition(rising\|velocity\|price_above, default rising), threshold:f64(default 0)}`. Option on Strategy, serde skip_if none. parse + round-trip; absent ⇒ no field emitted (long parity). validate_toml accepts it. unit tests (parse/emit/round-trip, absent = byte-identical)|V32,I.svc
T90|x|core entry_filter.rs: `compute_entry_gate(&EntryFilter, ref_candles:&[Candle], timeline:&[Candle]) -> Result<Vec<bool>>` — aggregate ref to filter.tf (reuse aggregate_candles), compute MA (ema_from_values; hma = wma(2·wma(n/2)−wma(n), √n)), per timeline tick binary-search last ref bar with close_ts ≤ tick_ts, eval condition; warmup/no-ref-bar ⇒ false. ref empty ⇒ Err (surface). unit tests: no-look-ahead (gate[t] ⊥ uses ref bar closing > t), each condition, warmup=false, alignment across mismatched timestamps/gaps|V32,V21
T91|x|core evaluation: `evaluate_tick` gains `entry_gate: Option<&[bool]>`; when Some, an `open_long`/`open_short` execution is suppressed unless `gate[tick_index]` (opens only — Buy/Sell/cover paths untouched). None ⇒ byte-identical (parity). thread through all 3 callers as None initially. unit test: gate=None ≡ pre-change; gate=[false] blocks opens but lets a DCA/sell fire|V32,V10
T92|x|core backtest wiring: service.run_backtest builds the gate when strat has `[entry_filter]` — load ref_pair Arrow for the window (reuse load_candles), `compute_entry_gate` vs the run's timeline, pass to engine.run_with_executor + unified.run_universe (multi-pair: align gate to the unified 1m timeline). ref Arrow missing ⇒ BacktestError (surface, V32). single-pair + no-filter paths unchanged|V32,V10,V22
T93|x|core live wiring: TradingEngine.run takes optional ref subscription — when strat has `[entry_filter]`, caller `ensure_pair(ref_pair)` + a 2nd `subscribe()` (ref_rx); engine keeps a ref candle buffer (bootstrap ref history same depth as own), drains ref_rx each tick, recomputes gate for the current tick before allowing an open. ref feed absent ⇒ surface notice (⊥ silent skip / idle, V32). own-pair hot path untouched (parity for no-filter strats)|V32,V10,B5
T94|x|editor UI: `[entry_filter]` section (reveal toggle "Cross-asset entry filter") — ref pair input, MA type select (EMA\|HMA), span, timeframe select, condition select (rising\|velocity\|price_above), threshold (shown for velocity); tooltip (gates ONLY opens, BTC-lead rationale, no look-ahead). JS buildState/stateToToml/restore; save POSTs raw TOML so it round-trips. render smoke test (off = no block, on = emits [entry_filter])|V32,I.svc
T95|x|tests: parity gate — strat w/o [entry_filter] ⇒ run byte-identical to pre-filter engine (extend parity_evaluation.rs); gate blocks an open that fires w/o filter; gated open delayed to first true-gate tick; exits unaffected while gate closed; backtest=live gate equality on shared fixture|V32,V10,V22
T96|x|DEPLOYED prod+demo (both HTTP 307 healthy, editor section live on demo). VALIDATED real data (ADA_USDT, BTC 1h-EMA-rising): 2022 bear -33.9%→+6.4% MaxDD 38%→7%, 2024 Calmar 0.72→2.32, trades cut every window. No-silent-idle: candle loop independent of gate; missing ref surfaces+blocks opens (test). live-session smoke = reasoned (parity: no existing strat uses filter)|V32,V10
T97|x|docs (V9): book editor.md — cross-asset entry filter section written + PUSHED to GitHub Pages (botmarley-book 58d1d26, user-approved 2026-06-16)|V9,I.docs

## §V (data integrity, cont.)

V33: ARROW 1m FILES SELF-HEALING + CONTIGUOUS (P0). backtest, paper bootstrap and live ALL read the SAME `{pair}/1m.arrow` → it MUST be gap-free, or the three modes silently diverge (V10) and indicators compute on holey data. An INTEGRITY GUARD scans every Arrow pair on disk for INTERNAL gaps (any interval > 2×1m → one missing candle tolerated, matches GapFiller), refetches the missing range (Binance primary, Kraken fallback) and MERGES it back to the Arrow file. Runs (a) at server startup (after a delay), (b) on a schedule, (c) after a history sync of a pair (so freshly-synced data is contiguous before any backtest reads it). TRAILING gap (last candle → now) is NOT a hole (just not-yet-fetched) → excluded from the contiguity verdict. A gap the exchange genuinely lacks (delist/downtime) is REPORTED (logged), not silently masked nor infinitely retried. ⊥ look-ahead: healing only inserts real historical candles at their true timestamps. The guard is idempotent — a clean file is a no-op.

## §U8 ARROW DATA-GAP INTEGRITY GUARD (P0 — backtests share the live Arrow)

# Why P0 (user, 2026-06-15): "powinien latac luki i sprawdzac to" — a hole in a pair's 1m Arrow silently corrupts BOTH the backtest AND the live bootstrap that read it. Existing gap-fill runs ONLY for ingestion-registered pairs at boot; backtests read ANY synced pair. Generalise: heal ALL Arrow pairs, on a schedule + after sync, with a verify report.

T98|x|core `ingestion::integrity` module: `PairIntegrity{pair,candles,internal_gaps,missing,contiguous,first_ts,last_ts}`; `list_arrow_pairs(storage)` (flat dirs with a 1m.arrow); `scan_pair(repo,pair)` (read 1m → GapFiller::detect_gaps INTERNAL only, exclude trailing → report); `heal_pair(repo,&GapFiller,pair)` (scan → fill_gap each internal gap → merge_candles → re-scan → HealReport{filled,remaining}); `heal_all`/`scan_all` over a pair list. idempotent (clean file = no-op). unit tests: synthetic gap detected; heal merges + re-scan clean; trailing gap excluded; clean file no-op|V33
T99|x|core: `CandleRepository::storage_path()` getter; `DataIngestionService::integrity_filler()` (Binance primary + Kraken fallback) + `heal_all_arrow_pairs()` (list_arrow_pairs(storage) → integrity::heal_all) + `scan_all_arrow_pairs()` (report, no fetch). reuses existing clients+repo|V33
T100|x|server: `data_integrity_scheduler` (like portfolio_scheduler) — wait ~90s after boot, run heal_all_arrow_pairs once, then every 6h; logs a summary (pairs scanned, gaps healed, remaining). spawned from main.rs only when ingestion service is present|V33
T101|x|server: after a history sync completes for a pair (handlers/history.rs sync path / sync task), heal THAT pair (integrity::heal_pair) so freshly-synced 1m is contiguous before any backtest reads it. surface remaining gaps in the log|V33,V10
T102|x|tests + verify: integrity unit tests (T98) green; a scan over a known-good fixture reports contiguous=true; a fixture with a hole reports the gap and (mocked/late-bound) heal path documented. SPEC V33 invariant recorded|V33

## §U9 AUTO-TUNE (per-pair parameter sweep — task #3)

# Goal (user task #3): tune a strategy's params per pair instead of one-size-fits-all. Generic, honest (train/test split so overfit is visible), DB-free offline tool. drift-swing is the first customer but the tool is strategy-agnostic.

T103|x|offline_research `tune` mode: TEMPLATE.toml with `{{KEY}}` tokens + `KEY=v1,v2` grid args → cartesian product of variants; each run on TRAIN (first 2/3) + TEST (last 1/3) of the window for one pair; rank by TEST Calmar, print train+test side-by-side (overfit visible). reuses BacktestEngine::run_with_executor + reconstruct_metrics. usage line added|V10
T104|x|`strats/templates/drift_swing.tmpl.toml` (tokenised drift-swing: DCA depths, RSI gate, BB period, TP/stop/time) + run `tune` on ≥2 local pairs → record best-per-pair params + the honest finding (does per-pair tuning beat the fixed builtin OOS?). result logged in memory/SPEC|V10

## §U10 DRIFT+TREND PAIRING (task #1 — combine the mean-rev defender with the trend earner)

# Goal (user task #1): pair drift-swing (range/bear defender) with all-weather ema-flip (bull earner) so the portfolio earns the bull legs the mean-rev sleeve sits out. The `combo` offline mode (2-strat 50/50, pair-per-leg) already exists — RUN it on real local data, confirm the blend lifts Calmar vs either sleeve alone, document the recommended basket.

T105|x|run `combo` drift_swing_meanrev × allweather_ema_flip on ≥2 real local pairs over a full window; record blended PnL/CAGR/MaxDD/Calmar vs each sleeve alone. Honest verdict: does the pairing improve risk-adjusted return? Logged in memory|V10
T106|x|BLEND WINS: drift_swing(XRP_BNB) + allweather(BTC_USDT) 50/50 → Calmar 1.48 > best leg 1.38, MaxDD 26%→14%. Recommended basket = mean-rev sleeve on ranging ratio pairs + trend sleeve on USD majors. YAGNI: combo mode + existing builtins express it, nothing new shipped. Documented in memory; book note deferred (T97-style)|V9

## §U11 GEM-LIVE (task #2 — DECISION REQUIRED before build)

# Research (memory): HOLD beats ROTATE at every cadence (trailing-2y-Calmar SELECT top-5 + HOLD ≈ Calmar 0.94 OOS; monthly/6mo ROTATE 0.34-0.38). A literal "live rotation engine" would ship the inferior mechanism. The valuable live feature = SELECTION + HOLD (+ yearly re-select), or a periodic-rebalance runner that the user can set to a slow cadence. NEED user decision: (a) live rotation as written, (b) live SELECT-and-HOLD instead, or (c) defer. Until decided, T107+ are unspecified.

T107|x|DECISION (user delegated → my rec): build LIVE SELECT-and-HOLD, not literal rotation. VALIDATED on real data (walkselect, allweather_ema_flip, 10 USD majors, trail 2y, re-select yearly, top5): OOS 2023-06→2025-06 = +62.8%, CAGR 27.5%, MaxDD 38%, Calmar 0.72; picks adapt (SOL/ETH/BTC→BTC/SOL/DOT). hold>rotate confirmed (memory). Mechanism proven; live engine = T108-T112 below|V14,V16
# Build breakdown for the live select-and-hold portfolio. NOT built this session: it is a REAL-MONEY-capable live execution subsystem (multi-asset orders + scheduler + persistence + recovery + UI). Per safety + paper-first posture, this warrants a focused session with the user present for the order-execution path. Mechanism + decision done (T107); the live build is scoped here.
T108|.|core: `portfolio_session` type + `select_hold_service` — periodic re-select (trailing-Calmar rank over universe via run_with_executor on trailing windows), top-K equal-weight target; reuse walkselect logic; persist {held picks, weights, last_reselect_ts}|V14,V16,V10
T109|.|core: live rebalance executor — diff current holdings vs target picks, place buy/sell orders (PAPER first; real orders gated like V31 short-guard), book fills, update portfolio state|V12,V16
T110|.|server: portfolio-session scheduler + recovery (re-spawn on boot like trading sessions); status surface (current holdings, next re-select date, equity)|V16,V17
T111|.|UI: live portfolio page — holdings, weights, next re-select, equity curve vs equal-weight hold benchmark|V14,I.svc
T112|.|tests + paper validation: select-and-hold paper session matches walkselect OOS on the same window (V10); deploy paper-only; real orders remain gated until user authorises|V10,V16

## §U12 INCREMENTAL HISTORY SYNC (resume + merge, then gap-heal)

# User (2026-06-16): history sync should resume from already-fetched data per pair instead of re-downloading everything, and fill any internal gaps at the end for consistency.

T113|x|binance sync_history INCREMENTAL: per pair read existing 1m Arrow, resume the fetch from the last stored candle's day (req start_date if empty/older), MERGE existing+fetched (BTreeMap dedup by ts, fetched wins on overlap), rewrite merged 1m + re-derive TFs from the full merged series. internal gaps filled by the post-sync integrity heal (T101). Kraken path unchanged (Binance is default)|V33,V10
