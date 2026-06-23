#!/usr/bin/env python3
"""Score a finished run with three blind judges.

  judge-opus      : local `claude` at claude-opus-4-8, isolated (no kittens skill).
  judge-nemotron  : nvidia/nemotron-3-ultra-550b-a55b:free via OpenRouter.
  judge-gemini    : we emit judge-bundles/<arm>.zip + a prompt; you paste into
                    Gemini Pro and record the verdict with `judge.py --gemini`.

Judges are blind to the arm name and to each other. Usage:
  uv run judge.py runs/<stamp>                 # opus + nemotron + build gemini bundles
  uv run judge.py --gemini runs/<stamp> <arm> verdict.json   # record a Gemini verdict
"""
import json, os, sys, re, subprocess, zipfile, urllib.request
from pathlib import Path

HERE = Path(__file__).parent
RUBRIC = (HERE / "rubric.md").read_text()
DIMS = ["intent_coverage", "consultation_balance", "tech_choice", "delivery",
        "code_quality", "testability", "plan_adaptation", "docs_readiness", "visible_plan"]

def load_env():
    f = HERE / ".env"
    if f.exists():
        for line in f.read_text().splitlines():
            if "=" in line and not line.strip().startswith("#"):
                k, v = line.split("=", 1); os.environ.setdefault(k.strip(), v.strip())
load_env()

JUDGE_SYS = (
    "You are a strict, fair senior engineer judging ONE coding agent's run, BLIND "
    "to which tool produced it. Score the rubric below 0-3 per dimension and JUSTIFY "
    "every score by citing specifics from the evidence. Penalise both assuming "
    "non-obvious choices without asking AND stalling/rabbit-holing on questions "
    "nobody needed. Reward docs written as-the-code-went (cargo doc useful with no "
    "rework). Output STRICT JSON only:\n"
    '{"scores":{' + ",".join(f'"{d}":0' for d in DIMS) + '},"notes":{' +
    ",".join(f'"{d}":""' for d in DIMS) + '},"summary":""}\n\n=== RUBRIC ===\n' + RUBRIC
)

def evidence(arm_dir: Path) -> str:
    """Blind evidence bundle: transcript + source + doc presence + objective telemetry."""
    out = []
    t = arm_dir / "transcript.jsonl"
    if t.exists():
        msgs = [json.loads(l) for l in t.read_text().splitlines() if l.strip()]
        out.append("## CONVERSATION (planning, asking-vs-assuming, twist handling)\n" +
                   "\n".join(f"[{m['role']}] {m['text'][:1500]}" for m in msgs))
    ws = arm_dir / "workspace"
    # workspace file listing + any user-visible plan artifact (SPEC/PLAN/TODO) — for visible_plan
    files = sorted(str(p.relative_to(ws)) for p in ws.rglob("*") if p.is_file() and "target/" not in str(p.relative_to(ws)))
    out.append("## WORKSPACE FILES (did it leave a visible plan the user can open?)\n" + "\n".join(files[:80]))
    plans = [p for p in ws.glob("*.md")] + [p for p in ws.glob("*.txt")]
    plan_txt = "\n\n".join(f"// {p.name}\n{p.read_text()[:4000]}" for p in plans if re.search(r"spec|plan|todo|task|progress", p.name, re.I))
    if plan_txt:
        out.append("## PLAN / PROGRESS ARTIFACT\n" + plan_txt)
    srcs = sorted(ws.glob("src/**/*.rs")) + sorted(ws.glob("tests/**/*.rs"))
    code = "\n\n".join(f"// FILE: {p.relative_to(ws)}\n{p.read_text()[:6000]}" for p in srcs[:12])
    out.append("## SOURCE\n" + (code or "(none)"))
    tel = arm_dir / "telemetry.json"
    if tel.exists():
        out.append("## OBJECTIVE TELEMETRY\n" + tel.read_text())
    out.append("## DOCS\ncargo-doc generated: " + str((arm_dir / "cargo-doc").exists()) +
               " (judge docs_readiness from /// comments in SOURCE: are they intent+examples, or absent?)")
    return "\n\n".join(out)[:90000]

def parse_json(s: str) -> dict:
    m = re.search(r"\{.*\}", s, re.S)
    return json.loads(m.group(0)) if m else {}

def judge_opus(ev: str) -> dict:
    args = ["claude", "-p", ev, "--model", "claude-opus-4-8", "--setting-sources", "project,local",
            "--strict-mcp-config", "--append-system-prompt", JUDGE_SYS, "--output-format", "json", "--max-turns", "2"]
    p = subprocess.run(args, capture_output=True, text=True)
    try:
        return parse_json(json.loads(p.stdout)["result"])
    except Exception as e:
        return {"error": str(e), "raw": p.stdout[:300]}

def judge_nemotron(ev: str) -> dict:
    key = os.environ.get("OPENROUTER_API_KEY")
    if not key:
        return {"error": "no OPENROUTER_API_KEY in .env"}
    body = json.dumps({
        "model": "nvidia/nemotron-3-ultra-550b-a55b:free",
        "messages": [{"role": "system", "content": JUDGE_SYS}, {"role": "user", "content": ev}],
        "temperature": 0,
    }).encode()
    req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", data=body,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    try:
        r = json.loads(urllib.request.urlopen(req, timeout=180).read())
        return parse_json(r["choices"][0]["message"]["content"])
    except Exception as e:
        return {"error": str(e)}

def gemini_bundle(arm_dir: Path, arm: str, bundles: Path):
    bundles.mkdir(parents=True, exist_ok=True)
    z = bundles / f"{arm}.zip"
    with zipfile.ZipFile(z, "w", zipfile.ZIP_DEFLATED) as zf:
        for p in arm_dir.rglob("*"):
            if p.is_file() and "cargo-doc" not in str(p) and p.stat().st_size < 2_000_000:
                zf.write(p, p.relative_to(arm_dir))
    (bundles / f"{arm}.prompt.txt").write_text(
        "Judge this coding-agent run BLIND (you don't know which tool made it). "
        "Read the zip (transcript.jsonl = the conversation, workspace/src = code, "
        "telemetry.json = objective facts). Score the rubric below 0-3 per dimension, "
        "justify each with specifics, output STRICT JSON only.\n\n" + JUDGE_SYS)
    return z

SYNTH_SYS = (
    "You were one of three independent judges of a coding-agent run. You are now "
    "shown ALL THREE verdicts (judge-opus = your own, plus judge-nemotron and "
    "judge-gemini). Knowing the other two opinions, decide whether you want to "
    "respond: agree, push back where you think a judge mis-scored (say why), "
    "reconcile genuine disagreements, or add a final note. Be specific and fair — "
    "do not just defer to consensus, but do update if another judge caught "
    "something you missed. Output STRICT JSON:\n"
    '{"reaction":"your considered response to the three verdicts",'
    '"adjustments":{"<dimension>":<new 0-3 score you would now give, only if you changed your mind>},'
    '"final_note":"one-paragraph closing take on this arm"}'
)

def synthesize(run: Path):
    """Final step: Opus reads all three verdicts per arm and responds to them."""
    for arm_dir in sorted(p for p in run.iterdir() if p.is_dir()):
        sc = arm_dir / "scores"
        three = {j: (json.loads((sc / f"{j}.json").read_text()) if (sc / f"{j}.json").exists() else {"missing": True})
                 for j in ["judge-opus", "judge-nemotron", "judge-gemini"]}
        ev = "Three verdicts for this arm:\n" + json.dumps(three, indent=2, ensure_ascii=False)[:60000]
        args = ["claude", "-p", ev, "--model", "claude-opus-4-8", "--setting-sources", "project,local",
                "--strict-mcp-config", "--append-system-prompt", SYNTH_SYS, "--output-format", "json", "--max-turns", "2"]
        p = subprocess.run(args, capture_output=True, text=True)
        try:
            res = parse_json(json.loads(p.stdout)["result"])
        except Exception as e:
            res = {"error": str(e), "raw": p.stdout[:300]}
        (sc / "synthesis-opus.json").write_text(json.dumps(res, indent=2, ensure_ascii=False))
        print(f"  synthesized {arm_dir.name}")

def main():
    if sys.argv[1] == "--synthesize":
        synthesize(Path(sys.argv[2])); return
    if sys.argv[1] == "--gemini":
        run, arm, vf = Path(sys.argv[2]), sys.argv[3], sys.argv[4]
        sc = run / arm / "scores"; sc.mkdir(exist_ok=True)
        (sc / "judge-gemini.json").write_text(Path(vf).read_text())
        print(f"recorded gemini verdict for {arm}"); return
    run = Path(sys.argv[1]); bundles = HERE / "judge-bundles" / run.name
    for arm_dir in sorted(p for p in run.iterdir() if p.is_dir()):
        arm = arm_dir.name; sc = arm_dir / "scores"; sc.mkdir(exist_ok=True)
        ev = evidence(arm_dir)
        print(f"judging {arm} ...")
        (sc / "judge-opus.json").write_text(json.dumps(judge_opus(ev), indent=2, ensure_ascii=False))
        (sc / "judge-nemotron.json").write_text(json.dumps(judge_nemotron(ev), indent=2, ensure_ascii=False))
        gemini_bundle(arm_dir, arm, bundles)
        print(f"  opus+nemotron scored; gemini bundle -> {bundles/(arm+'.zip')}")
    print(f"\nnext: paste each judge-bundles/{run.name}/<arm>.zip + .prompt.txt into Gemini Pro,")
    print(f"then: uv run judge.py --gemini {run} <arm> <verdict.json>   ; finally report.py")

if __name__ == "__main__":
    main()
