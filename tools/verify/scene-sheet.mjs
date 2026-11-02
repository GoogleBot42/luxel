// Contact sheet for LIBRARY scenes and sprites — sheet.mjs's twin for
// `library/scenes/*.scene` and `library/sprites/*.sprite`. A scene is drawn
// the way the playground draws it (web/src/lib/sceneRender.ts): the wasm
// compositor (`lx_comp_new/set/frame`) on a square SIZExSIZE grid, one engine
// per pattern layer, each `@pat/<slug>` / `@spr/<slug>` resolved to its
// library file through web/src/lib/librarySource.ts, clock text formatted from
// the pinned wall clock and slot text from `--slots`. Writes
// <out>/<slug>.png — N frames side by side, spread over the scene's keyframe
// loop (the longest track's last key) or `--seconds` when nothing loops — plus
// <out>/<slug>.json with the layer list, the period, host ms/frame and every
// compile/runtime error.
//
//   nix develop -c node --experimental-strip-types tools/verify/scene-sheet.mjs <outdir>
//        [--size 64] [--frames 6] [--seconds 10] [--scale K] [--fps 30]
//        [--slots "slot0|slot1|…"] library/scenes/a.scene …
//   nix develop -c node --experimental-strip-types tools/verify/scene-sheet.mjs <outdir>
//        --sprites [--scale K] library/sprites/a.sprite …
//
// `--scale` defaults to 256/SIZE (each frame ~256 px wide). `--sprites` writes
// each sprite's frames as a strip on a checkerboard (transparent texels show
// as the check), 16 frames to a row, plus <slug>.json {name, w, h, frames,
// fps, bytes}. Patterns that bind sensors get a synthetic music frame, as in
// sheet.mjs. Exit code 0 even when a scene errors: the JSON carries it.
//
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { load, sensorSlots } from "./enginehost.mjs";
import { encodePNG, upscale } from "./png.mjs";

const W = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const { parseSceneSource, parseSpriteSource } = await import(`${W}/web/src/lib/librarySource.ts`);
const { parseScene } = await import(`${W}/web/src/lib/scene.ts`);
const { encodeSprite } = await import(`${W}/web/src/lib/sprite.ts`);

const args = process.argv.slice(2);
const opts = { size: 64, frames: 6, seconds: 10, fps: 30, slots: "" };
const files = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === "--sprites") opts.sprites = true;
  else if (a.startsWith("--")) opts[a.slice(2)] = args[++i];
  else files.push(a);
}
const out = files.shift();
if (!out || files.length === 0) {
  console.error(
    "usage: scene-sheet.mjs <outdir> [--size N] [--frames N] [--seconds S] [--scale K] [--fps F] [--slots a|b] library/scenes/x.scene …\n" +
      "       scene-sheet.mjs <outdir> --sprites [--scale K] library/sprites/x.sprite …",
  );
  process.exit(2);
}
fs.mkdirSync(out, { recursive: true });
const slugOf = (f) => path.basename(f).replace(/\.(scene|sprite)$/, "");

if (opts.sprites) {
  for (const file of files) {
    const slug = slugOf(file);
    const r = parseSpriteSource(fs.readFileSync(file, "utf8"));
    if (!r.ok) {
      fs.writeFileSync(path.join(out, `${slug}.json`), JSON.stringify({ file, error: r.error }));
      console.log(`${slug}: ERROR ${r.error}`);
      continue;
    }
    const s = r.sprite;
    const sc = Number(opts.scale ?? Math.max(2, Math.floor(96 / Math.max(s.w, s.h))));
    const G = 4, cw = s.w * sc, chh = s.h * sc;
    const cols = Math.min(16, s.frames), rows = Math.ceil(s.frames / cols);
    const SW = cols * (cw + G) + G, SH = rows * (chh + G) + G;
    const img = new Uint8Array(SW * SH * 3).fill(60);
    for (let f = 0; f < s.frames; f++) {
      const ox = G + (f % cols) * (cw + G), oy = G + Math.floor(f / cols) * (chh + G);
      for (let y = 0; y < chh; y++)
        for (let x = 0; x < cw; x++) {
          const tx = Math.floor(x / sc), ty = Math.floor(y / sc);
          const k = s.index[f * s.w * s.h + ty * s.w + tx];
          const ck = (tx + ty) & 1 ? 18 : 28;
          const c = k === 0 ? [ck, ck, ck] : s.palette[k - 1];
          const o = ((oy + y) * SW + ox + x) * 3;
          img[o] = c[0];
          img[o + 1] = c[1];
          img[o + 2] = c[2];
        }
    }
    fs.writeFileSync(path.join(out, `${slug}.png`), encodePNG(SW, SH, img));
    const summary = { file, name: s.name, w: s.w, h: s.h, frames: s.frames, fps: s.fps, bytes: encodeSprite(s).length };
    fs.writeFileSync(path.join(out, `${slug}.json`), JSON.stringify(summary));
    console.log(`${slug}: ${s.w}x${s.h} ${s.frames} frames fps ${s.fps}, ${summary.bytes} B`);
  }
  process.exit(0);
}

const N = Number(opts.size), nFrames = Number(opts.frames), fps = Number(opts.fps);
const scale = Number(opts.scale ?? Math.max(1, Math.floor(256 / N)));
const slots = opts.slots === "" ? [] : String(opts.slots).split("|");
const CLOCK = 1756000000; // sheet.mjs's pinned wall clock
const PROJ = { index: 0, x: 1, y: 2, z: 3, xy: 4, xz: 5, yz: 6 };

const lx = await load();
const e = lx.e;
lx.setDefaultWallClock?.(CLOCK);

const spriteBySlug = new Map();
for (const f of fs.readdirSync(`${W}/library/sprites`)) {
  const r = parseSpriteSource(fs.readFileSync(`${W}/library/sprites/${f}`, "utf8"));
  if (r.ok) spriteBySlug.set(slugOf(f), encodeSprite(r.sprite));
}

function music(t) {
  const fd = new Array(32).fill(0).map((_, i) => Math.max(0, 0.6 * Math.exp(-i / 8) * (0.5 + 0.5 * Math.sin(t * 7 + i))));
  return sensorSlots({ frequencyData: fd, energyAverage: 0.3 + 0.2 * Math.sin(t * 6), maxFrequencyMagnitude: 0.5, maxFrequency: 440, light: 0.5 });
}

/** web/src/lib/sceneRender.ts `formatClock`, in UTC so the sheet is host-independent. */
function formatClock(fmt, at) {
  const p2 = (n) => String(n).padStart(2, "0");
  const h24 = at.getUTCHours(), h12 = h24 % 12 === 0 ? 12 : h24 % 12;
  const mm = p2(at.getUTCMinutes()), ss = p2(at.getUTCSeconds());
  switch (fmt) {
    case "HH:MM": return `${p2(h24)}:${mm}`;
    case "HH:MM:SS": return `${p2(h24)}:${mm}:${ss}`;
    case "hh:MM": return `${h12}:${mm}`;
    case "hh:MM:SS": return `${h12}:${mm}:${ss}`;
    case "MM-DD": return `${p2(at.getUTCMonth() + 1)}-${p2(at.getUTCDate())}`;
    default: return `${at.getUTCFullYear()}-${p2(at.getUTCMonth() + 1)}-${p2(at.getUTCDate())}`;
  }
}

function renderScene(file) {
  const slug = slugOf(file);
  const summary = { file, size: N, errors: [] };
  const src = parseSceneSource(fs.readFileSync(file, "utf8"));
  if (!src.ok) return { ...summary, errors: [src.error] };
  const parsed = parseScene(src.wire);
  if (!parsed.ok) return { ...summary, errors: [parsed.error] };
  const scene = parsed.scene;
  const refOf = new Map(src.refs.map((r) => [r.id, r]));
  summary.name = scene.name;
  summary.wireBytes = src.wire.length;
  summary.layers = scene.layers.map((l) => {
    const b = l.body;
    const ref =
      b.kind === "pat" ? `@pat/${refOf.get(b.pat.id)?.slug ?? b.pat.id}` :
      b.kind === "sprite" ? `@spr/${refOf.get(b.id)?.slug ?? b.id}` :
      b.kind === "color" ? `#${b.color}` :
      b.text.source.kind === "lit" ? JSON.stringify(b.text.source.text) :
      b.text.source.kind === "clock" ? `clock ${b.text.source.fmt}` : `slot ${b.text.source.slot}`;
    return { kind: b.kind, ref, name: l.name, blend: l.style.blend, visible: l.style.visible, ...(l.clip ? { clip: true } : {}) };
  });
  const period = Math.max(0, ...scene.layers.map((l) => (l.keys?.length ? l.keys[l.keys.length - 1].ms : 0)));
  const span = period > 0 ? period : Number(opts.seconds) * 1000;
  summary.periodMs = period;

  const ch = e.lx_comp_new(N, N);
  const w = lx.putStr(src.wire);
  const rc = e.lx_comp_set(ch, w.ptr, w.len);
  w.free();
  if (rc !== 0) {
    e.lx_comp_free(ch);
    return { ...summary, errors: [`lx_comp_set: ${lx.response()}`] };
  }
  const engines = [];
  const text = [];
  scene.layers.forEach((l, i) => {
    const b = l.body;
    text.push(null);
    if (b.kind === "pat") {
      const ref = refOf.get(b.pat.id);
      const pfile = ref && `${W}/library/${ref.slug}.js`;
      if (!pfile || !fs.existsSync(pfile)) return summary.errors.push(`layer ${i}: no pattern ${ref?.slug ?? b.pat.id}`);
      const eng = lx.compile(fs.readFileSync(pfile, "utf8"), N * N, 1);
      if (eng.compileError) return summary.errors.push(`layer ${i} ${ref.slug}: ${JSON.stringify(eng.compileError)}`);
      try { eng.setWallClock(CLOCK); } catch {}
      eng.setMapGrid(N, N);
      // geometry.ts withProjectionOverride: `P <mode>` replaces the field
      // matching the pattern's own dims in the default triple (index, z, xy)
      const triple = [0, 3, 4];
      const dims = e.lx_pattern_dims ? e.lx_pattern_dims(eng.h) : 0;
      if (b.pat.proj && dims > 0) triple[Math.min(dims, 3) - 1] = PROJ[b.pat.proj];
      e.lx_set_projection(eng.h, ...triple);
      for (const [name, vals] of Object.entries(b.pat.controls)) {
        if (eng.setControl(name, vals) === null) summary.errors.push(`layer ${i} ${ref.slug}: no control ${name}`);
      }
      engines.push({ eng, slug: ref.slug });
      e.lx_comp_bind(ch, i, eng.h);
    } else if (b.kind === "sprite") {
      const ref = refOf.get(b.id);
      const rec = ref && spriteBySlug.get(ref.slug);
      if (!rec) return summary.errors.push(`layer ${i}: no sprite ${ref?.slug ?? b.id}`);
      const p = e.lx_alloc(rec.length);
      new Uint8Array(e.memory.buffer).set(rec, p);
      if (e.lx_comp_sprite(ch, i, p, rec.length) !== 0) summary.errors.push(`layer ${i} ${ref.slug}: ${lx.response()}`);
      e.lx_dealloc(p, rec.length);
    } else if (b.kind === "text") {
      const s = b.text.source;
      text[i] = s.kind === "lit" ? () => s.text : s.kind === "slot" ? () => slots[s.slot] ?? "" : (ms) => formatClock(s.fmt, new Date(CLOCK * 1000 + ms));
    }
  });

  const dt = 1000 / fps;
  const want = [];
  for (let k = 0; k < nFrames; k++) want.push(Math.max(1, Math.round(((k + 0.5) * span) / nFrames / dt)));
  const shots = [];
  const runtime = new Set();
  const t0 = performance.now();
  const last = want[want.length - 1];
  for (let f = 1; f <= last; f++) {
    const ms = f * dt;
    text.forEach((fn, i) => {
      if (!fn) return;
      const b = lx.putStr(fn(ms));
      e.lx_comp_text(ch, i, b.ptr, b.len);
      b.free();
    });
    for (const { eng } of engines) if (eng.wantsSensors()) eng.setSensors(music(ms / 1000));
    const ptr = e.lx_comp_frame(ch, Math.round(dt * 65536));
    if (want.includes(f)) shots.push(ptr ? new Uint8Array(e.memory.buffer.slice(ptr, ptr + N * N * 3)) : new Uint8Array(N * N * 3));
    for (const { eng, slug: ps } of engines) {
      const err = eng.takeError();
      if (err && runtime.size < 3) runtime.add(`${ps}: ${err.message ?? JSON.stringify(err)}`);
    }
  }
  summary.msPerFrame = +((performance.now() - t0) / last).toFixed(3);
  summary.shotsMs = want.map((f) => Math.round(f * dt));
  summary.errors.push(...runtime);
  summary.litShots = shots.filter((s) => s.some((v) => v > 8)).length;
  engines.forEach(({ eng }) => eng.free());
  e.lx_comp_free(ch);

  const cw = N * scale, gap = 2;
  const SW = shots.length * (cw + gap) - gap;
  const sheet = Buffer.alloc(SW * cw * 3, 40); // grey gutters between frames
  shots.forEach((fr, k) => {
    const up = Buffer.from(upscale(N, N, fr, scale, scale).rgb);
    const ox = k * (cw + gap);
    for (let y = 0; y < cw; y++) up.copy(sheet, (y * SW + ox) * 3, y * cw * 3, (y + 1) * cw * 3);
  });
  fs.writeFileSync(path.join(out, `${slug}.png`), encodePNG(SW, cw, sheet));
  return summary;
}

for (const file of files) {
  const slug = slugOf(file);
  const s = renderScene(file);
  fs.writeFileSync(path.join(out, `${slug}.json`), JSON.stringify(s));
  console.log(
    `${slug}: ${s.layers?.length ?? 0} layers, ${s.periodMs ? `loop ${s.periodMs} ms` : "no loop"}, ${s.msPerFrame ?? "-"} ms/frame, lit ${s.litShots ?? 0}/${nFrames}` +
      (s.errors.length ? ` ERR ${s.errors.join("; ")}` : ""),
  );
}
