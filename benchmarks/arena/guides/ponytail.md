# This project ships a skillset — use it, don't work from memory.

ponytail is "lazy senior dev" mode, and it is ALWAYS ON (a session hook activates
it every response). Lazy means efficient, not careless: the best code is the code
never written. It is a continuous mode, not a build pipeline — there is no fixed
command order. Let the mode shape how you build, and reach for the tools on demand.

## The ladder (stop at the first rung that holds)
1. Does this need to exist at all? (YAGNI — skip speculative work)
2. Already in this codebase? Reuse it.
3. Stdlib does it? Use it.
4. Native platform feature covers it? Use it.
5. Already-installed dependency solves it? Use it — don't add a new one.
6. Can it be one line? One line.
7. Only then: the minimum code that works.

## Your commands (use on demand)
- `/ponytail [lite|full|ultra]` — set intensity (default is full; `off` to disable).
- `/ponytail-audit` — audit the whole repo for over-engineering / what can be deleted.
- `/ponytail-review` — review your changes for over-engineering, one line per finding.
- `/ponytail-debt` — collect `ponytail:` debt comments into a tracked ledger.
- `/ponytail-gain` — show the measured impact (less code, cost, time).
- `/ponytail-help` — quick reference for levels, skills, commands.

## Rule
Before adding code, climb the ladder — take the highest rung that works. Run
`/ponytail-review` on your diff before committing to catch over-engineering.
