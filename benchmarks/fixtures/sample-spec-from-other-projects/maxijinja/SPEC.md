# SPEC — maxijinja

Rust component lib. Backend-driven web UI. minijinja + htmx + axum. No JS/CSS build tooling.

## §G — goal

Rust crate. Build styled + interactive UI components (radix/shadcn/coreui-like) for axum apps using minijinja + htmx. React-like ergonomics, zero external JS/CSS build step. SSE supported. Ship component lib + live interactive showcase.

## §C — constraints

- C1 lang Rust. server axum (primary target).
- C2 templating minijinja. fragment render = first-class.
- C3 htmx = primary interaction model.
- C4 alpine.js (or equal-simple) = sprinkle interactivity when htmx not enough.
- C5 NO external build tooling. no node/npm/vite/webpack/tailwind-CLI to use lib or render.
- C6 apps backend-driven. server owns state + markup.
- C7 SSE = push updates, server-rendered fragments.
- C8 DX simple + pleasant. ergonomic Rust API.
- C9 components styled out-of-box, self-contained CSS shipped by lib.
- C10 ship showcase: component gallery + runnable interactive examples.
- C11 NO utility-CSS frameworks (tailwind etc) — they need extra build tools. own curated CSS instead.
- C12 built-in light + dark themes.
- C13 easy customize: lead colors, opacity/transparency, radius etc via CSS vars, no recompile.
- C14 curated helper/utility classes. essential set, NOT tailwind-billion.

## §I — surfaces

- I.comp   Rust component API. typed builders → html.
- I.axum   axum integration. response helpers, extractors, fragment-vs-full detect.
- I.jinja  minijinja registration. component fns/filters/templates.
- I.htmx   htmx attribute helpers (hx-get/post/swap/target/trigger).
- I.alpine alpine-style sprinkle helpers in templates.
- I.sse    SSE endpoint helper + event→fragment render.
- I.css    self-contained CSS: tokens + light/dark themes + curated utilities, served by lib, no bundler.
- I.theme  theme API: switch light/dark, override CSS vars (color/opacity/radius) for customization.
- I.show   showcase/playground binary. gallery + live examples.
- I.layout layout primitives: grid / stack / cluster → composable Markup.

## §V — invariants

- V1 fragment-vs-full: HX-Request header present → render fragment only. absent → full page. server sends only needed/changed markup.
- V2 zero-build: lib usable + components render with zero node/npm/bundler step.
- V3 scoped styles: component emits valid html + scoped CSS. no class-name collision across components.
- V4 sse-fragments: SSE event renders minijinja fragment, pushed to client, swapped via htmx-sse.
- V5 a11y: interactive components keyboard-navigable + correct aria (radix-like).
- V6 compose: components compose into axum responses w/o manual string concat.
- V7 sprinkle: common interactivity via declarative attrs. no hand-written JS for common cases.
- V8 showcase-live: showcase runs standalone, examples interactive not static.
- V9 themes: light + dark both built-in, switch at runtime w/o rebuild.
- V10 customize: lead colors, opacity, radius etc set via CSS custom props. no recompile, no extra tool.
- V11 utilities-curated: helper classes = essential curated set. covers common needs, NOT exhaustive tailwind-scale.
- V12 no-extra-tooling: styling requires zero downloaded CSS tool (no tailwind/postcss/sass CLI).
- V13 overlay-no-clip: popover/menu/overlay markup must NOT be clipped by ancestor overflow. containers holding them stay overflow-visible.
- V14 showcase-code: every showcase example displays its usage code next to the live demo.
- V15 input-variants: Input supports label placement (top/floating/hidden) + hint/error text + size, all a11y-correct.

## §T — tasks

```
id  | st | task                                                      | cites
T1  | x  | scaffold workspace: lib crate + showcase bin              | C1
T2  | x  | minijinja integration: register component tmpl/fns        | C2,I.jinja
T3  | x  | axum response helpers + HX-Request fragment detect        | V1,V6,I.axum
T4  | x  | component trait + typed builders → html                   | V3,V6,I.comp
T5  | x  | design tokens + scoped CSS bundle, served by lib          | V2,V3,C5,I.css
T5a | x  | light+dark themes via CSS vars                            | V9,C12,I.theme
T5b | x  | theme override API: colors/opacity/radius custom props    | V10,C13,I.theme
T5c | x  | curated utility/helper classes (essential set)            | V11,C14,I.css
T6  | x  | base components: button card input modal tabs dropdown    | V3,V5,I.comp
T7  | x  | htmx attribute helpers                                    | V1,I.htmx
T8  | x  | alpine-style sprinkle helpers in templates                | V7,C4,I.alpine
T9  | x  | SSE helper: event→fragment render + htmx-sse swap         | V4,C7,I.sse
T10 | x  | a11y pass: keyboard + aria on interactive comps           | V5
T11 | x  | showcase app: gallery + runnable interactive examples     | V8,I.show
T12 | x  | fix dropdown clip: overlay-safe containers                | V13,I.css
T13 | x  | input variants: floating/hidden label, hint, error, size | V15,I.comp
T14 | x  | layout primitives: grid/stack/cluster + util classes      | I.layout,V11
T15 | x  | showcase: Example wrapper shows live demo + usage code     | V14,I.show
```

## §B — bugs

```
id | date | cause | fix
B1 | 2026-06-11 | dropdown menu clipped — .mj-card overflow:hidden cut absolutely-positioned .mj-menu | V13
```
