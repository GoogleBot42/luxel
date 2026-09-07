#!/usr/bin/env node
// Host A/B throughput bench for PAIRS of patterns that render the same
// thing two ways (Gitea: the `renderFrame` bulk-op evaluation).
//
//   nix develop -c node tools/pairbench.mjs [opts] a1.js b1.js [a2.js b2.js …]
//
//   --pixels N     strip length                         (default 4096)
//   --grid WxH     install a coordinate map that size   (implies --pixels W*H)
//   --frames N     frames per run                       (default 2000)
//   --rounds N     rounds; the BEST run of each wins    (default 5)
//   --luxel PATH   the binary to drive        (default target/release/luxel)
//   --json PATH    also write the rows as JSON
//   --labels a,b   column names for the two sides       (default px,bulk)
//
// Files are consumed two at a time: the first of each pair is the baseline.
// Both sides of a pair are run BACK TO BACK inside every round and the
// maximum px/s of each is reported, because host `luxel bench` on this box
// has 6-19 % run-to-run spread and throughput noise only ever costs time —
// so the max is the least biased estimator (.claude/rules/vm-bytecode.md).
// Treat a ratio inside +/-3 % as no difference. Host numbers UNDERSTATE the
// device win by roughly 3x: an x86 per-pixel VM entry is cheap, the Xtensa
// one is 317-440 cycles/px (docs/boards.md "Second light").
import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { basename } from "node:path";

const argv = process.argv.slice(2);
const opt = { pixels: 4096, frames: 2000, rounds: 5, grid: null, json: null,
  luxel: "target/release/luxel", labels: ["px", "bulk"] };
const files = [];
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (a === "--pixels") opt.pixels = +argv[++i];
  else if (a === "--frames") opt.frames = +argv[++i];
  else if (a === "--rounds") opt.rounds = +argv[++i];
  else if (a === "--grid") opt.grid = argv[++i];
  else if (a === "--json") opt.json = argv[++i];
  else if (a === "--luxel") opt.luxel = argv[++i];
  else if (a === "--labels") opt.labels = argv[++i].split(",");
  else if (a.startsWith("--")) { console.error(`unknown option ${a}`); process.exit(2); }
  else files.push(a);
}
if (files.length < 2 || files.length % 2) {
  console.error("usage: pairbench.mjs [opts] baseline.js variant.js [ … ]");
  process.exit(2);
}
if (opt.grid) {
  const [w, h] = opt.grid.split("x").map(Number);
  if (!w || !h) { console.error("--grid expects WxH"); process.exit(2); }
  opt.pixels = w * h;
}

// One `luxel bench` run -> px/s. The rate is parsed from the summary line
// rather than timed here, so the harness never counts process startup.
// `luxel bench` writes that line to STDERR, so both streams are read.
function run(file) {
  const args = ["bench", file, "--pixels", String(opt.pixels),
    "--frames", String(opt.frames)];
  if (opt.grid) args.push("--map-grid", opt.grid);
  const r = spawnSync(opt.luxel, args, { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`${opt.luxel} bench ${file} failed:\n${r.stderr}`);
  const out = `${r.stdout}\n${r.stderr}`;
  const m = out.match(/([\d.]+) px\/s/);
  if (!m) throw new Error(`no px/s in luxel output for ${file}:\n${out}`);
  return +m[1];
}

const rig = opt.grid ? `${opt.pixels} px, grid ${opt.grid}` : `${opt.pixels} px, no map`;
console.log(`# pairbench: ${rig}, ${opt.frames} frames, best of ${opt.rounds}`);
const rows = [];
for (let p = 0; p < files.length; p += 2) {
  const pair = [files[p], files[p + 1]];
  const best = [0, 0];
  for (let r = 0; r < opt.rounds; r++)
    for (const s of [0, 1]) best[s] = Math.max(best[s], run(pair[s]));
  const ns = best.map((v) => 1e9 / v);
  rows.push({
    pair: basename(pair[0]).replace(/-(px|bulk)\.js$/, ""),
    baseline: pair[0], variant: pair[1],
    pixels: opt.pixels, grid: opt.grid, frames: opt.frames, rounds: opt.rounds,
    pxs: { [opt.labels[0]]: best[0], [opt.labels[1]]: best[1] },
    nsPerPx: { [opt.labels[0]]: ns[0], [opt.labels[1]]: ns[1] },
    ratio: best[1] / best[0],
  });
}
const w = Math.max(12, ...rows.map((r) => r.pair.length));
const h = (s, n) => String(s).padStart(n);
console.log(`| ${"pair".padEnd(w)} | ${h(opt.labels[0] + " px/s", 14)} | ${h(opt.labels[1] + " px/s", 14)} | ${h(opt.labels[0] + " ns/px", 12)} | ${h(opt.labels[1] + " ns/px", 12)} | ${h("ratio", 7)} |`);
console.log(`|${"-".repeat(w + 2)}|${"-".repeat(16)}|${"-".repeat(16)}|${"-".repeat(14)}|${"-".repeat(14)}|${"-".repeat(9)}|`);
for (const r of rows) {
  const [a, b] = opt.labels;
  console.log(`| ${r.pair.padEnd(w)} | ${h(r.pxs[a].toFixed(0), 14)} | ${h(r.pxs[b].toFixed(0), 14)} | ${h(r.nsPerPx[a].toFixed(2), 12)} | ${h(r.nsPerPx[b].toFixed(2), 12)} | ${h(r.ratio.toFixed(2) + "x", 7)} |`);
}
if (opt.json) { writeFileSync(opt.json, JSON.stringify({ rig, opt, rows }, null, 2)); console.error(`wrote ${opt.json}`); }
