#!/usr/bin/env node
// Writes web/tests/fixtures/ — the image fixtures BOTH sprite-import pipelines
// are pinned to (Gitea #784).
//
//   node --experimental-strip-types web/tools/gen-sprite-fixtures.mjs
//
// There are two implementations of one wire-visible result: `web/src/lib/
// imageImport.ts` (the browser) and `crates/luxel-cli/src/spriteimport.rs`
// (`luxel sprite import`). Each fixture is therefore FOUR files:
//
//   <name>.png / .gif   the image itself — what the CLI and a browser decode
//   <name>.rgba         the same pixels as raw RGBA, frame after frame
//   <name>.json         size, frame delays and the import options to use
//   <name>.lxsp         the record BOTH pipelines must produce, byte for byte
//
// The `.rgba` sidecar is why the TS test needs no PNG decoder: it feeds the
// pure pipeline directly. And because the Rust side decodes the real `.png`
// instead, a golden that matches on both sides also proves the encoder below
// wrote the pixels the sidecar claims.
//
// The encoders here are deliberately minimal and deliberately readable:
//   * PNG — 8-bit RGBA, no interlacing, one zlib stream of STORED deflate
//     blocks (no compression at all), which is a legal PNG every decoder
//     reads and about thirty lines to write.
//   * GIF — GIF89a, one 128-entry global table, and the classic
//     "uncompressed LZW" trick: with a 7-bit minimum code size every code is
//     exactly 8 bits, so the code stream is plain bytes as long as a CLEAR is
//     re-emitted before the table could grow past 256 entries.
// Every fixture frame is FULLY OPAQUE in the GIF (transparency is exercised by
// the PNG, where alpha is unambiguous) so frame disposal cannot come into it.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { encodeSprite } from "../src/lib/sprite.ts";
import { importSprite, resolveOptions } from "../src/lib/imageImport.ts";

const here = dirname(fileURLToPath(import.meta.url));
const OUT = join(here, "..", "tests", "fixtures");

// ---- PNG ----

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function adler32(bytes) {
  let a = 1;
  let b = 0;
  for (const byte of bytes) {
    a = (a + byte) % 65521;
    b = (b + a) % 65521;
  }
  return ((b << 16) | a) >>> 0;
}

function be32(n) {
  return [n >>> 24, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff];
}

function chunk(type, data) {
  const t = [...type].map((c) => c.charCodeAt(0));
  const body = Uint8Array.from([...t, ...data]);
  return Uint8Array.from([...be32(data.length), ...body, ...be32(crc32(body))]);
}

/** A zlib stream of STORED deflate blocks — no compression, every decoder. */
function zlibStored(raw) {
  const out = [0x78, 0x01];
  const MAX = 65535;
  for (let at = 0; at < raw.length || at === 0; at += MAX) {
    const part = raw.subarray(at, Math.min(raw.length, at + MAX));
    const last = at + MAX >= raw.length ? 1 : 0;
    out.push(last, part.length & 0xff, part.length >>> 8, ~part.length & 0xff, (~part.length >>> 8) & 0xff);
    out.push(...part);
    if (part.length === 0) break;
  }
  out.push(...be32(adler32(raw)));
  return out;
}

/** `w`×`h` RGBA bytes as an 8-bit RGBA PNG. */
function png(w, h, rgba) {
  const raw = new Uint8Array(h * (1 + w * 4));
  for (let y = 0; y < h; y++) {
    raw[y * (1 + w * 4)] = 0; // filter: none
    raw.set(rgba.subarray(y * w * 4, (y + 1) * w * 4), y * (1 + w * 4) + 1);
  }
  return Uint8Array.from([
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
    ...chunk("IHDR", [...be32(w), ...be32(h), 8, 6, 0, 0, 0]),
    ...chunk("IDAT", zlibStored(raw)),
    ...chunk("IEND", []),
  ]);
}

// ---- GIF ----

function le16(n) {
  return [n & 0xff, (n >>> 8) & 0xff];
}

/** The code stream for one frame, as bytes. 7-bit minimum code size makes
 *  every code exactly 8 bits; a CLEAR every 96 codes keeps it that way. */
function gifCodes(indices) {
  const CLEAR = 128;
  const EOI = 129;
  const out = [CLEAR];
  let since = 0;
  for (const i of indices) {
    if (since >= 96) {
      out.push(CLEAR);
      since = 0;
    }
    out.push(i & 0x7f);
    since++;
  }
  out.push(EOI);
  return out;
}

function subBlocks(bytes) {
  const out = [];
  for (let at = 0; at < bytes.length; at += 255) {
    const part = bytes.slice(at, at + 255);
    out.push(part.length, ...part);
  }
  out.push(0);
  return out;
}

/**
 * An animated GIF89a. `palette` is up to 128 `[r,g,b]`; every frame is a
 * full-canvas array of palette indices; `delayMs` is per frame.
 */
function gif(w, h, palette, frames, delayMs) {
  const gct = [];
  for (let k = 0; k < 128; k++) {
    const c = palette[k] ?? [0, 0, 0];
    gct.push(c[0], c[1], c[2]);
  }
  const out = [
    ...[..."GIF89a"].map((c) => c.charCodeAt(0)),
    ...le16(w),
    ...le16(h),
    0xf6, // global table, 128 entries
    0,
    0,
    ...gct,
    // loop forever
    0x21, 0xff, 0x0b, ...[..."NETSCAPE2.0"].map((c) => c.charCodeAt(0)), 0x03, 0x01, ...le16(0), 0x00,
  ];
  for (const indices of frames) {
    out.push(
      0x21,
      0xf9,
      0x04,
      0x04, // disposal "do not dispose", no transparent index
      ...le16(Math.round(delayMs / 10)),
      0,
      0,
      0x2c,
      ...le16(0),
      ...le16(0),
      ...le16(w),
      ...le16(h),
      0x00,
      7, // LZW minimum code size
      ...subBlocks(gifCodes(indices)),
    );
  }
  out.push(0x3b);
  return Uint8Array.from(out);
}

// ---- the fixtures ----

/** A 12×8 hue ramp with a diagonal alpha wedge — small, and every texel a
 *  different colour, so the palette cap and the alpha threshold both bite. */
function ramp() {
  const w = 12;
  const h = 8;
  const rgba = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const at = (y * w + x) * 4;
      rgba[at] = Math.floor((x * 255) / (w - 1));
      rgba[at + 1] = Math.floor((y * 255) / (h - 1));
      rgba[at + 2] = Math.floor(((x + y) * 255) / (w + h - 2));
      // the wedge: the top-left corner fades out entirely
      rgba[at + 3] = x + y < 4 ? (x + y) * 40 : 255;
    }
  }
  return { w, h, frames: [{ rgba, delayMs: 0 }] };
}

/** A 41×27 two-blob gradient — odd dimensions on purpose (the fit maths must
 *  not be exercised only on multiples) and far more than 16 colours. */
function blob() {
  const w = 41;
  const h = 27;
  const rgba = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const at = (y * w + x) * 4;
      const d1 = Math.hypot(x - 12, y - 9);
      const d2 = Math.hypot(x - 30, y - 18);
      rgba[at] = Math.max(0, Math.min(255, Math.round(255 - d1 * 14)));
      rgba[at + 1] = Math.max(0, Math.min(255, Math.round(255 - d2 * 12)));
      rgba[at + 2] = Math.round((x * 120) / (w - 1) + (y * 80) / (h - 1));
      rgba[at + 3] = 255;
    }
  }
  return { w, h, frames: [{ rgba, delayMs: 0 }] };
}

/** Four 8×8 frames: a four-colour block walking around the ring. Opaque
 *  everywhere, so nothing depends on GIF frame disposal. */
function spin() {
  const w = 8;
  const h = 8;
  const palette = [
    [8, 8, 12],
    [232, 163, 61],
    [59, 125, 221],
    [230, 230, 230],
  ];
  const spots = [
    [1, 1],
    [6, 1],
    [6, 6],
    [1, 6],
  ];
  const indexFrames = spots.map(([cx, cy]) => {
    const idx = new Uint8Array(w * h);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const near = Math.abs(x - cx) <= 1 && Math.abs(y - cy) <= 1;
        idx[y * w + x] = near ? 1 : (x + y) % 2 === 0 ? 0 : 2;
      }
    }
    idx[0] = 3;
    return idx;
  });
  const frames = indexFrames.map((idx) => {
    const rgba = new Uint8Array(w * h * 4);
    for (let i = 0; i < idx.length; i++) {
      const c = palette[idx[i]];
      rgba[i * 4] = c[0];
      rgba[i * 4 + 1] = c[1];
      rgba[i * 4 + 2] = c[2];
      rgba[i * 4 + 3] = 255;
    }
    return { rgba, delayMs: 100 };
  });
  return { w, h, frames, palette, indexFrames };
}

/** Every fixture: the image, the raw sidecar, the options and the golden. */
const FIXTURES = [
  {
    name: "ramp",
    file: "ramp.png",
    src: ramp(),
    kind: "png",
    // downscale a 12×8 by area averaging into 8×6, half the palette
    opts: { name: "ramp", w: 8, h: 6, fit: "fit", resample: "area", colors: 24, alphaThreshold: 128, dither: false, keepEvery: 1, fpsOverride: null },
  },
  {
    name: "blob",
    file: "blob.png",
    src: blob(),
    kind: "png",
    // the median cut doing real work: thousands of colours down to 16, and a
    // CROP so the fit maths is exercised on odd dimensions
    opts: { name: "blob", w: 24, h: 24, fit: "crop", resample: "area", colors: 16, alphaThreshold: 128, dither: false, keepEvery: 1, fpsOverride: null },
  },
  {
    name: "spin",
    file: "spin.gif",
    src: spin(),
    kind: "gif",
    // nearest neighbour at 1:1 on an animation: the record must come out
    // exactly four colours and exactly four frames, at the GIF's own 10 fps
    opts: { name: "spin", w: 8, h: 8, fit: "fit", resample: "nearest", colors: 255, alphaThreshold: 128, dither: false, keepEvery: 1, fpsOverride: null },
  },
  {
    name: "spin-half",
    file: "spin.gif",
    src: spin(),
    kind: "skip",
    // the frame knob: every 2nd frame, so 2 frames at half the rate
    opts: { name: "spin-half", w: 8, h: 8, fit: "fit", resample: "nearest", colors: 4, alphaThreshold: 128, dither: false, keepEvery: 2, fpsOverride: null },
  },
];

mkdirSync(OUT, { recursive: true });
const wrote = [];

for (const fx of FIXTURES) {
  const { src, opts } = fx;
  if (fx.kind === "png") {
    writeFileSync(join(OUT, fx.file), png(src.w, src.h, src.frames[0].rgba));
    wrote.push(fx.file);
  } else if (fx.kind === "gif") {
    writeFileSync(join(OUT, fx.file), gif(src.w, src.h, src.palette, src.indexFrames, 100));
    wrote.push(fx.file);
  }
  // the raw sidecar: every frame's RGBA, back to back
  const raw = new Uint8Array(src.frames.length * src.w * src.h * 4);
  src.frames.forEach((f, i) => raw.set(f.rgba, i * src.w * src.h * 4));
  writeFileSync(join(OUT, `${fx.name}.rgba`), raw);
  writeFileSync(
    join(OUT, `${fx.name}.json`),
    `${JSON.stringify(
      {
        image: fx.file,
        w: src.w,
        h: src.h,
        frames: src.frames.length,
        delaysMs: src.frames.map((f) => f.delayMs),
        options: resolveOptions(opts),
      },
      null,
      2,
    )}\n`,
  );
  const result = importSprite({ w: src.w, h: src.h, frames: src.frames }, opts);
  writeFileSync(join(OUT, `${fx.name}.lxsp`), encodeSprite(result.sprite));
  wrote.push(`${fx.name}.rgba`, `${fx.name}.json`, `${fx.name}.lxsp`);
  console.log(
    `${fx.name}: ${result.sprite.w}x${result.sprite.h} x${result.sprite.frames} @ ${result.sprite.fps} fps, ` +
      `${result.sprite.palette.length} colours of ${result.colorsSeen} seen, ${result.bytes} B`,
  );
}

console.log(`wrote ${wrote.length} files into web/tests/fixtures/`);
