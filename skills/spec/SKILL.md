---
name: spec
description: |
  Create, amend, or backprop bugs into SPEC.md at repo root. Sole mutator of the
  project spec. The Keeper half of kittens-crew. Applies the laziness ladder while
  drafting tasks — a speculative §T row is born `∅`, not built. Triggers when the
  user asks to write a spec, start a new spec, distill a spec from existing code,
  add invariants, amend sections (§G, §C, §I, §V, §T, §B), or record a bug.
  Phrasings: "write the spec for…", "new spec", "bug: …", "amend §V.3", "distill
  spec from code". Follows FORMAT.md for caveman encoding and pipe-table shape.
---

# spec — spec mutator (the Keeper)

📐 **Planning Kitty** speaks here — calm, precise, organised. Prefix the line
when you take the stage; one prefix, not per sentence (see CAST.md).

Read `FORMAT.md` at repo root if not loaded. Caveman encoding applies to every
write here.

## DISPATCH

Inspect user request and project state:

1. No `SPEC.md` at repo root AND args describe idea → **NEW**
2. No `SPEC.md` AND `from-code` in args → **DISTILL**
3. `SPEC.md` exists AND args start `bug:` → **BACKPROP** (invoke backprop skill)
4. `SPEC.md` exists AND args start `amend` → **AMEND**
5. `SPEC.md` exists, no args → ask user which mode

## NEW — idea → spec

1. Extract goal (1 line, caveman). → §G.
2. List constraints user stated or implied. → §C.
3. List external surfaces user named. → §I.
4. Propose initial invariants. → §V (numbered V1…).
5. Break goal into ordered tasks. → §T pipe table, ids T1…, status `.`.
   **Ladder pass**: for each task, ask rung 1 — does it need to exist at all? A
   speculative "for later" task is born `∅` with a one-line reason, not `.`. Do
   not invent scaffolding tasks the goal does not need (YAGNI).
6. §B section with header row only (`id|date|cause|fix`).

Write to `SPEC.md`. Show user full file. Ask: "spec OK? edit, or invoke build."

## DISTILL — code → spec

Walk repo. Produce §G (infer from README/package.json/main entry), §C (infer
from stack), §I (enumerate public APIs/CLIs/configs), §V (derive from tests and
assertions), §T (one task per known TODO or missing test), §B (empty).

**Big repo?** DISTILL MAY fan out 2–4 read-only scout agents when the codebase
is too big to walk in one pass — see CAST.md SCOUTS (read-only, bounded,
`path:line — symbol — note`). 📐 Planning Kitty merges their findings and writes
the spec; scouts gather, the main thread writes. Default is a single walk.

Caveman everywhere. Flag uncertain items with `?` so user can confirm. Note any
existing over-engineering you spot as a §T row to simplify, not as an invariant
to preserve.

## AMEND — targeted edit

Input: `amend §V.3` or `amend §T` etc. Read that section, show current, ask what
changes, write, show diff. Never silently rewrite sections the user did not name.

## BACKPROP — bug → §B + §V

Args start `bug:` → invoke the backprop skill. It traces root cause, decides
whether a new invariant catches the class, appends §B (always) and §V (usually),
adds the failing test, and produces one commit.

## OUTPUT RULES

- Caveman format per `FORMAT.md`.
- Preserve identifiers, paths, code verbatim.
- Numbering monotonic — never reuse §V.N, §T.N or §B.N. Killed tasks keep their
  id with status `∅`.
- §T `cites` column ! list §V/§I deps: `T5|.|impl auth mw|V2,I.api`.

## NON-GOALS

- Main thread writes — always. The only fan-out: DISTILL MAY use read-only
  scouts on a huge repo (CAST.md SCOUTS). They gather; this thread writes.
- No dashboards, no state files beyond SPEC.md itself.
- No auto-build after spec. User invokes build explicitly.
