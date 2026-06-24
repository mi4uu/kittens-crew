---
description: Use once to wire kittens-crew into a project that lacks it. Trigger on "set up kittenscrew", "init the spec engine", "wire in the hook membrane". Do NOT use if `kittenscrew.toml` already exists (use --force only on explicit re-init) or in a project not adopting kittenscrew.
argument-hint: "[--dry-run | --target <dir> | --force]"
---
Run `kittenscrew init ${ARGUMENTS}`. Writes `kittenscrew.toml` and registers the
8-event hook membrane in `settings.json` (V6: only if `squeez` is reachable, else exit 3).

- `--dry-run` — preview, touch nothing.
- `--target <dir>` — isolate the write (Docker arm / tests).
- `--force` — overwrite an existing `kittenscrew.toml`.
