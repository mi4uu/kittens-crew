#!/usr/bin/env python3
"""agency harness — runs each arm through an interactive, isolated Rust task.

For each arm: fresh workspace; multi-turn conversation driven with
`claude --resume`; a memoized human oracle answers clarifying questions
(identical answers across arms); scripted twists land at the right stages; then
we build/test/doc the result and record telemetry. Judging is separate (judge.py).

Isolation: --setting-sources project,local --strict-mcp-config excludes your
global ~/.claude, --append-system-prompt injects only the arm's skill.
"""
import json, os, re, subprocess, sys, time, shutil, glob, datetime, hashlib, threading
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
from tmux_driver import TmuxSession

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
BANK_LOCK = threading.Lock()  # guards answers + answers.json under parallel arms
WORKERS = int(os.environ.get("AGENCY_WORKERS", "1"))

def norm(q: str) -> str:
    return re.sub(r"[^a-z0-9 ]", "", q.lower()).strip()

SHORTCUT_PATTERNS = [
    (r"\btodo!\s*\(", "todo!()"),
    (r"\bunimplemented!\s*\(", "unimplemented!()"),
    (r"\bunreachable!\s*\(", "unreachable!()"),
    (r"//\s*(TODO|FIXME|HACK|XXX|STUB)\b", "TODO/FIXME/HACK comment"),
    (r"panic!\s*\(\s*\"[^\"]*(not implemented|todo|unimplemented|placeholder)", "panic: not-implemented"),
    (r"\b(placeholder|dummy data|mock(ed)?|stubbed?|not[ _-]?implemented|hard[ -]?cod|for now|temporar|FIXME)\b", "placeholder/mock/hardcode wording"),
]
def scan_shortcuts(proj: Path) -> dict:
    """Hunt fake delivery: stubs, mocks, placeholders, hardcoded returns, TODO/FIXME.
    Scans source (not tests). Returns {count, hits:[{file,line,text,kind}]}."""
    hits = []
    srcs = [p for p in proj.rglob("*.rs") if "target/" not in str(p) and "/tests/" not in str(p) and not p.name.endswith("test.rs")]
    for p in srcs:
        try:
            for i, line in enumerate(p.read_text().splitlines(), 1):
                for pat, kind in SHORTCUT_PATTERNS:
                    if re.search(pat, line, re.I):
                        hits.append({"file": str(p.relative_to(proj)), "line": i, "text": line.strip()[:160], "kind": kind})
                        break
        except Exception:
            pass
    return {"count": len(hits), "hits": hits}

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

def claude(workspace: Path, message: str, skill: str, resume=None, max_turns=60) -> dict:
    """One user turn in the interactive session. Returns parsed result JSON
    (session_id, cost, usage). Multi-turn is via --resume, so the agent converses
    turn-by-turn like a real session — not one autonomous shot."""
    args = ["claude", "-p", message, "--model", AGENT_MODEL,
            "--setting-sources", "project,local", "--strict-mcp-config",
            "--dangerously-skip-permissions", "--output-format", "json", "--max-turns", str(max_turns)]
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

def extract_questions(agent_text: str) -> list:
    """Pull the genuine clarifying questions the agent asked the USER (empty if none)."""
    if "?" not in agent_text:
        return []
    sys_p = ("From the assistant message, extract ONLY genuine clarifying questions it is asking "
             "the USER to decide (not rhetorical, not already self-answered, not status updates). "
             'Output STRICT JSON {"questions":["...verbatim..."]}. Empty list if none.')
    args = ["claude", "-p", agent_text[:6000], "--model", PROXY_MODEL, "--setting-sources",
            "project,local", "--strict-mcp-config", "--append-system-prompt", sys_p,
            "--output-format", "json", "--max-turns", "2"]
    p = subprocess.run(args, capture_output=True, text=True)
    try:
        res = json.loads(p.stdout)["result"]
        return json.loads(re.search(r"\{.*\}", res, re.S).group(0)).get("questions", [])
    except Exception:
        return []

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
            # concurrency-safe chat-bridge: per-question files keyed by hash, so
            # parallel arms asking the SAME question dedupe to one human prompt.
            h = hashlib.sha1(k.encode()).hexdigest()[:12]
            qf = HERE / f"_oracle_q_{h}.json"; af = HERE / f"_oracle_a_{h}.json"
            with BANK_LOCK:
                if k in answers:                 # answered by another arm meanwhile
                    parts.append(f"{q}\n> {answers[k]}"); continue
                if not qf.exists():              # first arm to ask this distinct question
                    qf.write_text(json.dumps({"question": q, "arm": "?"}, ensure_ascii=False))
                    print(f"\n  ❓ ORACLE waiting (bridged): {q}", flush=True)
            while not af.exists():               # all arms asking this Q wait on one answer
                time.sleep(3)
            a = json.loads(af.read_text())["answer"].strip()
            with BANK_LOCK:
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
    skill = resolve_skill(arm)  # extra prompt text (yagni strings, 'Be brief.')
    plugin_dir = None           # the FULL kit plugin (kittens-crew, ponytail, caveman)
    if arm.get("pluginDir"):
        pd = os.path.expanduser(arm["pluginDir"])
        if pd.startswith("../"):
            pd = str((HERE / arm["pluginDir"]).resolve())
        hits = glob.glob(pd)
        plugin_dir = hits[0] if hits else (pd if os.path.exists(pd) else None)
        if not plugin_dir:
            print(f"  skip {name}: plugin dir not found ({arm['pluginDir']})"); return None
    print(f"\n=== arm: {name} === (plugin: {plugin_dir or '—'})")
    transcript, telem = [], {"cost_usd": 0.0, "turns": 0, "wall_s": 0.0, "doc_twist": {}}
    out_dir = RUNS / stamp / name
    skill_file = None
    if skill:
        skill_file = out_dir / "_skill.txt"; skill_file.write_text(skill)
    # REAL interactive Claude Code via tmux — driven turn-by-turn like a user.
    sess = TmuxSession(name, ws, skill_file, AGENT_MODEL, plugin_dir=plugin_dir)
    def turn(msg, timeout, phase):
        transcript.append({"role": "user", "text": msg, "phase": phase})
        t0 = time.time()
        txt = sess.turn(msg, timeout)
        telem["turns"] += 1; telem["wall_s"] += time.time() - t0
        transcript.append({"role": "agent", "text": txt, "phase": phase})
        print(f"  {name} [{phase}] turn {telem['turns']} ({len(txt)} chars)", flush=True)
        return txt

    # PHASE 1 — PLAN, no code. Agent should propose an approach and ASK about the
    # real forks. Building now (ignoring 'no code') is a consultation failure (judged).
    out = turn(brief + "  [Before any code, let's talk.] Propose how you'd build this and ASK me "
               "about anything you genuinely need decided (formats, storage, sync vs async, output, "
               "config, commands, scale). Do NOT write code yet — just the plan and your questions.",
               150, "plan")
    for _ in range(3):  # up to 3 Q&A rounds, answered by the REAL user via the bridge
        qs = extract_questions(out)
        if not qs:
            break
        out = turn(oracle_answer(qs) + "  Update the plan if needed, then tell me when you're ready to build.", 150, "plan")

    # PHASE 2 — BUILD
    turn("Good — go ahead and build it. Get it compiling with the core covered by tests.", 900, "build")
    # PHASE 3 — TWISTS injected mid-stream like a real user
    for tw in [t for t in scenario["twists"] if t["id"] in ("scale", "filter")]:
        turn(tw["message"], 600, "twist:" + tw["id"])
    # PHASE 4 — DOCS (withheld until now) — measure the extra effort it costs
    telem["doc_twist"]["turns_before"] = telem["turns"]
    docs = next(t for t in scenario["twists"] if t["id"] == "docs")
    turn(docs["message"], 600, "docs")
    telem["doc_twist"]["turns_after"] = telem["turns"] - telem["doc_twist"]["turns_before"]
    (out_dir / "tui-pane.txt").write_text(sess.full_transcript())
    telem["cost_usd"] = sess.close()

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
    telem["shortcuts"] = scan_shortcuts(proj)  # stubs/mocks/placeholders/TODO — fake-delivery detector

    out = RUNS / stamp / name
    (out / "transcript.jsonl").write_text("\n".join(json.dumps(x, ensure_ascii=False) for x in transcript))
    telem["cost_usd"] = round(telem["cost_usd"], 4); telem["wall_s"] = round(telem["wall_s"])
    (out / "telemetry.json").write_text(json.dumps(telem, indent=2))
    print(f"  done {name}: build {telem['build_ok']} tests {telem['tests_pass']}/{telem['tests_pass']+telem['tests_fail']} loc {telem['src_loc']} cost ${telem['cost_usd']} turns {telem['turns']}")
    return telem

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--scan":
        run = Path(sys.argv[2])
        for arm_dir in sorted(p for p in run.iterdir() if p.is_dir()):
            ws = arm_dir / "workspace"
            cargos = sorted(ws.rglob("Cargo.toml"), key=lambda p: len(p.parts))
            proj = cargos[0].parent if cargos else ws
            sc = scan_shortcuts(proj)
            tel_p = arm_dir / "telemetry.json"
            tel = json.loads(tel_p.read_text()) if tel_p.exists() else {}
            tel["shortcuts"] = sc; tel_p.write_text(json.dumps(tel, indent=2))
            print(f"\n=== {arm_dir.name}: {sc['count']} shortcuts/stubs/placeholders ===")
            for h in sc["hits"]:
                print(f"  {h['file']}:{h['line']}  [{h['kind']}]  {h['text']}")
        sys.exit(0)
    stamp = sys.argv[1] if len(sys.argv) > 1 else datetime.datetime.now().strftime("%Y%m%d-%H%M")
    print(f"agency run {stamp} — {len(arms)} arms, agent={AGENT_MODEL}, proxy={PROXY_MODEL}, workers={WORKERS}")
    print("you'll be asked to answer any NEW clarifying question (reused for all arms).\n")
    if WORKERS > 1:
        with ThreadPoolExecutor(max_workers=WORKERS) as ex:
            list(ex.map(lambda a: run_arm(a, stamp), arms))
    else:
        for arm in arms:
            run_arm(arm, stamp)
    print(f"\nall arms done -> runs/{stamp}/  . next: uv run judge.py runs/{stamp}")
