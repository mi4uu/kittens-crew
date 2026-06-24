---
description: Wire kittens-crew into this project — config + 8-event hook membrane.
argument-hint: "[--dry-run | --target <dir> | --force]"
---
Run `kittenscrew init ${ARGUMENTS}`. Writes `kittenscrew.toml` and registers the
8-event hook membrane in `settings.json` (V6: only if `squeez` is reachable, else exit 3).

- `--dry-run` — preview, touch nothing.
- `--target <dir>` — isolate the write (Docker arm / tests).
- `--force` — overwrite an existing `kittenscrew.toml`.
