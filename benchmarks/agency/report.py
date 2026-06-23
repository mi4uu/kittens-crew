#!/usr/bin/env python3
"""Aggregate judges + telemetry into results/<date>-agency.md (committed).

  uv run report.py runs/<stamp>

Final score per dimension = mean of available judges (opus, nemotron, gemini).
Disagreement (range >= 2) is flagged so the judges' notes get read, not ignored.
"""
import json, sys, datetime
from pathlib import Path

HERE = Path(__file__).parent
DIMS = ["intent_coverage", "consultation_balance", "tech_choice", "delivery",
        "code_quality", "testability", "plan_adaptation", "docs_readiness", "visible_plan"]
JUDGES = ["judge-opus", "judge-nemotron", "judge-gemini"]

def load(p):
    try: return json.loads(Path(p).read_text())
    except Exception: return {}

def main():
    run = Path(sys.argv[1])
    arms = sorted(p.name for p in run.iterdir() if p.is_dir())
    rows, detail, flags = [], [], []
    for arm in arms:
        sc = {j: load(run / arm / "scores" / f"{j}.json") for j in JUDGES}
        tel = load(run / arm / "telemetry.json")
        means = {}
        for d in DIMS:
            vals = [sc[j].get("scores", {}).get(d) for j in JUDGES if isinstance(sc[j].get("scores", {}).get(d), (int, float))]
            means[d] = round(sum(vals) / len(vals), 1) if vals else None
            if vals and (max(vals) - min(vals)) >= 2:
                flags.append(f"- **{arm} / {d}**: judges disagree ({vals}) — read notes below.")
        tot = round(sum(v for v in means.values() if v is not None), 1)
        rows.append((arm, means, tot, tel))
        # per-arm judge notes
        notes = [f"### {arm}  (total {tot})"]
        notes.append(f"telemetry: build={tel.get('build_ok')} tests={tel.get('tests_pass')}/{tel.get('tests_pass',0)+tel.get('tests_fail',0)} "
                     f"loc={tel.get('src_loc')} cost=${tel.get('cost_usd')} turns={tel.get('turns')} doc_twist={tel.get('doc_twist')}")
        for j in JUDGES:
            s = sc[j]
            if not s or s.get("error"):
                notes.append(f"- _{j}_: (missing{': ' + s.get('error','') if s.get('error') else ''})"); continue
            notes.append(f"- _{j}_ — {s.get('summary','')}")
            for d in DIMS:
                n = s.get("notes", {}).get(d)
                if n: notes.append(f"    - {d} ({s.get('scores',{}).get(d)}): {n}")
        detail.append("\n".join(notes))

    rows.sort(key=lambda r: -r[2])
    hdr = "| arm | " + " | ".join(d.replace("_", " ") for d in DIMS) + " | **total** |"
    sep = "|" + "---|" * (len(DIMS) + 2)
    body = "\n".join("| " + a + " | " + " | ".join(str(m[d]) if m[d] is not None else "·" for d in DIMS) + f" | **{t}** |" for a, m, t, _ in rows)
    tel_hdr = "| arm | build | tests | src_loc | cost $ | turns | doc-twist turns |"
    tel_body = "\n".join(f"| {a} | {'✅' if tel.get('build_ok') else '❌'} | {tel.get('tests_pass')}/{tel.get('tests_pass',0)+tel.get('tests_fail',0)} | {tel.get('src_loc')} | {tel.get('cost_usd')} | {tel.get('turns')} | {tel.get('doc_twist',{}).get('turns_after','·')} |" for a, _, _, tel in rows)

    date = datetime.datetime.now().strftime("%Y-%m-%d")
    md = f"""# agency benchmark — {date}

Underspecified Rust task (`feedcat`), 7 arms, isolated (`--setting-sources
project,local`), memoized human oracle, scripted twists incl the withheld docs
request. Scored 0–3 per dimension by three blind judges (Opus local, Gemini Pro,
Nemotron via OpenRouter); cell = mean of available judges.

## scores (mean of judges)

{hdr}
{sep}
{body}

## objective telemetry

{tel_hdr}
|---|---|---|---|---|---|---|
{tel_body}

## judge disagreements (range ≥ 2)

{chr(10).join(flags) if flags else "_none — judges broadly agreed._"}

## judges' notes (verbatim, per arm)

{chr(10).join(detail)}

---
_Generated from `{run}` by report.py. Whatever this shows is published as-is,
including judges' critical notes. n=1 — directional._
"""
    out = HERE / "results" / f"{date}-agency.md"
    out.parent.mkdir(exist_ok=True)
    out.write_text(md)
    print(f"wrote {out}")

if __name__ == "__main__":
    main()
