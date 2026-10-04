#!/usr/bin/env node
// Per-BUILTIN cost on a real device, in cycles per call (Gitea #938).
//
// `opbench.mjs` prices the dispatch loop and `patbench.mjs` prices a whole
// pattern; neither says what ONE `cos()` or ONE `hypot()` costs once the
// JIT has compiled the call. This does: for each builtin in the ladder it
// pushes a synthetic pattern whose render() calls it K times in a loop,
// sweeps K, and fits the slope of `vm_us` against K — which cancels the
// wire, the per-pixel entry, beforeRender and the pipeline. The slope of
// the EMPTY loop body (`acc = acc + t`) is subtracted, so what is left is
// the call: argument marshalling, the entry point, the kernel, the return.
//
//   nix develop -c node tools/builtinbench.mjs <device-ip> [opts]
//
//   --k 0,2,4          K values to sweep                 (default 0,2,4)
//   --samples N        /api/status samples per K         (default 5)
//   --settle MS        wait after a push                 (default 3000)
//   --mhz F            CPU clock for the cycle conversion (default 240)
//   --only a,b,c       run only these ladder rows (by name)
//   --json PATH        also write the rows to PATH
//   --label TEXT       row label in the output (default the device version)
//
// The device's pixel count is NOT changed (on a panel board it belongs to
// the layout); the pattern found running (`GET /api/pattern.lxp`) is
// re-posted at the end, as jit-diff.mjs does. Every row records
// `jit.state`, so an interpreted row cannot pass for a native one — a
// program this small compiles everywhere the JIT exists, and on a board
// with no backend every row is the interpreter's number, which the label
// says. Single socket, `connection: close`, retries with a pause: the
// 3-socket web pool is why (seengreat-panel skill).
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { lxpBody } from "../web/tools/lxp.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const flag = (name, dflt) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const VALUED = new Set(["--k", "--samples", "--settle", "--mhz", "--only", "--json", "--label"]);
const POS = argv.filter((a, i) => !a.startsWith("--") && !VALUED.has(argv[i - 1]));
const IP = POS[0];
if (!IP) {
  console.error("usage: node tools/builtinbench.mjs <device-ip> [--k 0,2,4] [--only cos,hypot] [--json out.json]");
  process.exit(2);
}
const KS = flag("--k", "0,2,4").split(",").map(Number);
const SAMPLES = Number(flag("--samples", 5));
const SETTLE = Number(flag("--settle", 3000));
const MHZ = Number(flag("--mhz", 240));
const ONLY = flag("--only", null)?.split(",");
const JSON_OUT = flag("--json", null);
const DEV = `http://${IP}`;
const CALL_TIMEOUT_MS = 30_000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The ladder. `t` is a per-iteration value in roughly [0, 3) that the
// compiler cannot fold (it depends on `index` and `i`); `a` is in [0, 1).
// Every row's expression is numeric, so `acc` stays a number and the
// JIT's typed path applies throughout. The `base` row IS the loop body
// with no call at all and is subtracted from every other row.
const LADDER = [
  ["base", "t"],
  ["userfn", "ident(t)"],
  ["abs", "abs(t)"],
  ["floor", "floor(t)"],
  ["frac", "frac(t)"],
  ["clamp", "clamp(t, 0, 1)"],
  ["min", "min(t, a)"],
  ["mix", "mix(t, a, 0.3)"],
  ["mod", "mod(t, 0.7)"],
  ["mul", "t * a"],
  ["div", "t / (a + 0.3)"],
  ["div_int", "t / 3"],
  ["div_small", "(a * 0.001) / (a + 0.3)"],
  ["sqrt", "sqrt(t)"],
  ["hypot", "hypot(t, a)"],
  ["dist", "dist(0, 0, t, a)"],
  ["sin", "sin(t)"],
  ["cos", "cos(t)"],
  ["tan", "tan(t * 0.3)"],
  ["wave", "wave(t)"],
  ["triangle", "triangle(t)"],
  ["square", "square(t)"],
  ["atan", "atan(t)"],
  ["atan2", "atan2(t, a + 0.5)"],
  ["asin", "asin(a)"],
  ["acos", "acos(a)"],
  ["pow_int", "pow(t + 1, 3)"],
  ["pow_frac", "pow(t + 1, 2.5)"],
  ["exp", "exp(t)"],
  ["log", "log(t + 1)"],
  ["log2", "log2(t + 1)"],
  ["hash", "hash(t)"],
  ["smoothstep", "smoothstep(0, 1, t)"],
  ["step", "step(1, t)"],
  ["sign", "sign(t - 1)"],
  ["map", "map(t, 0, 3, 0, 1)"],
  ["time", "time(0.1)"],
  ["random", "random(1)"],
  ["perlin", "perlin(t, a, 0)"],
  ["simplex2", "simplex2(t, a, 0)"],
  ["simplex3", "simplex3(t, a, 0, 0)"],
  ["hsv", "hsv(t, 1, a)"],
  ["rgb", "rgb(t, a, a)"],
].filter(([name]) => !ONLY || ONLY.includes(name));

const source = (expr, k) => `// builtinbench ${expr} K=${k}
function ident(x) { return x }
export function render(index) {
  var a = index / pixelCount
  var acc = 0
  for (var i = 0; i < ${k}; i++) {
    var t = a + i * 0.37
    acc = acc + ${expr}
  }
  rgb(acc, 0, 0)
}
`;

async function call(pathname, { body, ct, raw = false } = {}) {
  for (let attempt = 0; ; attempt++) {
    try {
      const init = {
        headers: { connection: "close", ...(ct ? { "content-type": ct } : {}) },
        signal: AbortSignal.timeout(CALL_TIMEOUT_MS),
      };
      const r = await fetch(DEV + pathname, body === undefined ? init : { ...init, method: "POST", body });
      if (raw) return Buffer.from(await r.arrayBuffer());
      const text = await r.text();
      if (!text.length) throw new Error("empty reply");
      try {
        return JSON.parse(text);
      } catch {
        return text;
      }
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(6_000);
    }
  }
}
const get = (p) => call(p);
const getBytes = (p) => call(p, { raw: true });
const post = (p, body, ct) => call(p, { body, ct });

function fit(xs, ys) {
  const n = xs.length;
  const mx = xs.reduce((s, v) => s + v, 0) / n;
  const my = ys.reduce((s, v) => s + v, 0) / n;
  let sxy = 0;
  let sxx = 0;
  for (let i = 0; i < n; i++) {
    sxy += (xs[i] - mx) * (ys[i] - my);
    sxx += (xs[i] - mx) ** 2;
  }
  const slope = sxy / sxx;
  const intercept = my - slope * mx;
  let ssres = 0;
  let sstot = 0;
  for (let i = 0; i < n; i++) {
    ssres += (ys[i] - (intercept + slope * xs[i])) ** 2;
    sstot += (ys[i] - my) ** 2;
  }
  return { slope, intercept, r2: sstot === 0 ? 1 : 1 - ssres / sstot };
}
const median = (arr) => {
  const s = [...arr].sort((x, y) => x - y);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

const status0 = await get("/api/status");
const PIXELS = status0.pixels;
const foundPattern = await getBytes("/api/pattern.lxp");
const label = flag("--label", `v${status0.version} ${status0.slot}`);
console.log(
  `device ${IP}: v${status0.version} slot ${status0.slot}, ${PIXELS} px, jit ${status0.jit?.state ?? "n/a"} — ` +
    `${LADDER.length} rows × K ${KS.join("/")} × ${SAMPLES} samples`,
);

/** Push one program and return the median vm_us plus the JIT state. */
async function measure(expr, k) {
  await post("/api/code", await lxpBody("", source(expr, k)), "application/octet-stream");
  await sleep(SETTLE);
  const vm = [];
  let jit = null;
  for (let i = 0; i < SAMPLES; i++) {
    const s = await get("/api/status");
    if (s.vmerr) throw new Error(`vm error for ${expr} K=${k}: ${JSON.stringify(s.vmerr)}`);
    vm.push(s.vm_us);
    jit = s.jit?.state ?? "n/a";
    await sleep(600);
  }
  return { vm_us: median(vm), spread: Math.max(...vm) - Math.min(...vm), jit };
}

const rows = [];
let baseSlope = null;
try {
  for (const [name, expr] of LADDER) {
    const pts = [];
    for (const k of KS) pts.push({ k, ...(await measure(expr, k)) });
    const f = fit(pts.map((p) => p.k), pts.map((p) => p.vm_us));
    const usPerIter = f.slope / PIXELS;
    const row = {
      name,
      expr,
      jit: pts[pts.length - 1].jit,
      slope_us_per_k: f.slope,
      r2: f.r2,
      cycles_per_iter: usPerIter * MHZ,
      samples: pts,
    };
    if (name === "base") baseSlope = row.cycles_per_iter;
    row.cycles_per_call = baseSlope === null ? null : row.cycles_per_iter - baseSlope;
    rows.push(row);
    const spreads = pts.map((p) => p.spread).join("/");
    console.log(
      `  ${name.padEnd(10)} ${String(Math.round(row.cycles_per_iter)).padStart(6)} cyc/iter` +
        (row.cycles_per_call === null ? "" : `  ${String(Math.round(row.cycles_per_call)).padStart(6)} cyc/call`) +
        `  r2 ${f.r2.toFixed(4)}  jit ${row.jit}  (spreads ${spreads})  ${expr}`,
    );
  }
} finally {
  try {
    if (foundPattern.length) await post("/api/code", foundPattern, "application/octet-stream");
    console.log(`restored: the pattern found running, ${PIXELS} px untouched`);
  } catch (e) {
    console.log(`restore failed: ${String(e).slice(0, 120)} — the device may need a manual restore`);
  }
}

console.log("");
console.log(`| builtin | cycles/call | r² | jit |`);
console.log(`|---|---:|---:|---|`);
for (const r of rows) {
  if (r.name === "base") continue;
  console.log(`| \`${r.expr}\` | ${Math.round(r.cycles_per_call)} | ${r.r2.toFixed(3)} | ${r.jit} |`);
}
console.log(`(loop body alone: ${Math.round(baseSlope)} cycles/iter; label ${label}; ${PIXELS} px; ${MHZ} MHz)`);

if (JSON_OUT) {
  writeFileSync(
    JSON_OUT,
    JSON.stringify({ ip: IP, label, version: status0.version, slot: status0.slot, pixels: PIXELS, mhz: MHZ, ks: KS, rows }, null, 2) + "\n",
  );
  console.log(`wrote ${JSON_OUT}`);
}
