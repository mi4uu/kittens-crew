#!/usr/bin/env python3
"""Per-arm cost + a simplified human-readable story from each transcript.

A 'turn' here = one assistant API response (one model generation). Tokens are the
real cost: we sum input (uncached), cache-creation, cache-read, and output across
every turn.
"""
import json, sys, glob, os

HERE = os.path.dirname(os.path.abspath(__file__))
ARMS = ["baseline", "kittens", "cavekit", "ponytail"]

def short(s, n=90):
    s = " ".join(s.split())
    return s[:n] + ("…" if len(s) > n else "")

def analyze(arm):
    path = os.path.join(HERE, f"{arm}-transcript.jsonl")
    if not os.path.exists(path):
        return None
    turns = 0
    inp = cc = cr = out = 0
    story = []
    for line in open(path):
        try:
            o = json.loads(line)
        except Exception:
            continue
        msg = o.get("message", {})
        role = msg.get("role")
        content = msg.get("content", [])
        # USER text (skip tool_result echoes)
        if role == "user":
            if isinstance(content, str):
                story.append(("USER", short(content)))
            elif isinstance(content, list):
                for b in content:
                    if isinstance(b, dict) and b.get("type") == "text":
                        story.append(("USER", short(b["text"])))
        # ASSISTANT: count a turn, tally usage, capture text + tool calls
        if role == "assistant":
            u = msg.get("usage")
            if u:
                turns += 1
                inp += u.get("input_tokens", 0)
                cc  += u.get("cache_creation_input_tokens", 0)
                cr  += u.get("cache_read_input_tokens", 0)
                out += u.get("output_tokens", 0)
            if isinstance(content, list):
                for b in content:
                    if not isinstance(b, dict):
                        continue
                    if b.get("type") == "text" and b.get("text", "").strip():
                        story.append(("AGENT", short(b["text"])))
                    elif b.get("type") == "tool_use":
                        name = b.get("name", "?")
                        inp_ = b.get("input", {})
                        hint = inp_.get("command") or inp_.get("file_path") or inp_.get("description") or inp_.get("prompt") or ""
                        story.append(("TOOL", f"{name}: {short(str(hint), 70)}"))
    total = inp + cc + cr + out
    return dict(arm=arm, turns=turns, inp=inp, cc=cc, cr=cr, out=out, total=total, story=story)

rows = [analyze(a) for a in ARMS]
rows = [r for r in rows if r]

print(f"{'arm':9} {'turns':>5} {'in':>7} {'cache_cr':>9} {'cache_rd':>9} {'out':>7} {'TOTAL':>9}")
for r in rows:
    print(f"{r['arm']:9} {r['turns']:5d} {r['inp']:7d} {r['cc']:9d} {r['cr']:9d} {r['out']:7d} {r['total']:9d}")

# write each story file
for r in rows:
    p = os.path.join(HERE, f"{r['arm']}-story.txt")
    with open(p, "w") as f:
        f.write(f"# {r['arm']} — simplified story ({r['turns']} turns, {r['total']:,} tokens)\n\n")
        for kind, txt in r["story"]:
            f.write(f"{kind:5} │ {txt}\n")
    print(f"  story → {os.path.basename(p)} ({len(r['story'])} steps)")
