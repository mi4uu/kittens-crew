# SPEC.md FORMAT

Single file. Project root. Every kittens-crew command reads it.

## SECTIONS

Fixed order. Fixed headers. Addressable.

```
# SPEC

## §G GOAL
one line. what code must do.

## §C CONSTRAINTS
- bullet. non-negotiable boundary.
- bullet. tech/lang/lib locked in.

## §I INTERFACES
external surface. what world sees.
- cmd: `foo bar` → stdout JSON
- api: POST /x → 200 {id}
- file: `config.yaml` schema …
- env: `FOO_KEY` required

## §V INVARIANTS
numbered. testable. each ! MUST hold.
V1: ∀ req → auth check before handler
V2: token expiry ≤ ⊥ allowed
V3: DB write ! in transaction

## §T TASKS
pipe table. ids monotonic (never reused). status: `x` done / `~` wip / `.` todo / `∅` killed by ladder.
id|status|task|cites
T1|.|scaffold repo|-
T2|.|impl §I.api POST /x|V2
T3|x|add §V.1 middleware|V1,I.api
T4|∅|custom cache class|-   (ladder: stdlib lru covers it, see note)

## §B BUGS
pipe table. backprop log. each row = bug + invariant that catches recurrence.
id|date|cause|fix
B1|2026-04-20|token `<` not `≤`|V2
B2|2026-04-21|race on write|V3
```

**Table cell rules**: literal `|` → escape as `\|`. Backticks OK. Cells trimmed. Empty = `-`.

## ADDRESSING

`§<S>.<n>` = section.item. `§V.2` = invariants section, item 2.
Commands, commits, PRs all reference by §. Zero ambiguity.

## CAVEMAN ENCODING

Default for every section. Rules:

- Drop articles (a, an, the). Drop filler.
- Drop aux verbs (is, are, was) where fragment works.
- Short synonyms (fix > implement).
- Fragments fine.

**Preserve verbatim**: code, paths, identifiers, URLs, numbers, error strings, SQL, regex.

**Symbols** (save tokens, machine-readable):

```
→   leads to / becomes / triggers
∴   therefore / fix
∀   for all / every
∃   exists / some
!   must
?   may / optional
⊥   never / impossible / forbidden
∅   killed / does not exist / dropped by ladder
≠   not equal / differs from
∈   in / member of
∉   not in
≤   at most
≥   at least
&   and
|   or
```

## LADDER MARKS (kittens-crew addition)

The Builder climbs a laziness ladder before writing code (see ladder skill).
Two ledgers record the outcome so the simplification is intent, not accident:

1. **Killed tasks** — a §T row the ladder proved unnecessary gets status `∅`,
   never deleted. One-line note in cell why (`stdlib covers it`, `YAGNI`).
   The kill is history: future readers see it was considered, not forgotten.

2. **`// kitten:` code comments** — every deliberate shortcut in the code
   carries one. Simple shortcut: `// kitten: this exists`. Shortcut with a
   known ceiling: name the ceiling and the upgrade path —
   `// kitten: global lock, per-account locks if throughput matters`.

`/kitten:check` reports both: §T `∅` rows that grew back as real code (drift),
and `// kitten:` ceilings that the spec now outgrew.

## WHY CAVEMAN FOR SPECS

Spec loaded every invocation. 75% fewer tokens = 75% fewer dollars & faster reads.
Human skims fast too. Symbols unambiguous.

## ONE FILE RULE

Big project → more sections, not more files. grep ceremony kills agent speed.
If SPEC.md > 500 lines, compact §B (old bugs drop oldest) before splitting.

## WRITES

| command | writes | section |
|---|---|---|
| `/kitten:spec new` | creates | all |
| `/kitten:spec amend` | edits | chosen |
| `/kitten:spec bug` | appends | §B + §V |
| `/kitten:build` | flips | §T status cell `.` → `~` → `x` (or `∅` on ladder-kill) |
| `/kitten:check` | — | read only |

That is whole format.
