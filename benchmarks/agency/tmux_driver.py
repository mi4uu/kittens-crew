"""Drive a REAL interactive Claude Code TUI through tmux — the way a user actually
works with it (not headless -p). Send messages as keystrokes, detect when the
agent finishes a turn, read its response, capture cost. Validated mechanics:
trust prompt -> ready -> send -> idle-detect -> read -> /cost."""
import subprocess, time, re
from pathlib import Path

ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
BUSY = re.compile(r"esc to interrupt|Working|Thinking|Cogitat|✶|✻|⏳|↓ \d+ tokens|Running|Forging|Pondering")

class TmuxSession:
    def __init__(self, name: str, workspace: Path, skill_file: Path | None, model: str, plugin_dir: str | None = None):
        self.s = "ag_" + re.sub(r"[^a-zA-Z0-9_]", "", name)
        self.ws = workspace
        subprocess.run(["tmux", "kill-session", "-t", self.s], capture_output=True)
        subprocess.run(["tmux", "new-session", "-d", "-s", self.s, "-x", "220", "-y", "50"])
        cmd = (f"cd {workspace} && claude --setting-sources project,local --strict-mcp-config "
               f"--dangerously-skip-permissions --model {model}")
        if plugin_dir:                       # load the FULL kit plugin (commands + skills + hook)
            cmd += f" --plugin-dir '{plugin_dir}'"
        if skill_file:                       # extra prompt nudge (e.g. yagni, 'Be brief.')
            cmd += f" --append-system-prompt-file {skill_file}"
        self._raw(cmd); self._enter()
        time.sleep(7)
        if "trust this folder" in self.pane():
            self._enter(); time.sleep(5)        # confirm "Yes, I trust this folder"
        self.wait_idle(60)

    def _raw(self, text): subprocess.run(["tmux", "send-keys", "-t", self.s, "-l", text])
    def _enter(self): subprocess.run(["tmux", "send-keys", "-t", self.s, "Enter"])
    def pane(self) -> str:
        out = subprocess.run(["tmux", "capture-pane", "-t", self.s, "-p", "-S", "-6000"],
                             capture_output=True, text=True).stdout
        return ANSI.sub("", out)
    def ready(self, p: str) -> bool:
        return ("bypass permissions on" in p) and not BUSY.search(p)
    def wait_idle(self, timeout: int) -> str:
        t0 = time.time(); last = None; stable = 0
        while time.time() - t0 < timeout:
            p = self.pane()
            if self.ready(p):
                stable = stable + 1 if p == last else 1
                if stable >= 2:                 # ready + unchanged across two polls
                    return p
            else:
                stable = 0
            last = p; time.sleep(4)
        return self.pane()

    def turn(self, msg: str, timeout: int) -> str:
        """One user turn: send the message, wait for the agent to finish, return
        the new text it produced this turn."""
        before = set(self.pane().splitlines())
        self._raw(msg.replace("\n", " ").strip()); time.sleep(0.7); self._enter()
        time.sleep(5)                            # let the turn start (spinner appears)
        after = self.wait_idle(timeout)
        new = [l for l in after.splitlines() if l.strip() and l not in before]
        return "\n".join(new)

    def cost(self) -> float:
        self._raw("/cost"); time.sleep(0.5); self._enter(); time.sleep(4)
        m = re.search(r"Total cost:\s*\$([0-9.]+)", self.pane())
        return float(m.group(1)) if m else 0.0

    def full_transcript(self) -> str:
        return self.pane()

    def close(self) -> float:
        c = self.cost()
        subprocess.run(["tmux", "kill-session", "-t", self.s], capture_output=True)
        return c
