// Sweep probes: sample PB builtins over input grids by uploading patterns
// that fill exported arrays, then dump exact raws for offline algorithm
// fitting. Also captures the prng state chain via the prngSeed-return trick.
//
// Usage: node tools/oracle/sweep.mjs <ip> [sweep-name ...]
// Output: tools/oracle/sweeps/<name>.json

import fs from "node:fs";
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const only = process.argv.slice(3);
const OUT = "tools/oracle/sweeps";

// Each sweep: N inputs (raw 16.16) → pattern computes f into an exported
// array. 200 elements per batch keeps well under limits.
const BATCH = 200;

function gridRaw(from, to, n) {
  // inclusive endpoints, exact raws
  const a = Math.round(from * 65536);
  const b = Math.round(to * 65536);
  return Array.from({ length: n }, (_, i) => Math.round(a + ((b - a) * i) / (n - 1)));
}

export const SWEEPS = {
  // fn of one variable: expr uses `x`
  sin: { expr: "sin(x)", inputs: [...gridRaw(0, 6.5, 400), ...gridRaw(1.5697, 1.5719, 120), ...gridRaw(3.1405, 3.1427, 120), ...gridRaw(-7, 0, 150), ...gridRaw(90, 110, 100)] },
  atan: { expr: "atan(x)", inputs: [...gridRaw(-1, 1, 300), ...gridRaw(1, 30, 150), ...gridRaw(-30, -1, 80), ...gridRaw(30, 3000, 80)] },
  asin: { expr: "asin(x)", inputs: gridRaw(-1, 1, 300) },
  acos: { expr: "acos(x)", inputs: gridRaw(-1, 1, 200) },
  exp: { expr: "exp(x)", inputs: [...gridRaw(-8, 0, 150), ...gridRaw(0, 10, 300)] },
  log: { expr: "log(x)", inputs: [...gridRaw(0.001, 1, 200), ...gridRaw(1, 100, 200), ...gridRaw(100, 30000, 100)] },
  log2: { expr: "log2(x)", inputs: [...gridRaw(0.001, 1, 100), ...gridRaw(1, 256, 200)] },
  sqrt: { expr: "sqrt(x)", inputs: [...gridRaw(0, 4, 300), ...gridRaw(4, 1000, 200), ...gridRaw(1000, 32000, 150)] },
  pow2x: { expr: "pow(2, x)", inputs: gridRaw(-8, 12, 200) },
  powx2: { expr: "pow(x, 2.5)", inputs: gridRaw(0.01, 40, 200) },
  tan: { expr: "tan(x)", inputs: gridRaw(-1.5, 1.5, 200) },
  wave: { expr: "wave(x)", inputs: gridRaw(-1, 2, 300) },
  // --- perlin family (2026-08-22): capture for offline algorithm fitting.
  // Arities verified against PB's compiler: perlin(x,y,z,seed),
  // perlinFbm(6), perlinRidge(7), perlinTurbulence(6), setPerlinWrap(3) —
  // same as Luxel. Tail-arg MEANINGS on PB are unknown; the argN sweeps
  // isolate each one's effect at a fixed sample point.
  perlin1d: { expr: "perlin(x, 0.3, 0.7, 5)", inputs: [...gridRaw(-2, 2, 400), ...gridRaw(2, 10, 200)] },
  perlin1d_fine: { expr: "perlin(x, 0.3, 0.7, 5)", inputs: gridRaw(0, 1, 400) },
  perlin_seed: { expr: "perlin(0.37, 0.3, 0.7, x)", inputs: [...gridRaw(0, 20, 200), ...gridRaw(0, 1, 100)] },
  perlin_wrap4: {
    expr: "perlin(x, 0.3, 0.7, 5)",
    setup: "setPerlinWrap(4, 4, 4)",
    inputs: gridRaw(0, 9, 400),
  },
  fbm1d: { expr: "perlinFbm(x, 0.3, 0.7, 2, 0.5, 3)", inputs: gridRaw(-2, 4, 400) },
  fbm_arg4: { expr: "perlinFbm(0.37, 0.3, 0.7, x, 0.5, 3)", inputs: gridRaw(0.25, 6, 150) },
  fbm_arg5: { expr: "perlinFbm(0.37, 0.3, 0.7, 2, x, 3)", inputs: gridRaw(0, 2, 150) },
  fbm_arg6: { expr: "perlinFbm(0.37, 0.3, 0.7, 2, 0.5, x)", inputs: gridRaw(1, 8, 120) },
  ridge1d: { expr: "perlinRidge(x, 0.3, 0.7, 2, 0.5, 1, 3)", inputs: gridRaw(-2, 4, 400) },
  turb1d: { expr: "perlinTurbulence(x, 0.3, 0.7, 2, 0.5, 3)", inputs: gridRaw(-2, 4, 400) },
};

function sweepSource(exprTemplate, inputRaws) {
  // xs filled via arrayReplace-free assignment loop over literals is huge;
  // instead embed inputs as raw integers and decode: raw / 65536 loses
  // nothing? division by 65536 in-fixed truncates fraction — NO. Use
  // (raw >> 16) + low bits via composition: x = hi + lo * (1/65536) also
  // lossy. Cleanest: two arrays hi/lo and x = (hi << 16 | ...) — simpler:
  // pass value as two ints: whole raw = a * 256 + b (a,b < 32768 safe) and
  // x = (a << 8) + (b >> 8)? Fixed-point shifts move value: raw r target:
  // x_raw = r. Build via: x = a << 8 | via arithmetic: a<<8 has raw a*2^8*65536?
  // Shifts act on raw directly: (a << 8) raw = a_raw << 8 = a*65536*256.
  // We want plain raw r: x = r_hi * 256 + r_lo where value math scales by
  // 65536… Avoid cleverness: use e$ = 0.0000152587890625 (raw 1, kept
  // exactly as 16.15? NO — literal LSB cleared → raw 0!). Use (1 >> 16):
  // computed at runtime: one = 1; eps = one >> 16  → raw 1. x = int(r) * eps
  // where int(r) up to ±2^31 overflows literals… decompose r = h*65536 + l:
  // x = h + l * eps  (h integer literal ±32767 ok, l 0..65535 → l*eps: l is
  // literal int up to 65535 — wraps at 32768! split l into l1*256+l0 with
  // l1,l0 < 256: x = h + (l1 * 256 + l0) * eps. All literals small ints. ✓
  return null; // built inline below
}

export function buildBatchSource(expr, raws, setup = "") {
  let src = "eps = 1 >> 16\n";
  if (setup) src += setup + "\n";
  src += `export var ys = array(${raws.length})\n`;
  src += `export var n = ${raws.length}\n`;
  const lines = raws.map((r, i) => {
    const h = Math.floor(r / 65536);
    const l = r - h * 65536; // 0..65535
    const l1 = Math.floor(l / 256);
    const l0 = l % 256;
    // l1*(256*eps) stays in raw space — a bare l1*256 would wrap as a value
    return `x = ${h} + ${l1} * (256 * eps) + ${l0} * eps\nys[${i}] = ${expr}`;
  });
  src += lines.join("\n") + "\n";
  src += "export function render(index) { hsv(0, 0, 0) }\n";
  return src;
}

const PRNG_SOURCE = `// capture the prng state chain: prngSeed returns the OLD state
export var states = array(24)
export var outs = array(24)
export var seedmap = array(8)
eps = 1 >> 16
// state right after seeding with various seeds
seeds = [1, 2, 42, 12345, 31337, 100, 255, 4096]
for (i = 0; i < 8; i++) {
  prngSeed(seeds[i])
  s = prngSeed(0)
  seedmap[i] = s
}
// chain from seed 42: state after each prng(1) call
prngSeed(42)
for (i = 0; i < 24; i++) {
  outs[i] = prng(1)
  s = prngSeed(0)
  states[i] = s
  prngSeed(s)
}
export function render(index) { hsv(0, 0, 0) }
`;

async function runSweep(pb, compile, name, def) {
  const results = [];
  for (let off = 0; off < def.inputs.length; off += BATCH) {
    const batch = def.inputs.slice(off, off + BATCH);
    const src = buildBatchSource(def.expr, batch, def.setup);
    const compiled = compile(src);
    if (!compiled.ok) throw new Error(`${name}: PB compile failed: ${compiled.error}`);
    await pb.setCode(packBytecode(compiled));
    let vars = null;
    for (let tries = 0; tries < 8; tries++) {
      await sleep(300);
      vars = await pb.getVars();
      if (Array.isArray(vars.ys) && vars.n === batch.length) break;
    }
    if (!Array.isArray(vars?.ys)) throw new Error(`${name}: ys never arrived`);
    batch.forEach((x, i) => results.push([x, Math.round(vars.ys[i] * 65536)]));
    console.error(`${name}: ${results.length}/${def.inputs.length}`);
  }
  return results;
}

async function main() {
  fs.mkdirSync(OUT, { recursive: true });
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);
  const pb = await PB.connect(ip);
  const { settings, seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;
  console.error(`device: ${settings.name} fw ${settings.ver}`);

  try {
    // prng chain first (special shape)
    if (only.length === 0 || only.includes("prng")) {
      const compiled = compile(PRNG_SOURCE);
      if (!compiled.ok) throw new Error(`prng compile: ${compiled.error}`);
      await pb.setCode(packBytecode(compiled));
      let vars = null;
      for (let tries = 0; tries < 8; tries++) {
        await sleep(300);
        vars = await pb.getVars();
        if (Array.isArray(vars.states)) break;
      }
      const toRaws = (a) => a.map((v) => Math.round(v * 65536));
      fs.writeFileSync(
        `${OUT}/prng.json`,
        JSON.stringify(
          {
            seeds: [1, 2, 42, 12345, 31337, 100, 255, 4096],
            seedmap: toRaws(vars.seedmap),
            states: toRaws(vars.states),
            outs: toRaws(vars.outs),
          },
          null,
          1,
        ),
      );
      console.error("prng: saved");
    }

    for (const [name, def] of Object.entries(SWEEPS)) {
      if (only.length > 0 && !only.includes(name)) continue;
      const data = await runSweep(pb, compile, name, def);
      fs.writeFileSync(`${OUT}/${name}.json`, JSON.stringify(data));
      console.error(`${name}: saved ${data.length} samples`);
    }
  } finally {
    if (restoreId) await pb.setActivePattern(restoreId).catch(() => {});
    await pb.close();
  }
}

// Only probe the device when run directly — compare-sweeps.mjs imports the
// sweep definitions without touching the PB.
if (import.meta.url === (await import("node:url")).pathToFileURL(process.argv[1]).href) {
  main().then(
    () => process.exit(0),
    (e) => {
      console.error("fatal:", e.message ?? e);
      process.exit(1);
    },
  );
}
