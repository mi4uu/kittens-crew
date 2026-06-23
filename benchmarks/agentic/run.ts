#!/usr/bin/env bun
// Real agentic benchmark. Headless Claude Code edits a real repo; we score the
// git diff it leaves and the usage JSON it returns. Same agent, same tasks, with
// vs without each skill (injected via --append-system-prompt). No fabrication.
//
// ISOLATION: the user's global plugins/skills/hooks would contaminate every arm,
// so we back up ~/.claude/settings.json, strip `hooks` + `enabledPlugins` for the
// duration, and ALWAYS restore (finally + signal handlers). Each run then sees
// ONLY the skill we inject. The backup path is printed up front for manual
// recovery if anything goes sideways.
//
// Cost is real — each cell is a headless editing session. Run: bun run.ts
import { $ } from "bun";
import { readFileSync, writeFileSync, copyFileSync, existsSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const HERE = new URL(".", import.meta.url).pathname;
const cfg = JSON.parse(readFileSync(process.env.BENCH_CONFIG || join(HERE, "config.json"), "utf8"));
const WORK = join(HERE, ".work");
const REPO = join(WORK, "repo");
const SETTINGS = join(homedir(), ".claude", "settings.json");
const BACKUP = join(tmpdir(), `kc-settings-backup-${process.pid}.json`);

const expand = (p: string | null): string | null => {
  if (!p) return null;
  let r = p.startsWith("~") ? join(homedir(), p.slice(1)) : p.startsWith("../") ? join(HERE, p) : p;
  if (r.includes("*")) for (const m of new Bun.Glob(r.replace(homedir(), "~").replace("~", homedir())).scanSync({ absolute: true })) return m;
  return existsSync(r) ? r : null;
};

// --- isolation: back up + strip, restore no matter what ---------------------
let stripped = false;
function strip() {
  copyFileSync(SETTINGS, BACKUP);
  const d = JSON.parse(readFileSync(SETTINGS, "utf8"));
  delete d.hooks;
  d.enabledPlugins = {};
  writeFileSync(SETTINGS, JSON.stringify(d, null, 2));
  stripped = true;
  console.log(`isolated: stripped hooks+enabledPlugins from live settings.\n  backup: ${BACKUP}\n  restore by hand if needed: cp "${BACKUP}" "${SETTINGS}"\n`);
}
function restore() {
  if (stripped && existsSync(BACKUP)) {
    copyFileSync(BACKUP, SETTINGS);
    stripped = false;
    console.log("\nrestored ~/.claude/settings.json");
  }
}
process.on("exit", restore);
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"] as const)
  process.on(sig, () => { restore(); process.exit(1); });

type Cell = { arm: string; task: number; loc: number; tokens: number; cost: number; sec: number };

async function runCell(arm: { name: string; appendFile: string | null }, taskIdx: number): Promise<Cell | null> {
  await $`git -C ${REPO} reset --hard ${baseSha} -q`.quiet();
  await $`git -C ${REPO} clean -fdq`.quiet();
  const task =
    "You are working inside this git repository. Implement the following by EDITING FILES directly with your tools (do not just describe it). When done, stop.\n\nTask: " +
    cfg.tasks[taskIdx];
  const appendPath = expand(arm.appendFile);
  const args = ["-p", task, "--model", cfg.model, "--dangerously-skip-permissions", "--output-format", "json", "--max-turns", "40"];
  if (appendPath) args.push("--append-system-prompt", readFileSync(appendPath, "utf8"));
  const t0 = Date.now();
  const proc = Bun.spawn(["claude", ...args], { cwd: REPO, stdout: "pipe", stderr: "pipe" });
  const out = await new Response(proc.stdout).text();
  await proc.exited;
  const sec = (Date.now() - t0) / 1000;
  let j: any;
  try { j = JSON.parse(out); } catch { console.log(`  ${arm.name} t${taskIdx}: no JSON, skipped`); return null; }
  if (j.is_error) { console.log(`  ${arm.name} t${taskIdx}: ${String(j.result).slice(0, 60)}`); return null; }
  const u = j.usage ?? {};
  // real work tokens (exclude cache read/creation, which dwarf and mislead)
  const tokens = (u.input_tokens ?? 0) + (u.output_tokens ?? 0);
  await $`git -C ${REPO} add -A`.quiet(); // count new files too
  const shortstat = await $`git -C ${REPO} diff --cached --shortstat`.text();
  const loc = [...shortstat.matchAll(/(\d+) (insertion|deletion)/g)].reduce((s, m) => s + +m[1], 0);
  const cost = j.total_cost_usd ?? 0;
  console.log(`  ${arm.name} t${taskIdx}: LOC ${loc}  tok ${tokens}  $${cost.toFixed(4)}  ${sec.toFixed(0)}s`);
  return { arm: arm.name, task: taskIdx, loc, tokens, cost, sec };
}

// --- setup repo -------------------------------------------------------------
await $`mkdir -p ${WORK}`.quiet();
if (!existsSync(REPO)) await $`git clone --depth 1 ${cfg.repoUrl} ${REPO}`.quiet();
const baseSha = (await $`git -C ${REPO} rev-parse HEAD`.text()).trim();

// --- run --------------------------------------------------------------------
const cells: Cell[] = [];
try {
  strip();
  for (const arm of cfg.arms) {
    if (arm.appendFile && !expand(arm.appendFile)) { console.log(`arm ${arm.name}: skill file not found, skipping arm`); continue; }
    console.log(`\narm: ${arm.name}`);
    for (let t = 0; t < cfg.tasks.length; t++)
      for (let r = 0; r < cfg.runsPerCell; r++) {
        const c = await runCell(arm, t);
        if (c) cells.push(c);
      }
  }
} finally {
  restore();
}

// --- aggregate: median per arm per metric, % of baseline --------------------
const med = (xs: number[]) => { const s = [...xs].sort((a, b) => a - b); return s.length ? s[Math.floor((s.length - 1) / 2)] : 0; };
const arms = [...new Set(cells.map((c) => c.arm))];
const metric = (arm: string, k: keyof Cell) => med(cells.filter((c) => c.arm === arm).map((c) => c[k] as number));
const base = { loc: metric("baseline", "loc"), tokens: metric("baseline", "tokens"), cost: metric("baseline", "cost"), sec: metric("baseline", "sec") };
const results = {
  meta: { repo: cfg.repoUrl, model: cfg.model, tasks: cfg.tasks.length, n: cfg.runsPerCell, baseSha, isolated: "global plugins+hooks stripped per run" },
  base,
  arms: arms.map((a) => ({
    arm: a,
    loc: metric(a, "loc"), tokens: metric(a, "tokens"), cost: metric(a, "cost"), sec: metric(a, "sec"),
    pct: {
      loc: base.loc ? Math.round((metric(a, "loc") / base.loc) * 100) : 0,
      tokens: base.tokens ? Math.round((metric(a, "tokens") / base.tokens) * 100) : 0,
      cost: base.cost ? Math.round((metric(a, "cost") / base.cost) * 100) : 0,
      sec: base.sec ? Math.round((metric(a, "sec") / base.sec) * 100) : 0,
    },
  })),
};
writeFileSync(join(HERE, "results.json"), JSON.stringify(results, null, 2));
console.log("\nwrote results.json\n", JSON.stringify(results.arms, null, 2));
