// Hot-swap soak test: cycle a pile of patterns through a running luxel via
// POST /api/code — the same path the firmware takes — and watch liveness, fps,
// and (host mode) server memory. Rehearses the M2 exit criterion
// ("evening-long soak without a crash").
//
// Two modes:
//   host   (default) — builds and spawns `luxel serve` (the native mirror) and
//                      soaks that; also samples the server's RSS.
//   device (--device <ip|url>) — soaks a real device over the network. No RSS.
//
// Patterns come from `corpus/*.epe` when the (gitignored) corpus is present,
// otherwise from the tracked gallery (`web/public/gallery.json`, written by
// `node web/tools/gen-gallery.mjs`), so a fresh worktree can soak too.
//
// Usage (repo root, nix develop):
//   node tools/soak.mjs [rounds] [dwell-ms] [--device <ip|url>] [--limit <n>]

import { execSync, spawn } from "node:child_process";
import fs from "node:fs";
// Devices — and the mirror — take LXP1 envelopes (source + LXBC bytecode), not
// raw source. Compile locally via the built playground wasm (`npm run wasm`).
import { lxpBody } from "../web/tools/lxp.mjs";

const argv = process.argv.slice(2);
const takeFlag = (name) => {
  const i = argv.indexOf(name);
  if (i < 0) return undefined;
  const v = argv[i + 1];
  argv.splice(i, 2);
  return v;
};
const DEVICE = takeFlag("--device");
const LIMIT = Number(takeFlag("--limit") ?? Infinity);
const ROUNDS = Number(argv[0] ?? 2);
const DWELL = Number(argv[1] ?? 250);
const PORT = 8722;
const base = DEVICE
  ? DEVICE.startsWith("http")
    ? DEVICE.replace(/\/$/, "")
    : `http://${DEVICE}`
  : `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// picoserve closes idle connections after 1s and node's fetch pools them —
// "connection: close" avoids that reuse race on the device path.
async function api(pathname, body) {
  for (let attempt = 0; ; attempt++) {
    try {
      const opts = { headers: { connection: "close" }, signal: AbortSignal.timeout(30_000) };
      const r = await fetch(
        base + pathname,
        body === undefined ? opts : { ...opts, method: "POST", body },
      );
      return await r.json();
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(2_000);
    }
  }
}

// --- patterns -------------------------------------------------------------

function loadPatterns() {
  if (fs.existsSync("corpus")) {
    const files = fs.readdirSync("corpus").filter((f) => f.endsWith(".epe"));
    if (files.length) {
      const out = [];
      for (const f of files) {
        const epe = JSON.parse(fs.readFileSync(`corpus/${f}`, "utf8"));
        if (typeof epe.sources?.main === "string") out.push({ name: epe.name ?? f, source: epe.sources.main });
      }
      return { origin: "corpus/", list: out };
    }
  }
  const gallery = JSON.parse(fs.readFileSync("web/public/gallery.json", "utf8"));
  return { origin: "web/public/gallery.json", list: gallery.map((p) => ({ name: p.name, source: p.source })) };
}

const { origin, list } = loadPatterns();
const patterns = Number.isFinite(LIMIT) ? list.slice(0, LIMIT) : list;

// Compile once up front — the envelope is the same on every round, and a
// local compile failure is a different animal from a device rejection.
const compiled = [];
const compileFailed = [];
for (const p of patterns) {
  try {
    compiled.push({ name: p.name, body: await lxpBody("", p.source) });
  } catch (e) {
    compileFailed.push(`${p.name}: ${e.message}`);
  }
}
if (!compiled.length) {
  console.log(`FAIL: nothing compiled from ${origin} (${patterns.length} candidates) — cannot soak.`);
  if (compileFailed.length) console.log("  " + compileFailed.slice(0, 5).join("\n  "));
  process.exit(1);
}

// --- target ---------------------------------------------------------------

let server = null;
let died = false;
if (!DEVICE) {
  execSync("cargo build -q -p luxel-cli", { stdio: "inherit" });
  server = spawn("target/debug/luxel", ["serve", "--port", String(PORT), "--pixels", "300"], {
    stdio: ["ignore", "pipe", "inherit"],
  });
  server.on("exit", () => { died = true; });
  process.on("exit", () => server.kill());
  await new Promise((resolve, reject) => {
    server.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
    setTimeout(() => reject(new Error("server start timeout")), 30000);
  });
}

const rssKb = () =>
  Number(/VmRSS:\s+(\d+)/.exec(fs.readFileSync(`/proc/${server.pid}/status`, "utf8"))[1]);
const rss = () => (server ? `, RSS ${(rssKb() / 1024).toFixed(1)} MB` : "");

console.log(
  `soak: ${compiled.length} patterns from ${origin} × ${ROUNDS} rounds, ${DWELL}ms dwell` +
    ` → ${base}${compileFailed.length ? ` (${compileFailed.length} failed to compile locally)` : ""}${rss()}`,
);

// --- soak -----------------------------------------------------------------

let uploads = 0, accepted = 0, rejected = 0, vmerrs = 0;
const rejectedNames = new Set(), vmerrNames = new Set();

for (let round = 0; round < ROUNDS; round++) {
  for (const p of compiled) {
    if (died) break;
    uploads++;
    let r;
    try {
      r = await api("/api/code", p.body);
    } catch (e) {
      rejected++;
      rejectedNames.add(`${p.name}: push failed: ${e.message}`);
      continue;
    }
    if (r.ok) accepted++;
    else { rejected++; rejectedNames.add(`${p.name}: ${r.error ?? JSON.stringify(r)}`); continue; }
    await sleep(DWELL);
    const st = await api("/api/status");
    if (st.vmerr) { vmerrs++; vmerrNames.add(`${p.name}: ${st.vmerr}`); }
    if (st.fps === 0 && uploads > 5) console.log(`  WARN fps=0 after ${p.name}`);
  }
  console.log(`round ${round + 1}/${ROUNDS} done${rss()}`);
}

if (died) {
  console.log("FAIL: server process died during soak");
  process.exit(1);
}
const st = await api("/api/status");
console.log(
  `\nsoak done: ${uploads} uploads, ${accepted} accepted, ${rejected} rejected,` +
    ` ${compileFailed.length} local compile failures, ${vmerrs} runtime errors;` +
    ` final fps ${st.fps}${rss()}`,
);
if (compileFailed.length) console.log("local compile failures:\n  " + compileFailed.join("\n  "));
if (rejectedNames.size) console.log("rejected:\n  " + [...rejectedNames].join("\n  "));
if (vmerrNames.size) console.log("runtime errors:\n  " + [...vmerrNames].join("\n  "));
server?.kill();

// A run where (nearly) every upload bounced is not a green soak — it tests the
// envelope validator and nothing else. #218: soak.mjs used to POST raw source,
// so 100% of uploads were rejected and it still exited 0.
if (uploads > 0 && accepted / uploads < 0.5) {
  console.log(
    `\nFAIL: only ${accepted}/${uploads} uploads were accepted by ${base}` +
      ` — the soak exercised the upload validator, not the engine.`,
  );
  process.exit(1);
}
process.exit(0);
