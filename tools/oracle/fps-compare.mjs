#!/usr/bin/env node
// Engine-vs-engine frame-rate comparison: the same pattern sources rendered by
// the real Pixelblaze and by a Luxel device, at the same pixel count, and the
// per-pixel cost each one implies. (Gitea #312/#313.)
//
//   nix develop -c node tools/oracle/fps-compare.mjs \
//     --pb 192.168.0.140 --luxel 192.168.0.183 --pixels 420 --1d \
//     library/perlin-fire-wind-tunnel.js library/rainbow.js
//
//   --pb IP        oracle Pixelblaze (live-code only, nothing saved)
//   --luxel IP     Luxel device (pixel count is set and restored)
//   --pixels N     pixel count for BOTH devices (default: the PB's found count)
//   --settle S     seconds to let a freshly loaded pattern reach steady state (6)
//   --sample S     seconds of fps samples to take after that (25)
//   --1d           force both engines onto `render(index)` — see below
//   --empty        prepend a `render(){}` case (the pure per-pixel floor)
//   --md FILE / --json FILE   write the table / raw samples
//
// WHY --1d: the oracle has a 2D pixel map installed and a Pixelblaze map is a
// one-way door, so the PB dispatches `render2D` for any pattern that exports
// one, while a 1D Luxel strip dispatches `render`. That is a different entry
// point with different arguments, which is not an engine comparison. `--1d`
// rewrites BOTH copies identically -- `render2D`/`render3D`/`render` are
// renamed to plain (unexported) functions and a single
// `render(index) { shade2D(index, index / pixelCount, 0.3) }` is appended --
// so each engine executes the same per-pixel body on the same arguments.
//
// WHAT IT DOES TO THE DEVICES: the PB is live-coded only (type-3 bytecode over
// the websocket, nothing written to its flash) and its active pattern is
// restored in a `finally`, with a clean websocket close -- see
// .claude/rules/oracle.md, both are load-bearing. The Luxel device has its
// pixel count changed and patterns POSTed to /api/code (RAM, not the store);
// the found pixel count is restored and `library/rainbow.js` reloaded at the
// end. Neither device is reflashed.
//
// READING THE OUTPUT: the PB reports one number, `fps`, and it is a black box
// -- we cannot see whether its LED output overlaps its rendering, so its
// µs/px is an upper bound on engine time. Luxel reports both: `vm_us` is
// engine-only, and its fps additionally carries the output stage. On a WS2812
// strip the wire alone costs ~30 µs/px (12.6 ms at 420 px), and Luxel renders
// on core 1 while the wire runs, so its fps saturates at the wire rate while
// vm_us keeps falling. Compare fps-to-fps for "what the strip does" and treat
// the vm µs/px column as the engine number.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";
import { lxpBody } from "../../web/tools/lxp.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const argv = process.argv.slice(2);
const VALUE_FLAGS = new Set(["--pb", "--luxel", "--pixels", "--settle", "--sample", "--md", "--json"]);
const flag = (name, dflt) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const has = (name) => argv.includes(name);
const PB_IP = flag("--pb", null);
const LX_IP = flag("--luxel", null);
const PIXELS = flag("--pixels", null) === null ? null : Number(flag("--pixels"));
const SETTLE = Number(flag("--settle", 6)) * 1000;
const SAMPLE = Number(flag("--sample", 25)) * 1000;
const FORCE_1D = has("--1d");
const MD = flag("--md", null);
const JSON_OUT = flag("--json", null);
const files = argv.filter((a, i) => !a.startsWith("--") && !VALUE_FLAGS.has(argv[i - 1]));
if (!PB_IP && !LX_IP) {
  console.error("usage: fps-compare.mjs [--pb IP] [--luxel IP] [--pixels N] [--1d] pattern.js …");
  process.exit(2);
}

/** Rewrite a pattern so BOTH engines dispatch the same `render(index)`. */
export function force1D(src) {
  let s = src
    .replace(/export function render3D\s*\(/g, "function shade3D(")
    .replace(/export function render2D\s*\(/g, "function shade2D(")
    .replace(/export function render\s*\(/g, "function shadeOld1D(");
  s = s.replace(/\brender3D\s*\(/g, "shade3D(").replace(/\brender2D\s*\(/g, "shade2D(");
  if (!/function shade2D\s*\(/.test(s)) return src; // 1D-only already
  return `${s}\n// --1d harness entry: identical per-pixel work on any device.\nexport function render(index) {\n  shade2D(index, index / pixelCount, 0.3)\n}\n`;
}

const cases = [];
if (has("--empty")) cases.push({ name: "empty render", src: "export function render(index) {}\n" });
for (const f of files) {
  const raw = fs.readFileSync(f, "utf8");
  cases.push({ name: path.basename(f, ".js") + (FORCE_1D ? " [1d]" : ""), src: FORCE_1D ? force1D(raw) : raw });
}
if (!cases.length) {
  console.error("no pattern files given");
  process.exit(2);
}

const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  return s.length % 2 ? s[s.length >> 1] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const results = new Map(cases.map((c) => [c.name, { name: c.name }]));

// ---------------------------------------------------------------- Pixelblaze
async function runPB() {
  const webUI = await (await fetch(`http://${PB_IP}/`)).text();
  const compile = buildCompiler(webUI);
  const pb = await PB.connect(PB_IP);
  const { settings, seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;
  const found = { px: settings.pixelCount, active: seq.activeProgram?.name, ver: settings.ver };
  console.log(`PB found: ${settings.name} fw ${settings.ver}, ${settings.pixelCount} px, active="${found.active}"`);
  const n = PIXELS ?? settings.pixelCount;
  if (n !== settings.pixelCount) throw new Error(`--pixels ${n} != PB's ${settings.pixelCount}; refusing to reconfigure the oracle`);
  try {
    for (const c of cases) {
      const r = results.get(c.name);
      const compiled = compile(c.src);
      if (!compiled.ok) {
        r.pbNote = `REJECTED: ${compiled.error}`;
        console.log(`PB ${c.name}: ${r.pbNote}`);
        continue;
      }
      await pb.setCode(packBytecode(compiled));
      await sleep(SETTLE);
      const samples = [];
      let seen = null;
      let vmerr = null;
      for (const t0 = Date.now(); Date.now() - t0 < SAMPLE; ) {
        await sleep(200);
        if (pb.lastStats && pb.lastStats !== seen) {
          seen = pb.lastStats;
          samples.push(seen.fps);
          if (seen.vmerr) vmerr = seen.vmerr;
        }
      }
      const med = median(samples);
      Object.assign(r, { pbFps: med, pbSamples: samples, pbVmErr: vmerr, pbUsPx: 1e6 / (med * n) });
      console.log(`PB ${c.name}: fps ${med.toFixed(2)} (n=${samples.length}, ${Math.min(...samples).toFixed(2)}–${Math.max(...samples).toFixed(2)}) → ${(1e6 / (med * n)).toFixed(1)} µs/px${vmerr ? ` vmerr=${vmerr}` : ""}`);
    }
  } finally {
    if (restoreId) await pb.setActivePattern(restoreId).catch(() => {});
    // the switch pushes its own activeProgram frame; let it land and drop any
    // stale queued frame (the live-coded one has an empty name) before verifying
    await sleep(1500);
    pb.queue.length = 0;
    const { seq: after } = await pb.getConfig().catch(() => ({ seq: {} }));
    console.log(`PB restored active pattern: "${after.activeProgram?.name}"`);
    await pb.close();
  }
  return found;
}

// --------------------------------------------------------------------- Luxel
async function runLuxel() {
  const DEV = `http://${LX_IP}`;
  const api = async (p, body) => {
    for (let attempt = 0; ; attempt++) {
      try {
        const opts = { headers: { connection: "close" }, signal: AbortSignal.timeout(30_000) };
        const res = await fetch(DEV + p, body === undefined ? opts : { ...opts, method: "POST", body });
        return await res.json();
      } catch (e) {
        if (attempt >= 2) throw e;
        await sleep(8000);
      }
    }
  };
  const st0 = await api("/api/status");
  const found = { px: st0.pixels, version: st0.version, slot: st0.slot };
  console.log(`Luxel found: v${st0.version} ${st0.slot}, ${st0.pixels} px, fps ${st0.fps}`);
  const n = PIXELS ?? st0.pixels;
  try {
    if (n !== st0.pixels) {
      await api("/api/config", String(n));
      await sleep(2000);
      const st = await api("/api/status");
      if (st.pixels !== n) throw new Error(`pixel_count did not take: ${st.pixels}`);
      console.log(`Luxel pixel_count → ${st.pixels}`);
    }
    for (const c of cases) {
      const r = results.get(c.name);
      await api("/api/code", await lxpBody("", c.src, n));
      await sleep(SETTLE);
      const fps = [];
      const vm = [];
      const frame = [];
      const out = [];
      let vmerr = null;
      for (const t0 = Date.now(); Date.now() - t0 < SAMPLE; ) {
        await sleep(1000);
        const st = await api("/api/status");
        fps.push(st.fps);
        vm.push(st.vm_us);
        frame.push(st.frame_us);
        out.push(st.out_us);
        if (st.vmerr) vmerr = st.vmerr;
      }
      const mf = median(fps);
      const mv = median(vm);
      Object.assign(r, {
        lxFps: mf, lxVmUs: mv, lxFrameUs: median(frame), lxOutUs: median(out),
        lxSamples: fps, lxVmErr: vmerr,
        lxVmUsPx: mv / n, lxFpsUsPx: 1e6 / (mf * n),
      });
      console.log(`LX ${c.name}: fps ${mf.toFixed(2)} vm ${mv}µs frame ${median(frame)}µs out ${median(out)}µs → vm ${(mv / n).toFixed(2)} µs/px, fps ${(1e6 / (mf * n)).toFixed(1)} µs/px${vmerr ? ` vmerr=${vmerr}` : ""}`);
    }
  } finally {
    if (n !== st0.pixels) {
      await api("/api/config", String(st0.pixels)).catch(() => {});
      await sleep(1500);
    }
    await api("/api/code", await lxpBody("", fs.readFileSync(path.join(ROOT, "library/rainbow.js"), "utf8"), st0.pixels)).catch(() => {});
    await sleep(2000);
    const st = await api("/api/status").catch(() => ({}));
    console.log(`Luxel restored: ${st.pixels} px, fps ${st.fps}, vmerr ${st.vmerr}`);
  }
  return found;
}

const foundPB = PB_IP ? await runPB() : null;
const foundLX = LX_IP ? await runLuxel() : null;

const n = PIXELS ?? foundPB?.px ?? foundLX?.px;
const rows = [...results.values()];
const cell = (v, d = 2) => (v === undefined || v === null || !isFinite(v) ? "—" : v.toFixed(d));
let md = `| pattern | PB fps | PB µs/px | Luxel fps | Luxel fps µs/px | Luxel vm µs/px | PB µs/px ÷ Luxel vm µs/px |\n|---|---|---|---|---|---|---|\n`;
for (const r of rows) {
  const ratio = r.pbUsPx && r.lxVmUsPx ? r.pbUsPx / r.lxVmUsPx : null;
  md += `| \`${r.name}\` | ${r.pbNote ? "rejected" : cell(r.pbFps)} | ${cell(r.pbUsPx, 1)} | ${cell(r.lxFps)} | ${cell(r.lxFpsUsPx, 1)} | ${cell(r.lxVmUsPx, 1)} | ${ratio ? cell(ratio) + "×" : "—"} |\n`;
}
console.log(`\n@ ${n} px\n\n${md}`);
if (MD) fs.writeFileSync(MD, `@ ${n} px\n\n${md}`);
if (JSON_OUT) fs.writeFileSync(JSON_OUT, JSON.stringify({ pixels: n, foundPB, foundLX, settleMs: SETTLE, sampleMs: SAMPLE, force1D: FORCE_1D, rows }, null, 2));
