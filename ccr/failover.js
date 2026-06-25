// Tiny failover shim in front of claude-code-router.
//
// CCR translates Anthropic <-> OpenAI and routes a model to its provider, but it
// has NO post-error failover. Free OpenRouter models are intermittently 429'd or
// slow (different upstream providers — when one refuses on free-tier limit,
// another serves). This shim retries the SAME request through CCR against an
// ordered list of models until one returns a usable response.
//
// Flow:  Claude Code --Anthropic /v1/messages--> failover :3460
//        for model in MODELS: POST to CCR with body.model = model
//          ok (2xx + non-error body) -> return verbatim
//          429 / 5xx / timeout / error body -> try next
//
// No dependencies (node http only). Config via env.

const http = require("http");

const UPSTREAM = process.env.UPSTREAM || "http://ccr:3456";
const PORT = parseInt(process.env.PORT || "3460", 10);
const PER_MODEL_TIMEOUT = parseInt(process.env.PER_MODEL_TIMEOUT_MS || "75000", 10);
// Ordered failover chain: fast/reliable first, big/slow last. Edit freely.
const MODELS = (process.env.MODELS ||
  "google/gemma-4-31b-it:free," +
  "nvidia/nemotron-3-nano-30b-a3b:free," +
  "nousresearch/hermes-3-llama-3.1-405b:free," +
  "poolside/laguna-m.1:free," +
  "nvidia/nemotron-3-ultra-550b-a55b:free"
).split(",").map(s => s.trim()).filter(Boolean);

const up = new URL(UPSTREAM);

function postToCcr(pathname, headers, bodyBuf) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      { hostname: up.hostname, port: up.port, path: pathname, method: "POST", headers },
      res => {
        const chunks = [];
        res.on("data", c => chunks.push(c));
        res.on("end", () => resolve({ status: res.statusCode, body: Buffer.concat(chunks) }));
      }
    );
    req.setTimeout(PER_MODEL_TIMEOUT, () => req.destroy(new Error("per-model timeout")));
    req.on("error", reject);
    req.end(bodyBuf);
  });
}

// A response is "usable" if 2xx and the JSON body is not an Anthropic error.
function usable(status, body) {
  if (status < 200 || status >= 300) return false;
  try {
    const j = JSON.parse(body.toString());
    if (j && j.type === "error") return false;
    if (j && j.error) return false;
  } catch { return false; } // non-JSON / empty -> not usable
  return true;
}

const server = http.createServer((cin, cout) => {
  const chunks = [];
  cin.on("data", c => chunks.push(c));
  cin.on("end", async () => {
    const raw = Buffer.concat(chunks);
    // Only the messages endpoint gets failover; everything else is a single pass.
    const isMessages = cin.url.startsWith("/v1/messages") && cin.method === "POST";
    const hdr = { ...cin.headers, host: up.host };
    delete hdr["content-length"];

    if (!isMessages) {
      try {
        const r = await postToCcr(cin.url, hdr, raw);
        cout.writeHead(r.status, { "content-type": "application/json" });
        return cout.end(r.body);
      } catch (e) {
        cout.writeHead(502); return cout.end(JSON.stringify({ type: "error", error: { message: String(e) } }));
      }
    }

    let payload;
    try { payload = JSON.parse(raw.toString()); }
    catch { cout.writeHead(400); return cout.end(JSON.stringify({ type: "error", error: { message: "bad json" } })); }

    const tried = [];
    for (const model of MODELS) {
      const body = Buffer.from(JSON.stringify({ ...payload, model }));
      const h = { ...hdr, "content-type": "application/json", "content-length": Buffer.byteLength(body) };
      try {
        const r = await postToCcr("/v1/messages", h, body);
        if (usable(r.status, r.body)) {
          cout.writeHead(r.status, { "content-type": "application/json", "x-failover-model": model });
          return cout.end(r.body);
        }
        tried.push(`${model}:${r.status}`);
      } catch (e) {
        tried.push(`${model}:${e.message || "err"}`);
      }
    }
    cout.writeHead(503, { "content-type": "application/json" });
    cout.end(JSON.stringify({ type: "error", error: { message: "all models failed", tried } }));
  });
});

server.listen(PORT, "0.0.0.0", () => console.log(`failover listening :${PORT} -> ${UPSTREAM} | models: ${MODELS.join(", ")}`));
