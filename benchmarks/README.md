# benchmarks

Honest, reproducible measurement. No fabricated numbers — what's here, you can
run yourself with `bun`.

## compression (runnable now)

```bash
cd benchmarks && bun install && bun bench
```

Measures the one thing that's deterministic without a model run: how many tokens
the **caveman SPEC.md** encoding saves over the **same spec written as a normal
prose PRD**. Both fixtures live in `fixtures/` and contain the same content, so
the difference is the encoding alone. Tokenizer: `o200k_base` (GPT-4o family,
used as a proxy — Claude's exact tokenizer isn't public).

The spec is reloaded on every command invocation, so the saving recurs each call.

## agentic evals (not run here — recipe, not results)

Pass-rate and token-per-task comparisons against cavekit / ponytail / a vanilla
agent need real model runs, which cost money and aren't reproducible inside a
single session. So this repo ships the **method, not invented numbers**:

1. Pick a fixed task set (e.g. 10 small features with hidden tests).
2. Run each harness (kittens-crew, cavekit, ponytail, no-plugin) on each task
   with the same model, capturing: pass/fail, total tokens, wall-clock, diff LOC.
3. Report medians.

Wire it with `promptfoo` or a small bun driver + the Anthropic SDK and your own
key. Until those runs exist, this section stays empty on purpose — see the
project's stance on `/kitten:debt` and honest reporting. A made-up benchmark is
worse than none.
