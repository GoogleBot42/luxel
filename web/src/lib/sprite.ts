// The sprite record, in TypeScript — a faithful mirror of
// `luxel_core::sprite` (crates/luxel-core/src/sprite.rs, contract §1 of
// Gitea #740).
//
// A sprite is a FIRST-CLASS store record now, not a sprite-tagged pattern:
// `LXSP` v1, a palette-indexed pixel image with frames. ONE byte layout is
// the device's flash bytes, the `GET/POST /api/sprites…` bodies, the
// playground's localStorage bytes (base64) and the wasm compositor's input —
// there is no second representation, so this module is a codec and nothing
// else.
//
//   off  size        field
//   0    4           magic  "LXSP"
//   4    1           version = 1
//   5    1           w        1..=64
//   6    1           h        1..=64
//   7    1           frames   1..=255
//   8    1           fps      0..=30    (0 = static)
//   9    1           colors   0..=255
//   10   1           name_len 1..=64    (UTF-8 bytes)
//   11   1           flags    0         (reserved; readers ignore)
//   12   name_len    name
//   +    3*colors    palette  [r,g,b] × colors, RGB888
//   +    w*h*frames  index    frame-major then row-major;
//                             0 = TRANSPARENT, k = palette[k-1]
//
// TRANSPARENCY IS INDEX 0, NOT BLACK. An opaque black texel is a palette
// colour like any other — the change from the old tagged-pattern format,
// where `v == 0` was the key. `spriteFromTaggedPattern` is the one-release
// migration reader that converts the old records, and it is the ONLY thing
// in here that knows HSV at all.
//
// Keep this file pure: no stores, no fetch, no Svelte. `web/tests/sprite.test.mjs`
// pins the bytes against the same hand-built record the Rust test builds, so
// the two codecs cannot drift.

import { hsvToRgb, type Hsv } from "./color.ts";

/** A palette entry: r, g, b, each 0..255. */
export type Rgb8 = [number, number, number];

/** A sprite, decoded. `index` is `w*h*frames` bytes, frame-major then
 *  row-major; 0 is transparent and `k` names `palette[k-1]`. */
export interface Sprite {
  name: string;
  w: number;
  h: number;
  frames: number;
  fps: number;
  palette: Rgb8[];
  index: Uint8Array;
}

export const SPRITE_MAGIC = "LXSP";
export const SPRITE_VERSION = 1;
/** Fixed header bytes before the name. */
export const SPRITE_HDR = 12;
export const SPRITE_MAX_EDGE = 64;
export const SPRITE_MAX_FPS = 30;
export const SPRITE_MAX_FRAMES = 255;
/** Palette entries — the index byte's 255 non-zero values (#741 item 14:
 *  the old 16 was an editor rule with no reason behind it). */
export const SPRITE_MAX_COLORS = 255;
export const SPRITE_MAX_NAME = 64;
/** The device's HTTP request buffer, so a bigger record could never arrive. */
export const SPRITE_MAX_BYTES = 16 * 1024;

/** THE palette-cap refusal, in one place (#741 item 14: the old limit was
 *  enforced by disabling a button while `fill` walked straight past it). */
export const SPRITE_COLORS_FULL = `sprite: ${SPRITE_MAX_COLORS} colours max`;

const O_VER = 4;
const O_W = 5;
const O_H = 6;
const O_FRAMES = 7;
const O_FPS = 8;
const O_COLORS = 9;
const O_NAME_LEN = 10;
const O_FLAGS = 11;

/** The four drawing tools (#741: four LARGE labelled buttons, keys 1-4). */
export type SpriteTool = "pencil" | "eraser" | "fill" | "pick";

/** Exact byte length a record of these dimensions has —
 *  `luxel_core::sprite::record_len`. */
export function recordLen(
  nameLen: number,
  colors: number,
  w: number,
  h: number,
  frames: number,
): number {
  return SPRITE_HDR + nameLen + 3 * colors + w * h * frames;
}

/** What `encodeSprite` would produce, in bytes — the editor's budget line. */
export function spriteBytes(s: Sprite): number {
  return recordLen(nameBytes(s.name).length, s.palette.length, s.w, s.h, s.frames);
}

/** Texels per frame. */
export function spriteTexels(s: Sprite): number {
  return s.w * s.h;
}

/** Index of (col, row) in `frame` — the record's own order, and the same one
 *  `blit_sprite` walks. */
export function texelIndex(s: Sprite, frame: number, col: number, row: number): number {
  return frame * s.w * s.h + row * s.w + col;
}

// ---- encode / decode ----

/**
 * The record for `sprite`. The name is clamped to 1..=64 UTF-8 bytes (an
 * empty one becomes `Sprite`) so what comes out of here always passes
 * [`checkSprite`] — a console that could have prevented a refusal and did
 * not is the console's bug.
 */
export function encodeSprite(s: Sprite): Uint8Array {
  const name = nameBytes(s.name);
  const w = clampInt(s.w, 1, SPRITE_MAX_EDGE);
  const h = clampInt(s.h, 1, SPRITE_MAX_EDGE);
  const frames = clampInt(s.frames, 1, SPRITE_MAX_FRAMES);
  const colors = Math.min(s.palette.length, SPRITE_MAX_COLORS);
  const out = new Uint8Array(recordLen(name.length, colors, w, h, frames));
  out[0] = 0x4c; // L
  out[1] = 0x58; // X
  out[2] = 0x53; // S
  out[3] = 0x50; // P
  out[O_VER] = SPRITE_VERSION;
  out[O_W] = w;
  out[O_H] = h;
  out[O_FRAMES] = frames;
  out[O_FPS] = clampInt(s.fps, 0, SPRITE_MAX_FPS);
  out[O_COLORS] = colors;
  out[O_NAME_LEN] = name.length;
  out[O_FLAGS] = 0;
  out.set(name, SPRITE_HDR);
  let at = SPRITE_HDR + name.length;
  for (let k = 0; k < colors; k++) {
    const c = s.palette[k] ?? [0, 0, 0];
    out[at++] = clampInt(c[0], 0, 255);
    out[at++] = clampInt(c[1], 0, 255);
    out[at++] = clampInt(c[2], 0, 255);
  }
  const texels = w * h * frames;
  for (let i = 0; i < texels; i++) {
    const k = s.index[i] ?? 0;
    out[at++] = k > colors ? 0 : k;
  }
  return out;
}

/**
 * Why a record is bad, as the user-facing sentence the routes answer with —
 * every message starts `sprite: `, and every check below is one of
 * `luxel_core::sprite::check`'s, in its order. `decodeSprite` is the fast
 * yes/no; this is the diagnosis.
 */
export function checkSprite(bytes: Uint8Array): string | null {
  if (bytes.length < SPRITE_HDR + 1) return "sprite: record is too short";
  if (bytes[0] !== 0x4c || bytes[1] !== 0x58 || bytes[2] !== 0x53 || bytes[3] !== 0x50) {
    return "sprite: bad magic";
  }
  if (bytes[O_VER] !== SPRITE_VERSION) return "sprite: unknown record version";
  if (bytes.length > SPRITE_MAX_BYTES) return "sprite: over the 16 KiB cap";
  const w = bytes[O_W] ?? 0;
  const h = bytes[O_H] ?? 0;
  const frames = bytes[O_FRAMES] ?? 0;
  if (w === 0 || h === 0 || w > SPRITE_MAX_EDGE || h > SPRITE_MAX_EDGE) {
    return "sprite: 1..64 texels on each edge";
  }
  if (frames === 0) return "sprite: at least one frame";
  if ((bytes[O_FPS] ?? 0) > SPRITE_MAX_FPS) return "sprite: fps 0..30";
  const colors = bytes[O_COLORS] ?? 0;
  const nameLen = bytes[O_NAME_LEN] ?? 0;
  if (nameLen === 0 || nameLen > SPRITE_MAX_NAME) return "sprite: name must be 1..=64 bytes";
  if (bytes.length !== recordLen(nameLen, colors, w, h, frames)) {
    return "sprite: length does not match its header";
  }
  if (decodeUtf8(bytes.subarray(SPRITE_HDR, SPRITE_HDR + nameLen)) === null) {
    return "sprite: name is not utf-8";
  }
  const idxAt = SPRITE_HDR + nameLen + 3 * colors;
  for (let i = idxAt; i < bytes.length; i++) {
    if ((bytes[i] ?? 0) > colors) return "sprite: index out of palette";
  }
  return null;
}

/** The sprite a record carries, or null for anything malformed (the reason
 *  is [`checkSprite`]'s). */
export function decodeSprite(bytes: Uint8Array): Sprite | null {
  if (checkSprite(bytes) !== null) return null;
  const nameLen = bytes[O_NAME_LEN] ?? 0;
  const colors = bytes[O_COLORS] ?? 0;
  const name = decodeUtf8(bytes.subarray(SPRITE_HDR, SPRITE_HDR + nameLen));
  if (name === null) return null;
  const palAt = SPRITE_HDR + nameLen;
  const idxAt = palAt + 3 * colors;
  const palette: Rgb8[] = [];
  for (let k = 0; k < colors; k++) {
    palette.push([bytes[palAt + k * 3] ?? 0, bytes[palAt + k * 3 + 1] ?? 0, bytes[palAt + k * 3 + 2] ?? 0]);
  }
  return {
    name,
    w: bytes[O_W] ?? 0,
    h: bytes[O_H] ?? 0,
    frames: bytes[O_FRAMES] ?? 0,
    fps: bytes[O_FPS] ?? 0,
    palette,
    // A COPY, not a view: the caller edits this and the bytes it came from
    // may be a slice of a much larger buffer (a fetch response).
    index: new Uint8Array(bytes.subarray(idxAt)),
  };
}

// ---- making and editing ----

/** A brand-new, fully transparent sprite with an empty palette. */
export function newSprite(name: string, w: number, h: number): Sprite {
  const cw = clampInt(w, 1, SPRITE_MAX_EDGE);
  const ch = clampInt(h, 1, SPRITE_MAX_EDGE);
  return {
    name: name === "" ? "Sprite" : name,
    w: cw,
    h: ch,
    frames: 1,
    fps: 0,
    palette: [],
    index: new Uint8Array(cw * ch),
  };
}

/** What [`colorIndex`] answers: the sprite that holds the colour (the same
 *  one when it already did) and the index to paint with. */
export interface ColorSlot {
  sprite: Sprite;
  k: number;
}

/**
 * THE one enforcement point for the palette cap (#741 item 14).
 *
 * Returns the existing index for a colour the sprite already uses, or a
 * sprite with one more palette entry and the new index. At 255 colours it
 * returns [`SPRITE_COLORS_FULL`] instead — one place, one sentence, and
 * every writer (pencil, fill, the picker) goes through it, which is exactly
 * what `paletteWouldOverflow` failed to be: it was never called.
 */
export function colorIndex(s: Sprite, rgb: Rgb8): ColorSlot | string {
  const want: Rgb8 = [clampInt(rgb[0], 0, 255), clampInt(rgb[1], 0, 255), clampInt(rgb[2], 0, 255)];
  for (let k = 0; k < s.palette.length; k++) {
    if (sameRgb(s.palette[k] ?? [0, 0, 0], want)) return { sprite: s, k: k + 1 };
  }
  if (s.palette.length >= SPRITE_MAX_COLORS) return SPRITE_COLORS_FULL;
  return { sprite: { ...s, palette: [...s.palette, want] }, k: s.palette.length + 1 };
}

/** Paint one texel with palette index `k` (0 erases). Returns a NEW sprite —
 *  records are replaced, never mutated, so Svelte sees the change. */
export function paint(s: Sprite, frame: number, col: number, row: number, k: number): Sprite {
  if (frame < 0 || frame >= s.frames) return s;
  if (col < 0 || row < 0 || col >= s.w || row >= s.h) return s;
  const i = texelIndex(s, frame, col, row);
  if ((s.index[i] ?? 0) === k) return s;
  const index = new Uint8Array(s.index);
  index[i] = k;
  return { ...s, index };
}

/** Flood fill from (col, row) within ONE frame, four-connected, over every
 *  texel carrying the seed's index. */
export function fill(s: Sprite, frame: number, col: number, row: number, k: number): Sprite {
  if (frame < 0 || frame >= s.frames) return s;
  if (col < 0 || row < 0 || col >= s.w || row >= s.h) return s;
  const seed = s.index[texelIndex(s, frame, col, row)] ?? 0;
  if (seed === k) return s;
  const index = new Uint8Array(s.index);
  const stack: number[] = [col, row];
  while (stack.length > 0) {
    const r = stack.pop() ?? 0;
    const c = stack.pop() ?? 0;
    if (c < 0 || r < 0 || c >= s.w || r >= s.h) continue;
    const i = texelIndex(s, frame, c, r);
    if ((index[i] ?? 0) !== seed) continue;
    index[i] = k;
    stack.push(c + 1, r, c - 1, r, c, r + 1, c, r - 1);
  }
  return { ...s, index };
}

/** Grow or shrink, keeping the texels that still fit in every frame. */
export function resize(s: Sprite, w: number, h: number): Sprite {
  const nw = clampInt(w, 1, SPRITE_MAX_EDGE);
  const nh = clampInt(h, 1, SPRITE_MAX_EDGE);
  if (nw === s.w && nh === s.h) return s;
  const next: Sprite = { ...s, w: nw, h: nh, index: new Uint8Array(nw * nh * s.frames) };
  const keepW = Math.min(s.w, nw);
  const keepH = Math.min(s.h, nh);
  for (let f = 0; f < s.frames; f++) {
    for (let row = 0; row < keepH; row++) {
      for (let col = 0; col < keepW; col++) {
        next.index[texelIndex(next, f, col, row)] = s.index[texelIndex(s, f, col, row)] ?? 0;
      }
    }
  }
  return next;
}

/** Insert a copy of `frame` directly after it — the frame strip's `+` and its
 *  `Duplicate` are the same gesture on the same frame. */
export function duplicateFrame(s: Sprite, frame: number): Sprite {
  if (s.frames >= SPRITE_MAX_FRAMES) return s;
  const at = clampInt(frame, 0, s.frames - 1);
  const n = s.w * s.h;
  const index = new Uint8Array(n * (s.frames + 1));
  index.set(s.index.subarray(0, n * (at + 1)), 0);
  index.set(s.index.subarray(n * at, n * (at + 1)), n * (at + 1));
  index.set(s.index.subarray(n * (at + 1)), n * (at + 2));
  return { ...s, frames: s.frames + 1, index };
}

/** The strip's `+`: a copy of the frame you are on, after it. */
export function addFrame(s: Sprite, frame: number): Sprite {
  return duplicateFrame(s, frame);
}

/** Drop a frame. A sprite always has at least one, so the last is kept. */
export function deleteFrame(s: Sprite, frame: number): Sprite {
  if (s.frames <= 1) return s;
  if (frame < 0 || frame >= s.frames) return s;
  const n = s.w * s.h;
  const index = new Uint8Array(n * (s.frames - 1));
  index.set(s.index.subarray(0, n * frame), 0);
  index.set(s.index.subarray(n * (frame + 1), n * s.frames), n * frame);
  return { ...s, frames: s.frames - 1, index };
}

/** Move a frame to another position (the strip's ◂ / ▸). */
export function moveFrame(s: Sprite, from: number, to: number): Sprite {
  if (from < 0 || from >= s.frames) return s;
  const dest = clampInt(to, 0, s.frames - 1);
  if (dest === from) return s;
  const n = s.w * s.h;
  const order: number[] = [];
  for (let f = 0; f < s.frames; f++) if (f !== from) order.push(f);
  order.splice(dest, 0, from);
  const index = new Uint8Array(s.index.length);
  order.forEach((src, f) => {
    index.set(s.index.subarray(n * src, n * (src + 1)), n * f);
  });
  return { ...s, index };
}

/** A colour the drawing is actually made of, and the index that paints it. */
export interface UsedColor {
  k: number;
  rgb: Rgb8;
}

/**
 * The colours in USE, in first-appearance order — the editor's "In use"
 * grid, and stable, so repainting one texel never reshuffles it.
 *
 * #741: this REPLACES the recents strip. "Just drag the color slider and it
 * becomes filled with intermediate colors you didn't want" — a colour enters
 * this grid only by being painted, because the grid is read off the record
 * rather than off the picker's event stream.
 */
export function usedColors(s: Sprite): UsedColor[] {
  const seen = new Set<number>();
  const out: UsedColor[] = [];
  for (let i = 0; i < s.index.length; i++) {
    const k = s.index[i] ?? 0;
    if (k === 0 || seen.has(k)) continue;
    const rgb = s.palette[k - 1];
    if (!rgb) continue;
    seen.add(k);
    out.push({ k, rgb });
  }
  return out;
}

/**
 * Drop palette entries nothing paints with, renumbering the index — run on
 * SAVE, so the 255-colour cap is about colours in use rather than about
 * every colour the session ever touched.
 */
export function compactPalette(s: Sprite): Sprite {
  const used = usedColors(s);
  if (used.length === s.palette.length) return s;
  const remap = new Uint8Array(256);
  used.forEach((u, at) => {
    remap[u.k] = at + 1;
  });
  const index = new Uint8Array(s.index.length);
  for (let i = 0; i < s.index.length; i++) index[i] = remap[s.index[i] ?? 0] ?? 0;
  return { ...s, palette: used.map((u) => u.rgb), index };
}

/** The frame shown `elapsedMs` into playback — `SpriteView::frame_at`, so
 *  the playground's thumbnails cycle exactly as the device's do. */
export function frameAt(s: Sprite, elapsedMs: number): number {
  if (s.frames <= 1 || s.fps === 0) return 0;
  const ms = Math.max(0, Math.floor(elapsedMs));
  return Math.floor((ms * s.fps) / 1000) % s.frames;
}

/** One frame as RGBA bytes, for `ctx.putImageData` on a `w`×`h` canvas.
 *  A transparent texel is alpha 0 — the checkerboard shows through. */
export function renderFrame(s: Sprite, frame: number): Uint8ClampedArray {
  const out = new Uint8ClampedArray(s.w * s.h * 4);
  const base = clampInt(frame, 0, Math.max(0, s.frames - 1)) * s.w * s.h;
  for (let i = 0; i < s.w * s.h; i++) {
    const k = s.index[base + i] ?? 0;
    if (k === 0) continue;
    const c = s.palette[k - 1];
    if (!c) continue;
    out[i * 4] = c[0];
    out[i * 4 + 1] = c[1];
    out[i * 4 + 2] = c[2];
    out[i * 4 + 3] = 255;
  }
  return out;
}

// ---- the one-release migration reader (#740 step 3) ----

/** Is this pattern source a sprite-tagged PATTERN — the format sprites used
 *  before they were records? The Patterns library filters these out even
 *  before the migration runs, so a sprite never looks like a playable
 *  pattern. */
export function isTaggedSpriteSource(source: string): boolean {
  return /^\s*\/\/\s*@sprite\b/.test(source.split("\n", 1)[0] ?? "");
}

/**
 * Read a `// @sprite w= h= frames= fps=` pattern into a record — the ONE
 * direction the old format is still supported in (#740 step 3: "keep the old
 * tag readable for one release").
 *
 * The old format stored three HSV float arrays and keyed transparency on
 * `v == 0`; this quantises each texel through `hsvToRgb` to RGB888 and
 * assigns palette entries in first-appearance order, so a texel that was
 * transparent becomes index 0 and an opaque black one becomes a palette
 * colour. The name comes from the tag line's `// <name>` follower, which is
 * what the old emitter wrote, and falls back to `Sprite`.
 */
export function spriteFromTaggedPattern(source: string): Sprite | null {
  const lines = source.split("\n");
  const first = lines[0] ?? "";
  const m = /^\s*\/\/\s*@sprite\b(.*)$/.exec(first);
  if (!m) return null;
  const kv = new Map<string, number>();
  for (const part of (m[1] ?? "").split(/\s+/)) {
    const at = part.indexOf("=");
    if (at <= 0) continue;
    const v = Number.parseInt(part.slice(at + 1), 10);
    if (Number.isFinite(v)) kv.set(part.slice(0, at), v);
  }
  const w = kv.get("w") ?? 0;
  const h = kv.get("h") ?? 0;
  const frames = kv.get("frames") ?? 1;
  const fps = kv.get("fps") ?? 0;
  if (w < 1 || h < 1 || w > SPRITE_MAX_EDGE || h > SPRITE_MAX_EDGE) return null;
  if (frames < 1 || frames > SPRITE_MAX_FRAMES) return null;
  if (fps < 0 || fps > SPRITE_MAX_FPS) return null;

  const hs = readArray(source, "sprH");
  const ss = readArray(source, "sprS");
  const vs = readArray(source, "sprV");
  if (!hs || !ss || !vs) return null;
  const n = w * h * frames;
  if (hs.length !== n || ss.length !== n || vs.length !== n) return null;

  const second = (lines[1] ?? "").trim();
  const nm = /^\/\/\s*(.+)$/.exec(second);
  const name = nm ? (nm[1] ?? "").trim() : "";

  const palette: Rgb8[] = [];
  const keyed = new Map<string, number>();
  const index = new Uint8Array(n);
  for (let i = 0; i < n; i++) {
    const v = vs[i] ?? 0;
    if (v === 0) continue; // the old format's transparency key
    const hsv: Hsv = [hs[i] ?? 0, ss[i] ?? 0, v];
    const rgb = hsvToRgb(hsv);
    const q: Rgb8 = [
      clampInt(Math.round((rgb[0] ?? 0) * 255), 0, 255),
      clampInt(Math.round((rgb[1] ?? 0) * 255), 0, 255),
      clampInt(Math.round((rgb[2] ?? 0) * 255), 0, 255),
    ];
    const key = `${q[0]},${q[1]},${q[2]}`;
    let k = keyed.get(key);
    if (k === undefined) {
      if (palette.length >= SPRITE_MAX_COLORS) continue; // over the cap: transparent
      palette.push(q);
      k = palette.length;
      keyed.set(key, k);
    }
    index[i] = k;
  }
  return { name: name === "" ? "Sprite" : name, w, h, frames, fps, palette, index };
}

/** One top-level `var <name> = [ … ]` numeric literal, or null. A scan and
 *  not a parse, exactly as the old reader was: anything else reads as "not a
 *  sprite this migration can convert". */
function readArray(source: string, name: string): number[] | null {
  const re = new RegExp(String.raw`(^|\n)\s*var\s+${name}\s*=\s*\[([^\]]*)\]`);
  const m = re.exec(source);
  if (!m) return null;
  const body = (m[2] ?? "").trim();
  if (body === "") return [];
  const out: number[] = [];
  for (const piece of body.split(",")) {
    const t = piece.trim();
    if (t === "") continue;
    const v = Number(t);
    if (!Number.isFinite(v)) return null;
    out.push(v);
  }
  return out;
}

// ---- small shared helpers ----

export function sameRgb(a: Rgb8, b: Rgb8): boolean {
  return a[0] === b[0] && a[1] === b[1] && a[2] === b[2];
}

/** `rrggbb`, no `#` — the spelling `SceneSwatch` speaks. */
export function rgb8ToHex(rgb: Rgb8): string {
  return rgb.map((c) => clampInt(c, 0, 255).toString(16).padStart(2, "0")).join("");
}

/** `rrggbb` or `#rrggbb` back to bytes, or null. */
export function hexToRgb8(hex: string): Rgb8 | null {
  const s = hex.replace(/^#/, "");
  if (!/^[0-9a-fA-F]{6}$/.test(s)) return null;
  return [
    Number.parseInt(s.slice(0, 2), 16),
    Number.parseInt(s.slice(2, 4), 16),
    Number.parseInt(s.slice(4, 6), 16),
  ];
}

export function rgbCss(rgb: Rgb8): string {
  return `rgb(${clampInt(rgb[0], 0, 255)},${clampInt(rgb[1], 0, 255)},${clampInt(rgb[2], 0, 255)})`;
}

/** `9×8 · 2 frames` — the tile's and the layer row's meta line. */
export function spriteMetaLine(w: number, h: number, frames: number): string {
  return `${w}×${h} · ${frames} frame${frames === 1 ? "" : "s"}`;
}

/** The name as 1..=64 UTF-8 bytes, never split mid-code-point. */
function nameBytes(name: string): Uint8Array {
  const enc = new TextEncoder();
  let s = name.trim();
  if (s === "") s = "Sprite";
  let bytes = enc.encode(s);
  while (bytes.length > SPRITE_MAX_NAME) {
    s = s.slice(0, -1);
    if (s === "") return enc.encode("Sprite");
    bytes = enc.encode(s);
  }
  return bytes;
}

function decodeUtf8(bytes: Uint8Array): string | null {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

function clampInt(v: number, lo: number, hi: number): number {
  const n = Math.round(v);
  if (!Number.isFinite(n)) return lo;
  return n < lo ? lo : n > hi ? hi : n;
}
