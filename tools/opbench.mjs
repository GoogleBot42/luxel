#!/usr/bin/env node
// Per-op interpreter cost on a real device (Gitea #312).
//
// Measures how many microseconds — and how many CPU cycles — one bytecode
// operation costs inside the dispatch loop, by pushing a synthetic pattern
// whose render() runs a K-iteration inner loop and sweeping K. Taking the
// SLOPE of vm_us against K cancels everything that is not the loop: the
// wire, the per-pixel VM entry, beforeRender, the frame pipeline. What is
// left is pure dispatch.
//
//   nix develop -c node tools/opbench.mjs <device-ip> [opts]
//
//   --k 0,50,100,200   K values to sweep          (default 0,50,100,200)
//   --pixels N         pixel count for the run    (default 256)
//   --samples N        /api/status samples per K  (default 7)
//   --settle MS        wait after a push          (default 4000)
//   --mhz F            CPU clock for the cycle    (default 240)
//                      conversion
//   --label TEXT       row label in the output    (default the device version)
//   --json PATH        also write the raw samples + fit to PATH
//
// Ops per iteration come from the host profiler on the SAME sources
// (`luxel bench --profile --json`, built here with --features profile),
// as the slope of insns/px against K — so a compiler change that alters
// the fused shape is reflected automatically instead of being hardcoded.
//
// The device is restored to the pixel count, brightness and pattern it was
// found with. It is a MEASUREMENT tool, not a stability check: a device
// that stops answering aborts the run.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { lxpBody } from "../web/tools/lxp.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const flag = (name, dflt) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
// Positional = anything that is neither a flag nor a flag's value.
const VALUED = new Set(["--k", "--pixels", "--samples", "--settle", "--mhz", "--label", "--json"]);
const POS = argv.filter((a, i) => !a.startsWith("--") && !VALUED.has(argv[i - 1]));
const IP = POS[0];
if (!IP) {
  console.error("usage: node tools/opbench.mjs <device-ip> [--k 0,50,100,200] [--pixels N]");
  process.exit(2);
}
const KS = flag("--k", "0,50,100,200").split(",").map(Number);
const PIXELS = Number(flag("--pixels", 256));
const SAMPLES = Number(flag("--samples", 7));
const SETTLE = Number(flag("--settle", 4000));
const MHZ = Number(flag("--mhz", 240));
const JSON_OUT = flag("--json", null);
const DEV = `http://${IP}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The pattern under test. The inner loop is `x += i * 0.5` — cheap
// arithmetic on locals only, no builtins, no arrays: what is being timed
// is the dispatch loop itself, not any one operation's implementation.
const source = (k) => `// opbench K=${k}
export function render(index) {
  var x = index
  for (var i = 0; i < ${k}; i++) {
    x = x + i * 0.5
  }
  hsv(x, 1, 1)
}
`;

async function api(path, body) {
  for (let attempt = 0; ; attempt++) {
    try {
      const opts = { headers: { connection: "close" }, signal: AbortSignal.timeout(30_000) };
      const r = await fetch(DEV + path, body === undefined ? opts : { ...opts, method: "POST", body });
      const t = await r.text();
      try {
        return JSON.parse(t);
      } catch {
        return t;
      }
    } catch (e) {
      if (attempt >= 3) throw e;
      await sleep(2000);
    }
  }
}

/** Least-squares slope + intercept of y against x. */
function fit(xs, ys) {
  const n = xs.length;
  const mx = xs.reduce((a, b) => a + b, 0) / n;
  const my = ys.reduce((a, b) => a + b, 0) / n;
  let sxy = 0;
  let sxx = 0;
  for (let i = 0; i < n; i++) {
    sxy += (xs[i] - mx) * (ys[i] - my);
    sxx += (xs[i] - mx) ** 2;
  }
  const slope = sxy / sxx;
  const intercept = my - slope * mx;
  // r^2, so a nonlinear sweep (thermal throttling, a background task) is
  // visible instead of being averaged into a confident wrong number.
  let ssres = 0;
  let sstot = 0;
  for (let i = 0; i < n; i++) {
    ssres += (ys[i] - (intercept + slope * xs[i])) ** 2;
    sstot += (ys[i] - my) ** 2;
  }
  return { slope, intercept, r2: sstot === 0 ? 1 : 1 - ssres / sstot };
}

const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

/** insns/px for each K from the host profiler on the same sources. */
function hostInsnsPerPx() {
  const targetDir = join(root, "target/profile");
  execFileSync(
    "cargo",
    ["build", "--release", "-p", "luxel-cli", "--features", "profile", "--target-dir", targetDir],
    { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
  );
  const bin = join(targetDir, "release/luxel");
  const dir = mkdtempSync(join(tmpdir(), "opbench-"));
  return KS.map((k) => {
    const f = join(dir, `k${k}.js`);
    writeFileSync(f, source(k));
    const out = execFileSync(
      bin,
      ["bench", f, "--profile", "--json", "--pixels", "64", "--frames", "5", "--out", "-"],
      { cwd: root, encoding: "utf8" },
    );
    const line = out.trim().split("\n").filter((l) => l.startsWith("{")).pop();
    return JSON.parse(line).insns_per_px;
  });
}

const status0 = await api("/api/status");
const cfg0 = await api("/api/config");
console.log(`device ${IP}: v${status0.version} slot ${status0.slot}, ${status0.pixels} px`);

const opsPerPx = hostInsnsPerPx();
const opsFit = fit(KS, opsPerPx);
console.log(`host profile: insns/px ${opsPerPx.map((v) => v.toFixed(1)).join(" ")} → ${opsFit.slope.toFixed(2)} ops per loop iteration (r2 ${opsFit.r2.toFixed(4)})`);

await api("/api/config", String(PIXELS));
await sleep(1500);

const rows = [];
for (const k of KS) {
  await api("/api/code", await lxpBody("", source(k)));
  await sleep(SETTLE);
  const vm = [];
  const frame = [];
  for (let i = 0; i < SAMPLES; i++) {
    const s = await api("/api/status");
    if (s.vmerr) throw new Error(`vm error at K=${k}: ${JSON.stringify(s.vmerr)}`);
    vm.push(s.vm_us);
    frame.push(s.frame_us);
    await sleep(700);
  }
  const row = { k, vm_us: median(vm), frame_us: median(frame), vm_samples: vm };
  rows.push(row);
  const spread = Math.max(...vm) - Math.min(...vm);
  console.log(`  K=${String(k).padStart(4)}  vm_us ${String(row.vm_us).padStart(8)}  (spread ${spread})  frame_us ${row.frame_us}`);
}

const vmFit = fit(rows.map((r) => r.k), rows.map((r) => r.vm_us));
// vm_us is the whole frame: PIXELS render() calls. Per-iteration cost is
// the slope divided by the pixel count.
const usPerIter = vmFit.slope / PIXELS;
const cyclesPerIter = usPerIter * MHZ;
const opsPerIter = opsFit.slope;
const cyclesPerOp = cyclesPerIter / opsPerIter;

console.log("");
console.log(`slope        ${vmFit.slope.toFixed(1)} vm_us per K (r2 ${vmFit.r2.toFixed(5)}), intercept ${vmFit.intercept.toFixed(0)} µs`);
console.log(`per iter     ${usPerIter.toFixed(3)} µs = ${cyclesPerIter.toFixed(0)} cycles @ ${MHZ} MHz`);
console.log(`per op       ${opsPerIter.toFixed(2)} ops/iter → ${cyclesPerOp.toFixed(1)} cycles/op`);
console.log(`(label: ${flag("--label", `v${status0.version} ${status0.slot}`)})`);

if (JSON_OUT) {
  writeFileSync(
    JSON_OUT,
    JSON.stringify(
      {
        ip: IP,
        label: flag("--label", `v${status0.version} ${status0.slot}`),
        version: status0.version,
        slot: status0.slot,
        pixels: PIXELS,
        mhz: MHZ,
        ks: KS,
        rows,
        ops_per_px: opsPerPx,
        ops_per_iter: opsPerIter,
        vm_fit: vmFit,
        us_per_iter: usPerIter,
        cycles_per_iter: cyclesPerIter,
        cycles_per_op: cyclesPerOp,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(`wrote ${JSON_OUT}`);
}

// Restore what we found: the pixel count, and rainbow as the pattern (the
// device has no "what was playing" query, so this matches hw-bench.mjs).
await api("/api/config", String(status0.pixels));
await sleep(1000);
await api("/api/code", await lxpBody("", readFileSync(join(root, "library/rainbow.js"), "utf8")));
console.log(`(device restored to ${status0.pixels} px, rainbow; protocol ${cfg0?.protocol ?? "?"})`);
