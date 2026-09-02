// On-device soak of external event injection (POST /api/events → readEvent).
// First run 2026-08-22 on the Athom rig (v0.1.39): 30,286/30,286 delivered,
// 44/44 malformed rejected, heap/fps stable, no vmerr, no reboot.
//
// Usage (repo root, nix develop; needs a readEvent-era web/public/luxel.wasm
// — `npm run wasm` if stale):
//   node tools/event-soak.mjs <device-ip>
//
// Phases: baseline → steady injection (batches 1..32 every 200 ms, malformed
// frames every 60 s, 32-batch bursts every 3 min) → cooldown → restore
// rainbow. All verification over HTTP (/api/status + /api/vars) — works with
// no serial console. NOTE: /api/vars returns raw 16.16 — divide by 65536.

import fs from "node:fs";
import { lxpBody } from "../web/tools/lxp.mjs";

const IP = process.argv[2] ?? "192.168.0.183";
const DEV = `http://${IP}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const STEADY_MS = 12 * 60_000;
const COOLDOWN_MS = 2 * 60_000;
const MIN_VERSION = "0.1.39";

/** Compare dotted numeric versions: <0, 0, >0. Unparseable → treated as 0. */
function cmpVersion(a, b) {
  const parse = (v) => String(v ?? "").split(".").map((n) => Number(n) || 0);
  const pa = parse(a), pb = parse(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) - (pb[i] ?? 0);
  }
  return 0;
}

async function api(pathname, body, binary = false) {
  for (let attempt = 0; ; attempt++) {
    try {
      const opts = {
        headers: { connection: "close" },
        signal: AbortSignal.timeout(30_000),
      };
      const r = await fetch(
        DEV + pathname,
        body === undefined ? opts : { ...opts, method: "POST", body },
      );
      return await r.json();
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(8_000);
    }
  }
}

function evFrame(quads) {
  // "EV1\0" + u8 count + count × 4×i32-LE raws
  const buf = Buffer.alloc(5 + quads.length * 16);
  buf.write("EV1\0", 0, "latin1");
  buf[4] = quads.length;
  quads.forEach((q, i) => {
    q.forEach((v, j) => buf.writeInt32LE(Math.round(v * 65536) | 0, 5 + i * 16 + j * 4));
  });
  return buf;
}

const PATTERN = `export var evTotal = 0
export var evLastType = -1
export var evLastX = 0
export var evLastVal = 0
export var frames = 0
var ev = array(4)
export function beforeRender(delta) {
  frames = frames + 1
  while (readEvent(ev)) {
    evTotal = evTotal + 1
    evLastType = ev[0]
    evLastX = ev[1]
    evLastVal = ev[3]
  }
}
export function render(index) {
  hsv(evTotal / 60 + index / pixelCount, 1, 0.25)
}
`;

const samples = [];
let rebootSuspected = false;
let lastEvTotal = -1;

async function sample(phase) {
  const st = await api("/api/status");
  const vars = await api("/api/vars");
  const s = {
    t: Math.round(performance.now() / 1000),
    phase,
    fps: st.fps,
    heap: st.heap_free,
    vmerr: st.vmerr,
    slot: st.slot,
    version: st.version,
    // /api/vars snapshots raw 16.16 — decode counters to plain numbers.
    // frames wraps i32 by design past 32768.0 (two's-complement VM), so it
    // is NOT a reboot signal — only a monotonic evTotal drop is.
    evTotal: typeof vars.evTotal === "number" ? Math.round(vars.evTotal / 65536) : null,
    frames: typeof vars.frames === "number" ? Math.round(vars.frames / 65536) : null,
  };
  samples.push(s);
  if (typeof s.evTotal === "number" && s.evTotal < lastEvTotal) rebootSuspected = true;
  if (typeof s.evTotal === "number") lastEvTotal = s.evTotal;
  console.log(
    `[${s.t}s ${phase}] fps=${s.fps} heap=${s.heap} vmerr=${s.vmerr} evTotal=${s.evTotal} frames=${s.frames}`,
  );
  return s;
}

const st0 = await api("/api/status");
console.log(`device ${IP}: v${st0.version} slot=${st0.slot} heap=${st0.heap_free} fps=${st0.fps}`);
// Event injection (POST /api/events → readEvent) landed in v0.1.39; anything
// newer is fine too. An exact-version pin made this script unrunnable against
// every later build (#218).
if (cmpVersion(st0.version, MIN_VERSION) < 0) {
  throw new Error(
    `event injection needs firmware >= v${MIN_VERSION}; device reports v${st0.version ?? "?"}`,
  );
}

// push the counter pattern live (ad-hoc → never persisted, nothing on flash)
const push = await api("/api/code", await lxpBody("", PATTERN));
if (!push.ok) throw new Error(`pattern push rejected: ${JSON.stringify(push)}`);
console.log("counter pattern live");
await sleep(3000);

// --- baseline ---
for (let i = 0; i < 6; i++) {
  await sample("baseline");
  await sleep(5000);
}
const heapBase = samples.at(-1).heap;

// --- steady injection ---
let sentSteady = 0;
let sentBurst = 0;
let malformedOk = 0;
let malformedTotal = 0;
const sizes = [1, 2, 4, 8, 16, 32];
let seq = 0;
const t0 = performance.now();
let nextSample = 0;
let nextMalformed = 60_000;
let nextBurst = 180_000;

while (performance.now() - t0 < STEADY_MS) {
  const n = sizes[seq % sizes.length];
  const quads = Array.from({ length: n }, (_, k) => [
    (seq + k) % 5,
    ((seq * 7 + k) % 100) / 100,
    ((seq * 13 + k) % 100) / 100,
    (seq % 30000) / 100,
  ]);
  const r = await api("/api/events", evFrame(quads));
  if (r.ok) sentSteady += n;
  else console.log(`steady frame REJECTED: ${JSON.stringify(r)}`);
  seq++;

  const el = performance.now() - t0;
  if (el >= nextSample) {
    await sample("steady");
    nextSample += 5000;
  }
  if (el >= nextMalformed) {
    nextMalformed += 60_000;
    // bad magic, truncated, count>32, count/len mismatch — expect ok:false
    const bad = [
      Buffer.from("EV2\0\x01" + "\0".repeat(16), "latin1"),
      evFrame([[1, 0, 0, 0]]).subarray(0, 12),
      Buffer.concat([Buffer.from("EV1\0", "latin1"), Buffer.from([33]), Buffer.alloc(33 * 16)]),
      Buffer.concat([Buffer.from("EV1\0", "latin1"), Buffer.from([4]), Buffer.alloc(16)]),
    ];
    for (const b of bad) {
      malformedTotal++;
      const rr = await api("/api/events", b);
      if (rr.ok === false) malformedOk++;
      else console.log(`MALFORMED FRAME ACCEPTED?! ${JSON.stringify(rr)}`);
    }
  }
  if (el >= nextBurst) {
    nextBurst += 180_000;
    console.log("burst: 30 × 32-event frames back-to-back (overflow expected)");
    for (let b = 0; b < 30; b++) {
      const q = Array.from({ length: 32 }, (_, k) => [9, k / 32, 0.5, b]);
      const rr = await api("/api/events", evFrame(q));
      if (rr.ok) sentBurst += 32;
    }
  }
  await sleep(200);
}

// --- cooldown ---
const tc = performance.now();
while (performance.now() - tc < COOLDOWN_MS) {
  await sample("cooldown");
  await sleep(10_000);
}

// --- verdicts ---
const last = samples.at(-1);
const steadySamples = samples.filter((s) => s.phase === "steady");
const heapMin = Math.min(...steadySamples.map((s) => s.heap));
const fpsMin = Math.min(...steadySamples.map((s) => s.fps));
const totalSent = sentSteady + sentBurst;
const delivered = last.evTotal;
console.log("\n=== VERDICT ===");
console.log(`sent: ${totalSent} (steady ${sentSteady}, burst ${sentBurst})`);
console.log(`delivered (evTotal): ${delivered} — drops ${totalSent - delivered} (only bursts may drop, cap ${sentBurst})`);
console.log(`steady-phase delivery ${delivered >= sentSteady ? "OK (all steady events arrived)" : "FAIL: lost steady events"}`);
console.log(`malformed rejected: ${malformedOk}/${malformedTotal} ${malformedOk === malformedTotal ? "OK" : "FAIL"}`);
console.log(`vmerr: ${last.vmerr === null ? "none OK" : `FAIL ${last.vmerr}`}`);
console.log(`reboot suspected: ${rebootSuspected ? "FAIL" : "no OK"}`);
console.log(`heap: base ${heapBase}, min-under-load ${heapMin}, final ${last.heap} (${last.heap - heapBase >= -2000 ? "stable OK" : "FAIL: possible leak"})`);
console.log(`fps: min ${fpsMin} under load`);

// --- restore: rainbow live (the rodata default the device boots into) ---
const rainbow = fs.readFileSync("library/rainbow.js", "utf8");
const rr = await api("/api/code", await lxpBody("", rainbow));
console.log(`restore rainbow: ${JSON.stringify(rr)}`);
const stf = await api("/api/status");
console.log(`final: fps=${stf.fps} heap=${stf.heap_free} vmerr=${stf.vmerr}`);

fs.writeFileSync("event-soak-samples.json", JSON.stringify(samples, null, 1));
console.log("samples → event-soak-samples.json");
