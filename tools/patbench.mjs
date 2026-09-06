#!/usr/bin/env node
// Per-pattern VM cost on a real device (Gitea #318).
//
// The companion to tools/opbench.mjs, and the reason it exists: opbench's
// K-sweep measures the DISPATCH LOOP in isolation, and an engine change can
// win there and lose catastrophically on a real pattern. Measured on the
// Athom (classic ESP32), inlining the fused const-arg arms' `binop_const`
// was −7.5 % on the loop microbench and **+38 % on a noise-heavy pattern**,
// because `Vm::run` growing pushes the 21 KB `call_builtin` out of what
// stays resident in the flash instruction cache. The effect is not even
// monotonic in size — it is layout. So: judge any luxel-core change on BOTH
// tools, and prefer a builtin-heavy pattern here.
//
//   nix develop -c node tools/patbench.mjs <device-ip> <pattern> [opts]
//
//   <pattern>        a name in library/ (no .js), or a path to a .js file
//   --pixels N       pixel count for the run       (default 256)
//   --samples N      /api/status samples           (default 9)
//   --settle MS      wait after the push           (default 5000)
//   --repeat N       repeat the whole measurement  (default 1)
//   --json PATH      write the samples + medians to PATH
//
// Reports the MEDIAN vm_us and vm_us/px — vm_us is the frame's VM time, so
// vm_us/px is directly comparable across pixel counts and across builds.
//
// Pick a STATELESS pattern. `snake-2d` and friends carry game state whose
// per-frame work varies, which makes them useless as an A/B probe;
// `perlin-fire-wind-tunnel` is a pure function of time and coordinates and
// repeats to ±0.3 % on this rig.
//
// The device is restored to the pixel count it was found with, running
// library/rainbow.js. It is a MEASUREMENT tool, not a stability check.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { lxpBody } from "../web/tools/lxp.mjs";

const ROOT = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const argv = process.argv.slice(2);
const positional = argv.filter((a) => !a.startsWith("--") && !isOptValue(a));
function isOptValue(a) {
  const i = argv.indexOf(a);
  return i > 0 && argv[i - 1].startsWith("--");
}
const [ip, pattern] = positional;
if (!ip || !pattern) {
  console.error("usage: node tools/patbench.mjs <device-ip> <pattern> [--pixels N] [--samples N] [--settle MS] [--repeat N] [--json PATH]");
  process.exit(2);
}
const opt = (n, d) => {
  const i = argv.indexOf(`--${n}`);
  return i >= 0 && argv[i + 1] !== undefined ? argv[i + 1] : d;
};
const PIXELS = Number(opt("pixels", "256"));
const SAMPLES = Number(opt("samples", "9"));
const SETTLE = Number(opt("settle", "5000"));
const REPEAT = Number(opt("repeat", "1"));
const JSON_OUT = opt("json", null);

const srcPath = pattern.endsWith(".js") ? pattern : path.join(ROOT, "library", `${pattern}.js`);
if (!fs.existsSync(srcPath)) {
  console.error(`no such pattern: ${srcPath}`);
  process.exit(2);
}
const src = fs.readFileSync(srcPath, "utf8");
const rainbow = fs.readFileSync(path.join(ROOT, "library", "rainbow.js"), "utf8");

const H = { connection: "close" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const status = async () => (await fetch(`http://${ip}/api/status`, { headers: H })).json();
async function post(p, body, ct) {
  const r = await fetch(`http://${ip}${p}`, { method: "POST", body, headers: { ...H, "content-type": ct } });
  if (!r.ok) throw new Error(`${p} ${r.status}: ${await r.text()}`);
}
const push = async (s) => post("/api/code", await lxpBody("patbench", s), "application/octet-stream");
const median = (xs) => xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)];

const found = await status();
console.log(`found: ${found.pixels} px, slot ${found.slot}, v${found.version}, fps ${found.fps}`);
const runs = [];
try {
  if (found.pixels !== PIXELS) {
    await post("/api/config", String(PIXELS), "text/plain");
    await sleep(2500);
  }
  for (let r = 0; r < REPEAT; r++) {
    await push(src);
    await sleep(SETTLE);
    const s = [];
    for (let i = 0; i < SAMPLES; i++) {
      await sleep(1000);
      s.push(await status());
    }
    const vm_us = median(s.map((x) => x.vm_us));
    const fps = median(s.map((x) => x.fps));
    const vmerr = s.at(-1).vmerr ?? null;
    runs.push({ vm_us, fps, us_per_px: vm_us / PIXELS, vmerr, samples: s.map((x) => x.vm_us) });
    console.log(
      `${path.basename(srcPath, ".js")} @ ${PIXELS} px: vm_us ${vm_us}  fps ${fps}  ` +
        `${(vm_us / PIXELS).toFixed(3)} µs/px  spread ${Math.max(...s.map((x) => x.vm_us)) - Math.min(...s.map((x) => x.vm_us))}` +
        (vmerr ? `  vmerr=${vmerr}` : ""),
    );
    if (vmerr) console.error(`WARNING: the pattern reported a runtime error — the number is meaningless`);
  }
} finally {
  await push(rainbow).catch((e) => console.log(`restore pattern failed: ${e}`));
  await post("/api/config", String(found.pixels), "text/plain").catch((e) => console.log(`restore pixels failed: ${e}`));
  await sleep(2000);
  const back = await status();
  console.log(`restored: ${back.pixels} px, fps ${back.fps}, slot ${back.slot}`);
}
if (JSON_OUT) {
  fs.writeFileSync(JSON_OUT, JSON.stringify({ ip, pattern, pixels: PIXELS, runs }, null, 2));
  console.log(`wrote ${JSON_OUT}`);
}
