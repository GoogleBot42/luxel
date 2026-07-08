// Hardware soak + benchmark: run every gallery pattern on a real device,
// sampling fps and vmerr, then measure fps vs pixel count with a reference
// pattern. Writes a markdown report. (tools/soak.mjs is the host-side
// mirror soak; this one exercises the actual firmware + strip.)
//
// Usage (repo root, nix develop): node tools/hw-bench.mjs <device-ip> [report.md]
//
// Restores: rainbow, 300 px, the brightness it found. ~15 min for ~190
// patterns, one after another on the actual strip.

import fs from "node:fs";
// devices take LXP1 envelopes (source + LXBC bytecode), not raw source —
// compile locally via the built playground wasm (needs `npm run wasm` once)
import { lxpBody } from "../web/tools/lxp.mjs";

const IP = process.argv[2] ?? "192.168.0.205";
const OUT = process.argv[3] ?? "docs/bench-report.md";
const DEV = `http://${IP}`;
const DWELL_MS = 2500; // per pattern: settle + let the fps counter refill
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function api(path, body) {
  // node's fetch pools connections; picoserve closes idle ones after 1s —
  // "connection: close" avoids that reuse race. A request abandoned by a
  // short client timeout PINS one of the device's three sockets until its
  // server-side read timeout reaps it, and retry storms then cascade — so:
  // a generous timeout, few retries, and a real pause before each one.
  for (let attempt = 0; ; attempt++) {
    try {
      const opts = {
        headers: { connection: "close" },
        signal: AbortSignal.timeout(30_000),
      };
      const r = await fetch(
        DEV + path,
        body === undefined ? opts : { ...opts, method: "POST", body },
      );
      return await r.json();
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(8_000); // let the device reap any socket we just pinned
    }
  }
}

const gallery = JSON.parse(fs.readFileSync("web/public/gallery.json", "utf8"));
const rainbow = fs.readFileSync("library/rainbow.js", "utf8");
const status0 = await api("/api/status");
const bright0 = (await api("/api/brightness")).brightness;
console.log(`device ${IP}: v${status0.version}, ${status0.pixels}px, brightness ${bright0} — soaking ${gallery.length} patterns`);

const rows = [];
let i = 0;
for (const p of gallery) {
  i++;
  let res;
  try {
    res = await api("/api/code", await lxpBody("", p.source));
  } catch (e) {
    rows.push({ name: p.name, kind: p.kind, fail: `push failed: ${e}` });
    continue;
  }
  if (!res.ok) {
    rows.push({ name: p.name, kind: p.kind, fail: `rejected: ${res.error}` });
    continue;
  }
  await sleep(DWELL_MS);
  const st = await api("/api/status");
  rows.push({ name: p.name, kind: p.kind, fps: st.fps, heap: st.heap_free, vmerr: st.vmerr });
  const flag = st.vmerr ? `VMERR ${st.vmerr}` : st.fps < 30 ? "SLOW" : "";
  console.log(`${String(i).padStart(3)}/${gallery.length} ${String(st.fps).padStart(3)} fps  ${p.name} ${flag}`);
}

// fps vs pixel count with the reference pattern
console.log("-- pixel-count curve --");
const rainbowBody = await lxpBody("", rainbow);
await api("/api/code", rainbowBody);
const curve = [];
for (const n of [60, 150, 300, 600, 1024, 2048]) {
  await api("/api/config", String(n));
  await sleep(3500);
  const st = await api("/api/status");
  curve.push({ pixels: n, fps: st.fps });
  console.log(`${n} px → ${st.fps} fps`);
}

// restore
await api("/api/config", "300");
await api("/api/brightness", String(bright0));
await api("/api/code", rainbowBody);

// report
const errs = rows.filter((r) => r.fail || r.vmerr);
const slow = rows.filter((r) => !r.fail && !r.vmerr && r.fps < 30).sort((a, b) => a.fps - b.fps);
const ok = rows.filter((r) => !r.fail && !r.vmerr);
const fpss = ok.map((r) => r.fps).sort((a, b) => a - b);
const pct = (q) => fpss[Math.min(fpss.length - 1, Math.floor(q * fpss.length))] ?? 0;
const minHeap = Math.min(...ok.map((r) => r.heap));
const lines = [];
lines.push(`# Hardware soak + benchmark — ${new Date().toISOString().slice(0, 10)}`);
lines.push("");
lines.push(`*Device ${IP}, firmware v${status0.version}, ${status0.pixels} px SK9822, brightness ${bright0}.*`);
lines.push(`*Regenerate: \`node tools/hw-bench.mjs <ip>\` (≈15 min; runs every gallery pattern on the strip).*`);
lines.push("");
lines.push(`## Summary`);
lines.push("");
lines.push(`- ${gallery.length} patterns: **${ok.length} clean**, ${errs.length} with errors, ${slow.length} under 30 fps.`);
lines.push(`- fps at 300 px: median **${pct(0.5)}**, p10 ${pct(0.1)}, p90 ${pct(0.9)}.`);
lines.push(`- lowest heap_free seen while soaking: ${minHeap} bytes.`);
lines.push("");
lines.push(`## fps vs pixel count (rainbow reference)`);
lines.push("");
lines.push(`| pixels | fps |`);
lines.push(`|---:|---:|`);
for (const c of curve) lines.push(`| ${c.pixels} | ${c.fps} |`);
lines.push("");
if (errs.length) {
  lines.push(`## Errors`);
  lines.push("");
  lines.push(`| pattern | kind | problem |`);
  lines.push(`|---|---|---|`);
  for (const r of errs) lines.push(`| ${r.name} | ${r.kind} | ${r.fail ?? r.vmerr} |`);
  lines.push("");
}
if (slow.length) {
  lines.push(`## Slowest (< 30 fps at 300 px)`);
  lines.push("");
  lines.push(`| pattern | kind | fps |`);
  lines.push(`|---|---|---:|`);
  for (const r of slow) lines.push(`| ${r.name} | ${r.kind} | ${r.fps} |`);
  lines.push("");
}
lines.push(`## All results`);
lines.push("");
lines.push(`| pattern | kind | fps |`);
lines.push(`|---|---|---:|`);
for (const r of rows) lines.push(`| ${r.name} | ${r.kind} | ${r.fail ? "—" : r.fps} |`);
fs.writeFileSync(OUT, lines.join("\n") + "\n");
console.log(`wrote ${OUT}: ${ok.length} ok, ${errs.length} errors, ${slow.length} slow`);
