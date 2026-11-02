// Contact sheet for LIBRARY patterns (no corpus needed): renders each file on a
// 32x32 grid rig (1024 px) and, for strip-kind patterns, a 60 px strip
// waterfall, then writes <out>/<stem>.png — N frames side by side, upscaled —
// plus <out>/<stem>.json with the frame-time and runtime-error summary.
//
//   nix develop -c node tools/verify/sheet.mjs <outdir> [--grid 32x32] [--seconds 8]
//        [--frames 8] [--scale 4] [--fps 20] library/a.js library/b.js …
//
// Patterns that bind sensors get a synthetic music frame so sound patterns
// render lit. Wall clock is pinned. Exit code 0 even when a pattern errors:
// the JSON carries `compileError` / `runtimeErrors` for the reader.

import fs from "node:fs";
import path from "node:path";
import { load, sensorSlots } from "./enginehost.mjs";
import { encodePNG, upscale } from "./png.mjs";

const args = process.argv.slice(2);
const opts = { grid: "32x32", seconds: 8, frames: 8, scale: 4, fps: 20 };
const files = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a.startsWith("--")) opts[a.slice(2)] = args[++i];
  else files.push(a);
}
const out = files.shift();
if (!out || files.length === 0) {
  console.error("usage: sheet.mjs <outdir> [--grid WxH] [--seconds S] [--frames N] [--scale K] library/x.js …");
  process.exit(2);
}
fs.mkdirSync(out, { recursive: true });
const [GW, GH] = opts.grid.split("x").map(Number);
const seconds = Number(opts.seconds), nFrames = Number(opts.frames), scale = Number(opts.scale), fps = Number(opts.fps);

const lx = await load();
lx.setWallClock?.(1756000000);

function music(t) {
  const fd = new Array(32).fill(0).map((_, i) => Math.max(0, 0.6 * Math.exp(-i / 8) * (0.5 + 0.5 * Math.sin(t * 7 + i))));
  return sensorSlots({ frequencyData: fd, energyAverage: 0.3 + 0.2 * Math.sin(t * 6), maxFrequencyMagnitude: 0.5, maxFrequency: 440, light: 0.5 });
}

function run(source, pixelCount, mapGrid) {
  const eng = lx.compile(source, pixelCount, 1);
  if (eng.compileError) return { compileError: eng.compileError };
  try { eng.setWallClock(1756000000); } catch {}
  if (mapGrid) eng.setMapGrid(mapGrid[0], mapGrid[1]);
  const total = Math.round(seconds * fps);
  const keep = new Set();
  for (let k = 0; k < nFrames; k++) keep.add(Math.min(total - 1, Math.round(((k + 0.5) * total) / nFrames)));
  const frames = [];
  const errors = [];
  let lit = 0;
  const t0 = performance.now();
  for (let f = 0; f < total; f++) {
    if (eng.wantsSensors()) eng.setSensors(music(f / fps));
    const px = eng.frame(1000 / fps);
    const err = eng.takeError();
    if (err && errors.length < 3) errors.push(err.message);
    if (keep.has(f)) frames.push(px);
    for (let i = 0; i < px.length; i++) if (px[i] > 8) { lit++; break; }
  }
  const ms = (performance.now() - t0) / total;
  eng.free();
  return { frames, errors, litFrames: lit, totalFrames: total, msPerFrame: ms };
}

for (const file of files) {
  const stem = path.basename(file, ".js");
  const src = fs.readFileSync(file, "utf8");
  const isStrip = !/render2D|render3D|renderBulk/.test(src) && /\brender\s*\(/.test(src);
  const summary = { file, grid: opts.grid };
  // grid sheet
  const g = run(src, GW * GH, [GW, GH]);
  if (g.compileError) {
    summary.compileError = g.compileError;
  } else {
    summary.grid_msPerFrame = +g.msPerFrame.toFixed(3);
    summary.grid_litFrames = `${g.litFrames}/${g.totalFrames}`;
    summary.runtimeErrors = g.errors;
    const gap = 2;
    const W = nFrames * (GW * scale + gap), H = GH * scale;
    const sheet = Buffer.alloc(W * H * 3);
    g.frames.forEach((px, k) => {
      const up = Buffer.from(upscale(GW, GH, px, scale, scale).rgb);
      const ox = k * (GW * scale + gap);
      for (let y = 0; y < H; y++) up.copy(sheet, (y * W + ox) * 3, y * GW * scale * 3, (y + 1) * GW * scale * 3);
    });
    fs.writeFileSync(path.join(out, `${stem}.png`), encodePNG(W, H, sheet));
  }
  // strip waterfall (60 px, one row per frame over `seconds`)
  if (isStrip && !g.compileError) {
    const s = run(src, 60, null);
    if (!s.compileError) {
      summary.strip_msPerFrame = +s.msPerFrame.toFixed(3);
      summary.strip_litFrames = `${s.litFrames}/${s.totalFrames}`;
      // re-run to capture every frame as a row
      const eng = lx.compile(src, 60, 1);
      const total = Math.round(seconds * fps);
      const rows = Buffer.alloc(60 * total * 3);
      for (let f = 0; f < total; f++) {
        if (eng.wantsSensors()) eng.setSensors(music(f / fps));
        const px = eng.frame(1000 / fps);
        eng.takeError();
        Buffer.from(px).copy(rows, f * 60 * 3);
      }
      eng.free();
      const up = Buffer.from(upscale(60, total, rows, 6, 2).rgb);
      fs.writeFileSync(path.join(out, `${stem}.strip.png`), encodePNG(360, total * 2, up));
    }
  }
  fs.writeFileSync(path.join(out, `${stem}.json`), JSON.stringify(summary));
  console.log(`${stem}: ${summary.compileError ? "COMPILE ERROR " + summary.compileError : `grid ${summary.grid_msPerFrame} ms/frame, lit ${summary.grid_litFrames}` + (summary.runtimeErrors?.length ? ` ERR ${summary.runtimeErrors[0]}` : "") + (isStrip ? ` | strip ${summary.strip_msPerFrame} ms` : "")}`);
}
