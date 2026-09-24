// Throughput of the playground's engine, `web/public/luxel.wasm` — the number
// that decides how aggressively that wasm may be size-optimised (Gitea #683).
//
// The Patterns page renders ~40 animated tiles through this module, so a
// smaller-but-slower build is not automatically a win; `npm run wasm` picks a
// profile, this says what it cost. Usage (from web/):
//
//   node tools/wasm-bench.mjs [--wasm PATH] [--px N] [--frames N]
//                             [--patterns a.js,b.js] [--label NAME] [--json OUT]
//
// Prints one row per pattern (ms for the whole run, µs/frame) plus a TOTAL
// line. Compare two builds by pointing `--wasm` at each and diffing TOTAL;
// treat anything inside ±5 % as noise (the same run-to-run spread the native
// benches have — .claude/rules/vm-bytecode.md).

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(webDir);

const argv = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] !== undefined ? argv[i + 1] : fallback;
};

const WASM = path.resolve(arg("--wasm", path.join(webDir, "public/luxel.wasm")));
const PX = Number(arg("--px", 1024));
const FRAMES = Number(arg("--frames", 300));
const LABEL = arg("--label", path.basename(WASM));
const JSON_OUT = arg("--json", null);
// Five library patterns spanning what the tiles actually run: two plain 1D
// `render` patterns, a 2D `render2D` one, a noise-heavy float workload, and a
// dimensionless `renderFrame` one. Named, not sampled, so two runs of
// different builds are comparable; `--patterns` overrides the set.
const DEFAULT_PATTERNS = [
  "rainbow.js",
  "snake-2d.js",
  "perlin-fire-wind-tunnel.js",
  "sparks.js",
  "fairies.js",
];
const patterns = arg("--patterns", "")
  ? arg("--patterns", "").split(",").map((s) => s.trim()).filter(Boolean)
  : DEFAULT_PATTERNS;

const { instance } = await WebAssembly.instantiate(fs.readFileSync(WASM), {});
const e = instance.exports;

function compile(source) {
  const bytes = new TextEncoder().encode(source);
  const ptr = e.lx_alloc(bytes.length);
  new Uint8Array(e.memory.buffer).set(bytes, ptr);
  const h = e.lx_new(ptr, bytes.length, PX, 1);
  e.lx_dealloc(ptr, bytes.length);
  if (h < 0) {
    const msg = new TextDecoder().decode(
      new Uint8Array(e.memory.buffer, e.lx_response_ptr(), e.lx_response_len()),
    );
    throw new Error(`compile failed: ${msg}`);
  }
  return h;
}

// 16.16 fixed point, ~60 fps — the delta the playground feeds its tiles.
const DELTA = Math.round(16.667 * 65536);

const rows = [];
let total = 0;
for (const file of patterns) {
  const p = path.join(repo, "library", file);
  if (!fs.existsSync(p)) {
    console.error(`wasm-bench: ${file} not in library/ — skipping`);
    continue;
  }
  const h = compile(fs.readFileSync(p, "utf8"));
  // warm the engine (first frames allocate) before the timed run
  for (let i = 0; i < 30; i++) e.lx_frame(h, DELTA);
  const t0 = performance.now();
  for (let i = 0; i < FRAMES; i++) {
    const ptr = e.lx_frame(h, DELTA);
    if (ptr === 0) throw new Error(`${file}: lx_frame returned null`);
  }
  const ms = performance.now() - t0;
  // A pattern that traps mid-run stops doing work and would read as a
  // speed-up; `lx_frame` still returns its (stale) buffer, so the null check
  // above does not catch it.
  if (e.lx_take_error(h)) {
    const msg = new TextDecoder().decode(
      new Uint8Array(e.memory.buffer, e.lx_response_ptr(), e.lx_response_len()),
    );
    throw new Error(`${file}: runtime error during the timed run — ${msg}`);
  }
  e.lx_free(h);
  total += ms;
  rows.push({ pattern: file, ms, us_per_frame: (ms * 1000) / FRAMES });
}

console.log(`# ${LABEL} — ${PX} px, ${FRAMES} frames, wasm ${fs.statSync(WASM).size} B\n`);
console.log("| pattern | ms | µs/frame |");
console.log("|---|---:|---:|");
for (const r of rows) {
  console.log(`| ${r.pattern} | ${r.ms.toFixed(1)} | ${r.us_per_frame.toFixed(1)} |`);
}
console.log(`| **TOTAL** | **${total.toFixed(1)}** | |`);

if (JSON_OUT) {
  fs.writeFileSync(
    JSON_OUT,
    JSON.stringify({ label: LABEL, wasm: WASM, bytes: fs.statSync(WASM).size, px: PX, frames: FRAMES, rows, total_ms: total }, null, 2),
  );
}
