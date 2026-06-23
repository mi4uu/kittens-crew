#!/usr/bin/env bun
// Measures caveman spec compression: the same spec, written as caveman SPEC.md
// vs as a normal prose PRD, tokenized with a real tokenizer. Deterministic and
// reproducible — `bun benchmarks/compression.ts`. No model calls, no fabrication.
import { encode } from "gpt-tokenizer/encoding/o200k_base"; // GPT-4o family; proxy for Claude
import { readFileSync } from "node:fs";
import { join } from "node:path";

const here = new URL(".", import.meta.url).pathname;
const read = (f: string) => readFileSync(join(here, "fixtures", f), "utf8");

type Stat = { name: string; tokens: number; chars: number; lines: number };
const stat = (name: string, text: string): Stat => ({
  name,
  tokens: encode(text).length,
  chars: text.length,
  lines: text.split("\n").length,
});

const prose = stat("prose PRD", read("spec.prose.md"));
const caveman = stat("caveman SPEC.md", read("spec.caveman.md"));
const saved = (a: number, b: number) => `${(((a - b) / a) * 100).toFixed(0)}%`;

console.log("\ncaveman spec compression  (tokenizer: o200k_base)\n");
console.log("  encoding          tokens   chars   lines");
for (const s of [prose, caveman])
  console.log(
    `  ${s.name.padEnd(16)}  ${String(s.tokens).padStart(5)}   ${String(s.chars).padStart(5)}   ${String(s.lines).padStart(5)}`,
  );
console.log(
  `\n  tokens saved: ${saved(prose.tokens, caveman.tokens)}  (${prose.tokens} → ${caveman.tokens}, same spec)\n`,
);

// The spec is loaded on every command invocation, so the saving recurs each call.
const perCall = prose.tokens - caveman.tokens;
console.log(`  reloaded every command → ~${perCall} tokens saved per invocation.\n`);
