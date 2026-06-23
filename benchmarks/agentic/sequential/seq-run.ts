#!/usr/bin/env bun
// "Where kittens-crew shines" benchmark: a sequence of DEPENDENT tasks on one
// evolving repo. Each task is a fresh agent invocation (context resets between
// tasks). A spec pipeline can carry the integer-cents invariant forward in
// SPEC.md and avoid regressing earlier work; a memoryless agent re-derives it
// and may break it. We grade with `bun test` after each task.
//
// FAIRNESS (hard rule): each arm runs the WHOLE sequence on its OWN fresh copy,
// fully isolated. One kit's behaviour never touches another's run. The user's
// global plugins/skills/hooks are stripped per run. rtk is enabled ONLY on arms
// that declare it (it's an intrinsic kittens-crew habit) — other arms don't get
// it, exactly as they wouldn't in real life.
import { $ } from "bun";
import { readFileSync, writeFileSync, copyFileSync, existsSync, cpSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const HERE = new URL(".", import.meta.url).pathname;
const cfg = JSON.parse(readFileSync(process.env.SEQ_CONFIG || join(HERE, "config.json"), "utf8"));
const tasksCfg = JSON.parse(readFileSync(join(HERE, "tasks.json"), "utf8"));
const TASKS: string[] = tasksCfg.tasks;
const SETTINGS = join(homedir(), ".claude", "settings.json");
const BACKUP = join(tmpdir(), `kc-seq-backup-${process.pid}.json`);
const rtkOnPath = (() => { try { return Bun.spawnSync(["which", "rtk"]).success; } catch { return false; } })();

const expand = (p: string | null): string | null => {
  if (!p) return null;
  let r = p.startsWith("~") ? join(homedir(), p.slice(1)) : p.startsWith("../") ? join(HERE, p) : p;
  if (r.includes("*")) for (const m of new Bun.Glob(r).scanSync({ absolute: true })) return m;
  return existsSync(r) ? r : null;
};

let stripped = false;
function strip() {
  copyFileSync(SETTINGS, BACKUP);
  const d = JSON.parse(readFileSync(SETTINGS, "utf8"));
  delete d.hooks; d.enabledPlugins = {};
  writeFileSync(SETTINGS, JSON.stringify(d, null, 2));
  stripped = true;
  console.log(`isolated. backup: ${BACKUP}\n  restore: cp "${BACKUP}" "${SETTINGS}"\n  rtk on PATH: ${rtkOnPath}\n`);
}
function restore() { if (stripped && existsSync(BACKUP)) { copyFileSync(BACKUP, SETTINGS); stripped = false; console.log("\nrestored settings."); } }
process.on("exit", restore);
for (const s of ["SIGINT", "SIGTERM", "SIGHUP"] as const) process.on(s, () => { restore(); process.exit(1); });

const passCount = async (dir: string): Promise<number> => {
  const out = await $`bun test`.cwd(dir).quiet().nothrow().then((r) => r.stdout.toString() + r.stderr.toString());
  const m = out.match(/(\d+)\s+pass/);
  return m ? +m[1] : 0;
};

async function runArm(arm: any) {
  const work = join(HERE, ".work", arm.name);
  rmSync(work, { recursive: true, force: true });
  cpSync(join(HERE, tasksCfg.repoDir), work, { recursive: true });
  await $`git -C ${work} init -q && git -C ${work} add -A && git -C ${work} -c user.email=b@b -c user.name=b commit -q -m base`.quiet();
  const appendPath = expand(arm.appendFile);
  let sys = appendPath ? readFileSync(appendPath, "utf8") : "";
  if (arm.appendExtra) sys += (sys ? "\n\n" : "") + arm.appendExtra;
  const useRtk = !!arm.rtk && rtkOnPath; // only arms that declare rtk, and only if installed

  let tokens = 0, sec = 0, regressions = 0, prevPass = 0;
  const perTask: any[] = [];
  for (let i = 0; i < TASKS.length; i++) {
    const prompt =
      "You are working in this git repository across a SERIES of tasks; earlier work must keep passing. " +
      (useRtk ? "`rtk` is installed — wrap commands with verbose output in it (`rtk bun test`). " : "") +
      "Implement by EDITING FILES, run the tests, stop when done.\n\nTask " + (i + 1) + ": " + TASKS[i];
    const args = ["-p", prompt, "--model", cfg.model, "--dangerously-skip-permissions", "--output-format", "json", "--max-turns", "50"];
    if (sys) args.push("--append-system-prompt", sys);
    const t0 = Date.now();
    const proc = Bun.spawn(["claude", ...args], { cwd: work, stdout: "pipe", stderr: "pipe" });
    const out = await new Response(proc.stdout).text();
    await proc.exited;
    sec += (Date.now() - t0) / 1000;
    let j: any; try { j = JSON.parse(out); } catch { j = {}; }
    const u = j.usage ?? {};
    tokens += (u.input_tokens ?? 0) + (u.output_tokens ?? 0);
    const pass = await passCount(work);
    if (pass < prevPass) regressions += prevPass - pass; // a previously-green test went red
    perTask.push({ task: i + 1, pass, tokens: Math.round(tokens), regressedHere: Math.max(0, prevPass - pass) });
    console.log(`  ${arm.name} T${i + 1}: pass ${pass}  cumTok ${Math.round(tokens)}  ${pass < prevPass ? "REGRESSED " + (prevPass - pass) : ""}`);
    prevPass = pass;
  }
  return { arm: arm.name, totalTokens: Math.round(tokens), totalSec: Math.round(sec), regressions, finalPass: prevPass, rtk: useRtk, perTask };
}

const results: any[] = [];
try {
  strip();
  for (const arm of cfg.arms) {
    if (arm.appendFile && !expand(arm.appendFile)) { console.log(`skip ${arm.name}: skill file missing`); continue; }
    console.log(`\narm: ${arm.name}`);
    results.push(await runArm(arm));
  }
} finally { restore(); }

writeFileSync(join(HERE, "results-seq.json"), JSON.stringify({ meta: { model: cfg.model, tasks: TASKS.length, rtkOnPath }, arms: results }, null, 2));
console.log("\nwrote results-seq.json");
console.table(results.map((r) => ({ arm: r.arm, finalPass: r.finalPass, regressions: r.regressions, totalTokens: r.totalTokens, rtk: r.rtk })));
