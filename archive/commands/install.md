---
description: Doctor — check kittens-crew hooks are wired and rtk is ready. Reports, then offers fixes.
---

🎩 **Orchestrating Kitty** runs the setup check. Inspect, report a status table,
then offer the fixes — never run an install or touch settings without the user's
OK first.

## CHECKS — run these, read-only

1. **Plugin wiring** — confirm the plugin files exist under `${CLAUDE_PLUGIN_ROOT}`:
   `hooks/hooks.json`, `AGENTS.md`, `skills/`, `commands/`. The persona hook is
   runtime-free (`cat`/`type`) — no node or bun required; say so. `AGENTS.md`
   also serves Pi / opencode / other agents.md-aware agents.
2. **Persona active?** — if this session opened with `KITTENS-CREW ACTIVE` in
   context, the SessionStart hook fired. If not, the plugin may be installed but
   not enabled, or the session predates it (tell them to restart the session).
3. **rtk installed?** — `command -v rtk`. On PATH → grab `rtk --version`.
4. **rtk hook wired?** — grep `~/.claude/settings.json` and the project
   `.claude/settings.json` for `rtk`. Present → `rtk init -g` already ran.

## REPORT

```
## kittens-crew doctor

plugin files    ✅ wired (hooks + 6 skills + 8 commands)
persona hook    ✅ active this session   (runtime-free, no node needed)
rtk binary      ❌ not on PATH
rtk hook        ❌ not wired

## fixes
- install rtk:   brew install rtk   (or cargo install --git https://github.com/rtk-ai/rtk)
- wire rtk hook: rtk init -g
```

Use ✅ / ⚠️ / ❌ per row. If everything passes, say so in one line and stop.

## OFFER THE FIXES

List the exact commands. For anything that installs software or edits settings
(`brew install rtk`, `rtk init -g`), **ask before running** — or tell the user to
run it themselves with `! <command>` so it lands in the session. Don't auto-run
external installers.

rtk is optional: if the user doesn't want it, the only thing that matters is the
two ✅ plugin rows. Don't push it.

## NOT THIS

- No sub-agents. Main thread checks.
- Don't reinstall the plugin or rewrite settings unprompted.
- Don't bundle an rtk hook — `rtk init -g` owns that (see README).
