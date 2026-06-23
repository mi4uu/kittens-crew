#!/usr/bin/env python3
"""agency harness — runs each arm through an interactive, isolated Rust task.

For each arm: fresh workspace; multi-turn conversation driven with
`claude --resume`; a memoized human oracle answers clarifying questions
(identical answers across arms); scripted twists land at the right stages; then
we build/test/doc the result and record telemetry. Judging is separate (judge.py).

Isolation: --setting-sources project,local --strict-mcp-config excludes your
global ~/.claude, --append-system-prompt injects only the arm's skill.
"""
import json, os, re, subprocess, sys, time, shutil, glob, datetime
from pathlib import Path

HERE = Path(__file__).parent
RUNS = HERE / "runs"
ANSWERS = HERE / "answers.json"
PROXY_MODEL = "haiku"      # plays the user / decides next message (cheap, fixed)
AGENT_MODEL = "sonnet"     # the kit under test
MAX_TURNS = 24             # safety bound on the conversation

def load(p, default=None):
    return json.loads(Path(p).read_text()) if Path(p).exists() else default

scenario = load(HERE / "scenario.json")
arms = load(HERE / "arms.json")["arms"]
if os.environ.get("AGENCY_ARMS"):  # shakeout / subset: comma-separated arm names
    keep = set(os.environ["AGENCY_ARMS"].split(","))
    arms = [a for a in arms if a["name"] in keep]
brief = Path(os.environ["AGENCY_BRIEF_FILE"]).read_text() if os.environ.get("AGENCY_BRIEF_FILE") else (HERE / "brief.md").read_text()
AUTO = os.environ.get("AGENCY_AUTO")  # shakeout: auto-answer unknown oracle questions, no human
answers = load(ANSWERS, {})   # normalized-question -> human answer (persists across arms)

def norm(q: str) -> str:
    return re.sub(r"[^a-z0-9 ]", "", q.lower()).strip()

def resolve_skill(arm) -> str:
    txt = ""
    if arm.get("appendFile"):
        pat = os.path.expanduser(arm["appendFile"])
        if pat.startswith("../"):
            pat = str((HERE / arm["appendFile"]).resolve())
        hits = glob.glob(pat)
        if hits:
            txt = Path(hits[0]).read_text()
        elif "*" not in pat and Path(pat).exists():
            txt = Path(pat).read_text()
    if arm.get("appendText"):
        txt = (txt + "\n\n" + arm["appendText"]).strip()
    return txt

def claude(workspace: Path, message: str, skill: str, resume=None) -> dict:
    """One agent turn. Returns parsed result JSON (incl session_id, cost, usage)."""
    args = ["claude", "-p", message, "--model", AGENT_MODEL,
            "--setting-sources", "project,local", "--strict-mcp-config",
            "--dangerously-skip-permissions", "--output-format", "json", "--max-turns", "60"]
    if skill:
        args += ["--append-system-prompt", skill]
    if resume:
        args += ["--resume", resume]
    t0 = time.time()
    p = subprocess.run(args, cwd=workspace, capture_output=True, text=True)
    try:
        j = json.loads(p.stdout)
    except Exception:
        j = {"result": "", "is_error": True, "raw": p.stdout[:500], "stderr": p.stderr[:500]}
    j["_wall_s"] = round(time.time() - t0, 1)
    return j

def proxy_decide(transcript: list, stage: str) -> dict:
    """The user-proxy reads the convo and decides the next user message.
    Returns {action: ask|twist|continue|end, message, questions:[...]}."""
    sys_p = (
        "You are role-playing THE USER in a coding session, per this persona:\n"
        + json.dumps(scenario["userProxy"]) +
        "\nYou DO NOT volunteer the project's open choices. You answer only what is asked.\n"
        "Given the conversation and the current stage, output STRICT JSON:\n"
        '{"action":"ask|twist|continue|end","questions":["verbatim clarifying questions the agent asked, if any"],'
        '"message":"the user message to send next (leave empty if action=ask or end)"}\n'
        "- action=ask: the agent asked clarifying question(s) that need the real human. List them in questions.\n"
        "- action=twist: only the harness injects twists; never emit this yourself.\n"
        "- action=continue: nudge the agent to proceed (e.g. 'looks good, go ahead').\n"
        "- action=end: the agent has fully finished the CURRENT stage's work.\n"
        f"Current stage: {stage}."
    )
    convo = "\n\n".join(f"[{m['role']}]: {m['text'][:4000]}" for m in transcript[-6:])
    args = ["claude", "-p", convo, "--model", PROXY_MODEL, "--setting-sources", "project,local",
            "--strict-mcp-config", "--append-system-prompt", sys_p, "--output-format", "json", "--max-turns", "2"]
    p = subprocess.run(args, capture_output=True, text=True)
    try:
        res = json.loads(p.stdout)["result"]
        return json.loads(re.search(r"\{.*\}", res, re.S).group(0))
    except Exception:
        return {"action": "continue", "questions": [], "message": "Please continue."}

def oracle_answer(questions: list) -> str:
    """Memoized human oracle: reuse stored answers; ask the real user for new ones."""
    parts = []
    for q in questions:
        k = norm(q)
        if k in answers:
            parts.append(f"{q}\n> {answers[k]}")
        elif AUTO:
            parts.append(f"{q}\n> Use your best judgement; keep it simple.")  # shakeout stand-in
        elif os.environ.get("AGENCY_BRIDGE"):
            # chat-bridge: write the question, block until the assistant writes the answer
            pend = HERE / "_oracle_pending.json"; ans = HERE / "_oracle_answer.json"
            pend.write_text(json.dumps({"question": q}, ensure_ascii=False))
            print(f"\n  ❓ ORACLE waiting (bridged to chat): {q}", flush=True)
            while not ans.exists():
                time.sleep(3)
            a = json.loads(ans.read_text())["answer"].strip()
            ans.unlink(); pend.unlink(missing_ok=True)
            answers[k] = a
            ANSWERS.write_text(json.dumps(answers, indent=2, ensure_ascii=False))
            parts.append(f"{q}\n> {a}")
        else:
            print(f"\n  ❓ ORACLE (a kit asks — your answer is reused for all arms):\n     {q}")
            a = input("     your answer: ").strip()
            answers[k] = a
            ANSWERS.write_text(json.dumps(answers, indent=2, ensure_ascii=False))
            parts.append(f"{q}\n> {a}")
    return "\n\n".join(parts)

# ---- stages drive twist injection -----------------------------------------
TWISTS = {t["after"]: t for t in scenario["twists"]}
STAGE_ORDER = ["plan-accepted", "implementation-builds", "everything-done"]

def run_arm(arm, stamp):
    name = arm["name"]
    ws = RUNS / stamp / name / "workspace"
    ws.mkdir(parents=True, exist_ok=True)
    skill = resolve_skill(arm)
    if (arm.get("appendFile") and not skill):
        print(f"  skip {name}: skill file not found"); return None
    print(f"\n=== arm: {name} ===")
    transcript, telem = [], {"cost_usd": 0.0, "turns": 0, "wall_s": 0.0, "doc_twist": {}}
    transcript.append({"role": "user", "text": brief})
    r = claude(ws, brief, skill)
    sid = r.get("session_id")
    stage_i = 0
    for _ in range(MAX_TURNS):
        telem["turns"] += 1; telem["cost_usd"] += r.get("total_cost_usd", 0) or 0; telem["wall_s"] += r.get("_wall_s", 0)
        transcript.append({"role": "agent", "text": r.get("result", "")})
        stage = STAGE_ORDER[min(stage_i, len(STAGE_ORDER) - 1)]
        d = proxy_decide(transcript, stage)
        if d["action"] == "ask" and d.get("questions"):
            msg = oracle_answer(d["questions"])
        elif d["action"] == "end":
            # advance to the next stage's twist, or finish after the docs twist
            tw = TWISTS.get(STAGE_ORDER[stage_i]) if stage_i < len(STAGE_ORDER) else None
            if tw:
                if tw["id"] == "docs":  # measure the extra effort this twist costs
                    telem["doc_twist"]["turns_before"] = telem["turns"]
                msg = tw["message"]; transcript.append({"role": "user", "text": "[TWIST] " + msg})
                stage_i += 1
            else:
                break
        else:
            msg = d.get("message") or "Please continue."
        transcript.append({"role": "user", "text": msg})
        r = claude(ws, msg, skill, resume=sid)
    if "turns_before" in telem["doc_twist"]:
        telem["doc_twist"]["turns_after"] = telem["turns"] - telem["doc_twist"]["turns_before"]

    # ---- objective build/test/doc + size --------------------------------
    # the agent usually runs `cargo new <name>`, so the project is in a subdir.
    cargos = sorted(ws.rglob("Cargo.toml"), key=lambda p: len(p.parts))
    proj = cargos[0].parent if cargos else ws
    telem["proj_dir"] = str(proj.relative_to(ws)) or "."
    def sh(c): return subprocess.run(c, cwd=proj, shell=True, capture_output=True, text=True)
    telem["build_ok"] = sh("cargo build 2>&1").returncode == 0
    t = sh("cargo test 2>&1").stdout + sh("cargo test 2>&1").stderr
    m = re.search(r"(\d+) passed.*?(\d+) failed", t)
    telem["tests_pass"], telem["tests_fail"] = (int(m[1]), int(m[2])) if m else (0, 0)
    tok = sh("tokei src --output json 2>/dev/null")
    try:
        tk = json.loads(tok.stdout); telem["src_loc"] = tk.get("Total", {}).get("code", 0)
    except Exception:
        telem["src_loc"] = 0
    sh("cargo doc --no-deps 2>&1")
    doc_dir = proj / "target" / "doc"
    if doc_dir.exists():
        shutil.copytree(doc_dir, RUNS / stamp / name / "cargo-doc", dirs_exist_ok=True)

    out = RUNS / stamp / name
    (out / "transcript.jsonl").write_text("\n".join(json.dumps(x, ensure_ascii=False) for x in transcript))
    telem["cost_usd"] = round(telem["cost_usd"], 4); telem["wall_s"] = round(telem["wall_s"])
    (out / "telemetry.json").write_text(json.dumps(telem, indent=2))
    print(f"  done {name}: build {telem['build_ok']} tests {telem['tests_pass']}/{telem['tests_pass']+telem['tests_fail']} loc {telem['src_loc']} cost ${telem['cost_usd']} turns {telem['turns']}")
    return telem

if __name__ == "__main__":
    stamp = sys.argv[1] if len(sys.argv) > 1 else datetime.datetime.now().strftime("%Y%m%d-%H%M")
    print(f"agency run {stamp} — {len(arms)} arms, agent={AGENT_MODEL}, proxy={PROXY_MODEL}")
    print("you'll be asked to answer any NEW clarifying question (reused for all arms).\n")
    for arm in arms:
        run_arm(arm, stamp)
    print(f"\nall arms done -> runs/{stamp}/  . next: uv run judge.py runs/{stamp}")
