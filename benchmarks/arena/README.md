# arena — interactive Claude Code benchmark, in Docker

Compare skillsets by running the **same** Claude Code, interactively, against the
same underspecified brief — and watching how each one plans, asks, builds, and
cleans up. No `claude -p` one-shots: claude runs in a tmux session inside a
disposable container, and **you play the user** — send a prompt, peek the pane,
say little, expect results.

## Why Docker — and clean isolation
Hard isolation, no host pollution, no cross-arm leftovers. **Nothing is installed
on your machine** — every skillset is installed *inside* its container. The base
image is **neutral**: a generic dev toolchain only (rust, bun, uv/python, node,
tokei, ripgrep, git, tmux) + the `claude` CLI. kittenscrew / squeez / cavekit /
ponytail are **not** baked in — each arm installs ITS OWN skillset at provision
time into a fresh mounted `~/.claude`. So:

- **baseline** starts pristine — pure Claude Code, none of our binaries even on
  PATH, `/repo` not mounted.
- **kittens** self-installs from nothing in-container (`install.sh` cargo-builds
  kittenscrew, installs squeez, drops the plugin, wires the membrane) — which also
  exercises the real install path.
- comparison arms (cavekit, ponytail) likewise start from nothing.

Each arm runs in its own throwaway container against its OWN copy of the project,
so arms never see each other's work. `--dangerously-skip-permissions` is set
*inside the disposable container only* — never on your host. Auth comes from
`.env`, kept out of every skillset.

## Run
```bash
cp .env.example .env          # add your key (Anthropic or a proxy base-url)
./arena.sh build              # build the image once (~few min)
./arena.sh up kittens ../../path/to/brief-project
./arena.sh peek kittens               # watch it boot
./arena.sh send kittens "build me a CLI feed reader. surprise me."
./arena.sh peek kittens 80            # check progress; repeat as the user would
./arena.sh artifacts kittens ./out/kittens   # pull the result out
./arena.sh down kittens
```
Run each arm against a **copy** of the project (arena does this automatically under
`state/<arm>/work`), so arms never see each other's work.

## Driving (you are the user)
- `send <arm> "<text>"` types a prompt and hits Enter.
- `keys <arm> Enter|Escape|C-c` sends a raw key (approve a prompt, interrupt).
- `peek <arm> [lines]` dumps the pane — poll it to decide your next move.
- Be terse. Let it work. Step in only when a real user would.

## What to measure (the rubric)
Score each arm blind, across arms:

1. **Autonomy / self-sufficiency** — how much did it drive itself vs need hand-holding.
2. **Decision-making** — quality of the choices it made.
3. **Knows when to ask** — did it clarify the underspec, or just guess.
4. **Planning** — did it make a plan, and how good is the plan.
5. **Code quality** — how good is what it built.
6. **Plan adherence** — how closely the built result matches the plan.
7. **Did it all succeed** — does the result actually work end to end.
8. **Kept a plan to the end** — did the participant maintain a plan AND track
   progress in it through to completion (not just at the start).
9. **Time** — wall-clock the task took / effort spent.
10. **Code volume + quality** — how much code generated and of what quality (`tokei`).

Capture per arm: start/end timestamps (wall-time), `tokei` LOC on the final
`/work`, the full tmux transcript, and the artifacts. Score blind with
`../agency/{judge.py,rubric.md}` (local + remote judges).

## Files
`Dockerfile` toolchain+claude+kittenscrew · `arena.sh` build/up/send/peek/down ·
`.env` auth · `state/<arm>/` the arm's mounted `~/.claude` + `work` copy.
Judges/rubric: reuse `../agency/{judge.py,rubric.md}`.
