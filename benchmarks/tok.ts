import { encode } from "gpt-tokenizer/encoding/o200k_base";
const t = await new Response(Bun.stdin.stream()).text();
process.stdout.write(String(encode(t).length));
