// Image → `LXSP` sprite: the whole conversion, as pure functions (Gitea #784).
//
// Jeremy (2026-09-26): "the ability to import/convert images to sprites". A
// sprite could only be drawn texel by texel before this.
//
// THIS MODULE IS THE REFERENCE PIPELINE and it has a twin: the same fit, the
// same median cut and the same mapping are implemented in
// `crates/luxel-cli/src/spriteimport.rs` for `luxel sprite import`, and the
// two are pinned together on a checked-in fixture — `web/tests/fixtures/`
// holds `ramp.png` plus the `*.lxsp` records BOTH suites assert byte for byte
// (`web/tests/imageImport.test.mjs`, `crates/luxel-cli/tests/spriteimport.rs`).
// So every arithmetic decision in here is a wire-visible contract, not an
// implementation detail:
//
//   * rounding is ALWAYS `Math.floor(x + 0.5)`, never `Math.round` — Rust's
//     `f64::round` rounds halves away from zero and JS's rounds them up, so
//     the two disagree on negatives. `floor(x + 0.5)` is the same everywhere
//     and every value rounded here is non-negative anyway.
//   * every sum is written in the same order in both languages; nothing in
//     here reassociates, so IEEE-754 gives bit-identical results.
//   * ties are broken by INDEX (the bucket's position, the palette's), never
//     by a sort whose stability would have to be assumed.
//
// What this module deliberately does NOT do: decode. Decoding is browser API
// (`createImageBitmap`, `ImageDecoder`) and lives in `lib/imageDecode.ts`;
// what arrives here is RGBA bytes and frame delays, which is also what a unit
// test can build by hand and what the `image` crate hands the CLI.
//
// Keep it pure: no DOM, no stores, no Svelte.

import {
  compactPalette,
  SPRITE_MAX_BYTES,
  SPRITE_MAX_COLORS,
  SPRITE_MAX_EDGE,
  SPRITE_MAX_FPS,
  SPRITE_MAX_FRAMES,
  SPRITE_MAX_NAME,
  spriteBytes,
  type Rgb8,
  type Sprite,
} from "./sprite.ts";

/** One decoded source frame: `w*h*4` RGBA bytes and how long it is shown. */
export interface SourceFrame {
  rgba: Uint8Array;
  /** Frame delay in milliseconds, as the container states it (0 = unstated). */
  delayMs: number;
}

/** A decoded image: its natural size and one or more frames. */
export interface SourceImage {
  w: number;
  h: number;
  frames: SourceFrame[];
}

/**
 * How the source is placed in the target box — CSS `object-fit`'s three
 * useful words, spelled the way the import dialog labels them:
 *   * `fit`  — the whole image, proportions kept, transparent letterbox.
 *   * `fill` — stretched to the box exactly, proportions ignored.
 *   * `crop` — proportions kept, scaled to cover, the overflow cut off.
 */
export type FitMode = "fit" | "fill" | "crop";

/** Nearest neighbour keeps pixel art crisp; area average is right for photos. */
export type Resample = "nearest" | "area";

/** Alpha at or above this is opaque; below it the texel is index 0. */
export const DEFAULT_ALPHA_THRESHOLD = 128;

/** What a delay of "as fast as you like" is actually shown at. GIF writes its
 *  delays in hundredths and 0 or 1 means "no delay"; every browser and viewer
 *  shows those at 100 ms instead, so that is the rate to derive from. A delay
 *  ABOVE that is honoured as written — a 40 ms GIF really is 25 fps. */
export const SLOW_DELAY_MS = 100;
/** Delays up to this are the "no delay" case. */
export const NO_DELAY_MS = 10;

/** Every knob the conversion has. `keepEvery` and `colors` are the two the
 *  cap readout offers (#784: "never a silent truncation"). */
export interface ImportOptions {
  name: string;
  /** Target texels. 1..=64 each, clamped. */
  w: number;
  h: number;
  fit: FitMode;
  resample: Resample;
  /** Palette ceiling, 1..=255. */
  colors: number;
  /** 1..=255. */
  alphaThreshold: number;
  /** Error diffusion. OFF by default — it looks bad on LEDs. */
  dither: boolean;
  /** Keep source frame `i` when `i % keepEvery === 0`. 1 = every frame. */
  keepEvery: number;
  /** `null` = derive the rate from the frame delays. */
  fpsOverride: number | null;
}

/** What the conversion produced, and everything the readout says about it. */
export interface ImportResult {
  sprite: Sprite;
  /** `encodeSprite(sprite).length` — the record's real length. */
  bytes: number;
  /** Over [`SPRITE_MAX_BYTES`]: the import is refused, never truncated. */
  overCap: boolean;
  /** Source frames the `keepEvery` knob skipped. */
  skippedFrames: number;
  /** Kept frames past the format's 255 — reported, and the reason the UI
   *  asks for a bigger `keepEvery` rather than quietly dropping them. */
  droppedFrames: number;
  /** Distinct colours the record ended up with (after `compactPalette`). */
  colorsUsed: number;
  /** Distinct opaque colours the RESAMPLED source had, before quantizing. */
  colorsSeen: number;
  /** The record's `fps`. */
  fps: number;
}

/** A source frame's delay, normalised: "no delay" becomes 100 ms and anything
 *  longer is taken at its word. */
export function normalizedDelay(delayMs: number): number {
  const d = Math.floor(delayMs);
  if (!Number.isFinite(d) || d <= NO_DELAY_MS) return SLOW_DELAY_MS;
  return d;
}

/**
 * The target size an import starts at: the long edge scaled to `cap`, which
 * is 64 unless the fixture it is being drawn FOR is smaller (#784 — "the long
 * edge at 64, or the current layout's panel size when it is ≤ 64"). An image
 * already small enough keeps its own size: upscaling pixel art on the way in
 * only costs record bytes.
 */
export function defaultTargetSize(
  sw: number,
  sh: number,
  panel?: { w: number; h: number },
): { w: number; h: number } {
  const edge =
    panel && panel.w > 0 && panel.h > 0 ? Math.max(panel.w, panel.h) : SPRITE_MAX_EDGE;
  const cap = clampInt(edge, 1, SPRITE_MAX_EDGE);
  const long = Math.max(1, Math.max(sw, sh));
  if (long <= cap) return { w: clampInt(sw, 1, cap), h: clampInt(sh, 1, cap) };
  const s = cap / long;
  return {
    w: clampInt(Math.floor(sw * s + 0.5), 1, cap),
    h: clampInt(Math.floor(sh * s + 0.5), 1, cap),
  };
}

/**
 * Where an import starts before anyone touches a knob: the default size, the
 * whole image visible, area averaging for something photographic and nearest
 * neighbour for something that is already pixels, the format's full palette,
 * and no dithering.
 *
 * The resampler's default is a GUESS with a cheap tell: an image whose long
 * edge is at most 64 is already texel-sized art, so nearest keeps it exact.
 */
export function defaultImportOptions(
  src: SourceImage,
  name: string,
  panel?: { w: number; h: number },
): ImportOptions {
  const size = defaultTargetSize(src.w, src.h, panel);
  const pixelArt = Math.max(src.w, src.h) <= SPRITE_MAX_EDGE;
  return {
    name: spriteName(name),
    w: size.w,
    h: size.h,
    fit: "fit",
    resample: pixelArt ? "nearest" : "area",
    colors: SPRITE_MAX_COLORS,
    alphaThreshold: DEFAULT_ALPHA_THRESHOLD,
    dither: false,
    keepEvery: 1,
    fpsOverride: null,
  };
}

/** A file name as a sprite name: no extension, no path, 1..=64 bytes. */
export function spriteName(fileName: string): string {
  const base = (fileName.split(/[\\/]/).pop() ?? "").replace(/\.[^.]+$/, "");
  const clean = base.replace(/[\r\n\t]+/g, " ").trim();
  if (clean === "") return "Sprite";
  const enc = new TextEncoder();
  let s = clean;
  while (enc.encode(s).length > SPRITE_MAX_NAME) {
    s = s.slice(0, -1);
    if (s === "") return "Sprite";
  }
  return s;
}

/** The source frame indices `keepEvery` keeps, before the 255-frame cap. */
export function keptIndices(frameCount: number, keepEvery: number): number[] {
  const every = Math.max(1, Math.floor(keepEvery));
  const out: number[] = [];
  for (let i = 0; i < frameCount; i += every) out.push(i);
  return out;
}

/**
 * THE conversion. Every knob resolved, every byte decided — and nothing here
 * refuses: an over-cap result comes back with `overCap` true and its real
 * byte count, so the caller can say what to turn down (#784 item 4).
 */
export function importSprite(src: SourceImage, opts: ImportOptions): ImportResult {
  const o = resolveOptions(opts);
  const sw = Math.max(1, Math.floor(src.w));
  const sh = Math.max(1, Math.floor(src.h));

  const kept = keptIndices(src.frames.length, o.keepEvery);
  const droppedFrames = Math.max(0, kept.length - SPRITE_MAX_FRAMES);
  const use = kept.slice(0, SPRITE_MAX_FRAMES);
  const skippedFrames = Math.max(0, src.frames.length - kept.length);

  // 1. FIT — every kept frame resampled into the target box. Alpha is already
  //    a yes/no here: `a[i] = 0` is the transparent texel, everything else is
  //    opaque and carries an RGB.
  const n = o.w * o.h;
  const rgb = new Uint8Array(Math.max(1, n * Math.max(1, use.length) * 3));
  const on = new Uint8Array(Math.max(1, n * Math.max(1, use.length)));
  const rects = placement(sw, sh, o.w, o.h, o.fit);
  use.forEach((si, f) => {
    const frame = src.frames[si];
    resampleFrame(
      frame ? frame.rgba : new Uint8Array(sw * sh * 4),
      sw,
      sh,
      o,
      rects,
      rgb,
      on,
      f * n,
    );
  });

  // 2. HISTOGRAM of the opaque texels, in first-appearance order — the order
  //    is load-bearing: it is what makes the median cut and the mapping
  //    reproducible in two languages.
  const hist = histogram(rgb, on, n * use.length);

  // 3. QUANTIZE. At or under the ceiling the colours are kept EXACTLY, which
  //    is what makes importing pixel art lossless.
  const palette = hist.count.length <= o.colors ? histColors(hist) : medianCut(hist, o.colors);

  // 4. MAP every texel to a palette index (0 = transparent).
  const frames = Math.max(1, use.length);
  const index = o.dither
    ? mapDithered(rgb, on, o.w, o.h, frames, palette)
    : mapNearest(rgb, on, n * frames, palette);

  const sprite = compactPalette({
    name: o.name,
    w: o.w,
    h: o.h,
    frames,
    fps: framesFps(src, o, use.length),
    palette,
    index,
  });
  const bytes = spriteBytes(sprite);
  return {
    sprite,
    bytes,
    overCap: bytes > SPRITE_MAX_BYTES,
    skippedFrames,
    droppedFrames,
    colorsUsed: sprite.palette.length,
    colorsSeen: hist.count.length,
    fps: sprite.fps,
  };
}

/** Every option clamped to what the record can hold. */
export function resolveOptions(o: ImportOptions): ImportOptions {
  return {
    name: spriteName(o.name),
    w: clampInt(o.w, 1, SPRITE_MAX_EDGE),
    h: clampInt(o.h, 1, SPRITE_MAX_EDGE),
    fit: o.fit,
    resample: o.resample,
    colors: clampInt(o.colors, 1, SPRITE_MAX_COLORS),
    alphaThreshold: clampInt(o.alphaThreshold, 1, 255),
    dither: o.dither,
    keepEvery: Math.max(1, Math.floor(o.keepEvery)),
    fpsOverride: o.fpsOverride === null ? null : clampInt(o.fpsOverride, 0, SPRITE_MAX_FPS),
  };
}

/** The record's `fps`: the override, or the kept frames' own rate. */
function framesFps(src: SourceImage, o: ImportOptions, keptCount: number): number {
  if (o.fpsOverride !== null) return keptCount <= 1 ? 0 : o.fpsOverride;
  return derivedFps(src, o.keepEvery, keptCount);
}

/**
 * The rate the kept frames play at: the source's mean delay, multiplied by
 * `keepEvery` because keeping one frame in N makes each shown N times as
 * long, then capped at [`SPRITE_MAX_FPS`].
 */
export function derivedFps(src: SourceImage, keepEvery: number, keptCount: number): number {
  if (keptCount <= 1) return 0;
  let total = 0;
  for (const f of src.frames) total += normalizedDelay(f.delayMs);
  const mean = (total / src.frames.length) * Math.max(1, Math.floor(keepEvery));
  if (!(mean > 0)) return SPRITE_MAX_FPS;
  return clampInt(Math.floor(1000 / mean + 0.5), 1, SPRITE_MAX_FPS);
}

// ---- fit ----

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The source rectangle that is shown and the target rectangle it is shown
 *  in — the whole of what `fit`/`fill`/`crop` mean, in one place. */
function placement(sw: number, sh: number, tw: number, th: number, fit: FitMode): { src: Rect; dst: Rect } {
  const whole: Rect = { x: 0, y: 0, w: sw, h: sh };
  const box: Rect = { x: 0, y: 0, w: tw, h: th };
  if (fit === "fill") return { src: whole, dst: box };
  const sa = sw / sh;
  const ta = tw / th;
  if (fit === "crop") {
    // cover: keep the target's shape by cutting the source's long axis
    let cw = sw;
    let ch = sh;
    if (sa > ta) cw = sh * ta;
    else ch = sw / ta;
    return { src: { x: (sw - cw) / 2, y: (sh - ch) / 2, w: cw, h: ch }, dst: box };
  }
  // contain: the whole source, letterboxed inside the target
  let dw = tw;
  let dh = th;
  if (sa > ta) dh = tw / sa;
  else dw = th * sa;
  return { src: whole, dst: { x: (tw - dw) / 2, y: (th - dh) / 2, w: dw, h: dh } };
}

/** One frame into `rgb`/`on` at `at` (texels, not bytes). */
function resampleFrame(
  rgba: Uint8Array,
  sw: number,
  sh: number,
  o: ImportOptions,
  rects: { src: Rect; dst: Rect },
  rgb: Uint8Array,
  on: Uint8Array,
  at: number,
): void {
  const { src, dst } = rects;
  for (let y = 0; y < o.h; y++) {
    const cy = y + 0.5;
    for (let x = 0; x < o.w; x++) {
      const cx = x + 0.5;
      const i = at + y * o.w + x;
      // outside the letterbox: transparent, and no colour to record
      if (cx < dst.x || cx >= dst.x + dst.w || cy < dst.y || cy >= dst.y + dst.h) continue;
      const got =
        o.resample === "area"
          ? areaSample(rgba, sw, sh, src, dst, x, y)
          : nearestSample(rgba, sw, sh, src, dst, cx, cy);
      if (got === null || got[3] < o.alphaThreshold) continue;
      on[i] = 1;
      rgb[i * 3] = got[0];
      rgb[i * 3 + 1] = got[1];
      rgb[i * 3 + 2] = got[2];
    }
  }
}

/** `[r, g, b, a]` of the source pixel under the target pixel's centre. */
function nearestSample(
  rgba: Uint8Array,
  sw: number,
  sh: number,
  src: Rect,
  dst: Rect,
  cx: number,
  cy: number,
): [number, number, number, number] | null {
  const u = src.x + ((cx - dst.x) * src.w) / dst.w;
  const v = src.y + ((cy - dst.y) * src.h) / dst.h;
  const sx = clampInt(Math.floor(u), 0, sw - 1);
  const sy = clampInt(Math.floor(v), 0, sh - 1);
  const at = (sy * sw + sx) * 4;
  return [rgba[at] ?? 0, rgba[at + 1] ?? 0, rgba[at + 2] ?? 0, rgba[at + 3] ?? 0];
}

/**
 * Alpha-weighted average over the source box the target pixel covers, with
 * TRUE area weights (the overlap length on each axis), so a downscale loses
 * nothing to sample position. A box smaller than one source pixel degenerates
 * to the nearest sample, which is what upscaling should do anyway.
 */
function areaSample(
  rgba: Uint8Array,
  sw: number,
  sh: number,
  src: Rect,
  dst: Rect,
  x: number,
  y: number,
): [number, number, number, number] | null {
  const ux0 = src.x + ((x - dst.x) * src.w) / dst.w;
  const ux1 = src.x + ((x + 1 - dst.x) * src.w) / dst.w;
  const uy0 = src.y + ((y - dst.y) * src.h) / dst.h;
  const uy1 = src.y + ((y + 1 - dst.y) * src.h) / dst.h;
  const ax0 = Math.max(0, ux0);
  const ax1 = Math.min(sw, ux1);
  const ay0 = Math.max(0, uy0);
  const ay1 = Math.min(sh, uy1);
  if (!(ax1 > ax0) || !(ay1 > ay0)) {
    return nearestSample(rgba, sw, sh, src, dst, x + 0.5, y + 0.5);
  }
  let sr = 0;
  let sg = 0;
  let sb = 0;
  let sa = 0;
  let sarea = 0;
  const py0 = Math.floor(ay0);
  const py1 = Math.max(py0 + 1, Math.ceil(ay1));
  const px0 = Math.floor(ax0);
  const px1 = Math.max(px0 + 1, Math.ceil(ax1));
  for (let py = py0; py < py1; py++) {
    if (py < 0 || py >= sh) continue;
    const wy = Math.min(py + 1, ay1) - Math.max(py, ay0);
    if (!(wy > 0)) continue;
    for (let px = px0; px < px1; px++) {
      if (px < 0 || px >= sw) continue;
      const wx = Math.min(px + 1, ax1) - Math.max(px, ax0);
      if (!(wx > 0)) continue;
      const area = wx * wy;
      const at = (py * sw + px) * 4;
      const a = rgba[at + 3] ?? 0;
      const wa = area * a;
      sr += (rgba[at] ?? 0) * wa;
      sg += (rgba[at + 1] ?? 0) * wa;
      sb += (rgba[at + 2] ?? 0) * wa;
      sa += wa;
      sarea += area;
    }
  }
  if (!(sarea > 0)) return null;
  const avgA = sa / sarea;
  if (!(sa > 0)) return [0, 0, 0, Math.floor(avgA + 0.5)];
  return [
    clampInt(Math.floor(sr / sa + 0.5), 0, 255),
    clampInt(Math.floor(sg / sa + 0.5), 0, 255),
    clampInt(Math.floor(sb / sa + 0.5), 0, 255),
    clampInt(Math.floor(avgA + 0.5), 0, 255),
  ];
}

// ---- quantize ----

/** Distinct opaque colours and how often each occurs, in first-appearance
 *  order. Parallel arrays rather than objects: the median cut sorts indices
 *  into them and the Rust twin does the same. */
interface Histogram {
  r: number[];
  g: number[];
  b: number[];
  count: number[];
}

function histogram(rgb: Uint8Array, on: Uint8Array, texels: number): Histogram {
  const h: Histogram = { r: [], g: [], b: [], count: [] };
  const seen = new Map<number, number>();
  for (let i = 0; i < texels; i++) {
    if ((on[i] ?? 0) === 0) continue;
    const r = rgb[i * 3] ?? 0;
    const g = rgb[i * 3 + 1] ?? 0;
    const b = rgb[i * 3 + 2] ?? 0;
    const key = (r << 16) | (g << 8) | b;
    const at = seen.get(key);
    if (at === undefined) {
      seen.set(key, h.count.length);
      h.r.push(r);
      h.g.push(g);
      h.b.push(b);
      h.count.push(1);
    } else {
      h.count[at] = (h.count[at] ?? 0) + 1;
    }
  }
  return h;
}

function histColors(h: Histogram): Rgb8[] {
  const out: Rgb8[] = [];
  for (let i = 0; i < h.count.length; i++) out.push([h.r[i] ?? 0, h.g[i] ?? 0, h.b[i] ?? 0]);
  return out;
}

/**
 * Median cut down to `max` colours.
 *
 * Deterministic to the last tie: the bucket split next is the one with the
 * most texels (ties by position), the axis is the widest (ties r, then g,
 * then b), the entries are ordered by that channel with the packed colour as
 * the tie-break, and the cut is where the running texel count first reaches
 * half. The representative is the texel-weighted mean, rounded with
 * `floor(x + 0.5)`.
 */
function medianCut(h: Histogram, max: number): Rgb8[] {
  const n = h.count.length;
  if (n === 0) return [];
  let buckets: number[][] = [Array.from({ length: n }, (_, i) => i)];
  while (buckets.length < max) {
    let pick = -1;
    let best = -1;
    for (let i = 0; i < buckets.length; i++) {
      const b = buckets[i];
      if (!b || b.length < 2) continue;
      const total = bucketCount(h, b);
      if (total > best) {
        best = total;
        pick = i;
      }
    }
    if (pick < 0) break;
    const b = buckets[pick];
    if (!b) break;
    const axis = widestAxis(h, b);
    const chan = axis === 0 ? h.r : axis === 1 ? h.g : h.b;
    const sorted = [...b].sort((p, q) => {
      const d = (chan[p] ?? 0) - (chan[q] ?? 0);
      if (d !== 0) return d;
      return packed(h, p) - packed(h, q);
    });
    const total = bucketCount(h, sorted);
    let acc = 0;
    let cut = 1;
    for (let i = 0; i < sorted.length - 1; i++) {
      acc += h.count[sorted[i] ?? 0] ?? 0;
      cut = i + 1;
      if (acc * 2 >= total) break;
    }
    const left = sorted.slice(0, cut);
    const right = sorted.slice(cut);
    buckets = [...buckets.slice(0, pick), left, right, ...buckets.slice(pick + 1)];
  }
  return buckets.map((b) => representative(h, b));
}

function bucketCount(h: Histogram, b: readonly number[]): number {
  let total = 0;
  for (const i of b) total += h.count[i] ?? 0;
  return total;
}

function packed(h: Histogram, i: number): number {
  return ((h.r[i] ?? 0) << 16) | ((h.g[i] ?? 0) << 8) | (h.b[i] ?? 0);
}

/** 0 = red, 1 = green, 2 = blue — the widest extent, ties in that order. */
function widestAxis(h: Histogram, b: readonly number[]): number {
  const lo = [255, 255, 255];
  const hi = [0, 0, 0];
  for (const i of b) {
    const c = [h.r[i] ?? 0, h.g[i] ?? 0, h.b[i] ?? 0];
    for (let k = 0; k < 3; k++) {
      const v = c[k] ?? 0;
      if (v < (lo[k] ?? 0)) lo[k] = v;
      if (v > (hi[k] ?? 0)) hi[k] = v;
    }
  }
  let axis = 0;
  let span = (hi[0] ?? 0) - (lo[0] ?? 0);
  for (let k = 1; k < 3; k++) {
    const s = (hi[k] ?? 0) - (lo[k] ?? 0);
    if (s > span) {
      span = s;
      axis = k;
    }
  }
  return axis;
}

function representative(h: Histogram, b: readonly number[]): Rgb8 {
  let sr = 0;
  let sg = 0;
  let sb = 0;
  let sc = 0;
  for (const i of b) {
    const c = h.count[i] ?? 0;
    sr += (h.r[i] ?? 0) * c;
    sg += (h.g[i] ?? 0) * c;
    sb += (h.b[i] ?? 0) * c;
    sc += c;
  }
  if (sc === 0) return [0, 0, 0];
  return [
    clampInt(Math.floor(sr / sc + 0.5), 0, 255),
    clampInt(Math.floor(sg / sc + 0.5), 0, 255),
    clampInt(Math.floor(sb / sc + 0.5), 0, 255),
  ];
}

/** The palette entry closest to `(r, g, b)` in plain RGB distance, as a
 *  1-based index; ties go to the lower entry. */
export function nearestIndex(palette: readonly Rgb8[], r: number, g: number, b: number): number {
  let best = 0;
  let bestD = Number.POSITIVE_INFINITY;
  for (let k = 0; k < palette.length; k++) {
    const c = palette[k] ?? [0, 0, 0];
    const dr = r - (c[0] ?? 0);
    const dg = g - (c[1] ?? 0);
    const db = b - (c[2] ?? 0);
    const d = dr * dr + dg * dg + db * db;
    if (d < bestD) {
      bestD = d;
      best = k;
    }
  }
  return palette.length === 0 ? 0 : best + 1;
}

function mapNearest(rgb: Uint8Array, on: Uint8Array, texels: number, palette: readonly Rgb8[]): Uint8Array {
  const index = new Uint8Array(texels);
  // one lookup per DISTINCT colour, not per texel
  const memo = new Map<number, number>();
  for (let i = 0; i < texels; i++) {
    if ((on[i] ?? 0) === 0) continue;
    const r = rgb[i * 3] ?? 0;
    const g = rgb[i * 3 + 1] ?? 0;
    const b = rgb[i * 3 + 2] ?? 0;
    const key = (r << 16) | (g << 8) | b;
    let k = memo.get(key);
    if (k === undefined) {
      k = nearestIndex(palette, r, g, b);
      memo.set(key, k);
    }
    index[i] = k;
  }
  return index;
}

/**
 * Floyd–Steinberg, per frame, in INTEGER arithmetic: the error is carried in
 * whole levels and distributed with truncating division, which is the one way
 * two languages can agree on it. Off by default (#784: "it looks bad on
 * LEDs") — a 255-colour palette almost never needs it.
 */
function mapDithered(
  rgb: Uint8Array,
  on: Uint8Array,
  w: number,
  h: number,
  frames: number,
  palette: readonly Rgb8[],
): Uint8Array {
  const n = w * h;
  const index = new Uint8Array(n * Math.max(1, frames));
  for (let f = 0; f < frames; f++) {
    const cur = [new Int32Array(w), new Int32Array(w), new Int32Array(w)];
    const next = [new Int32Array(w), new Int32Array(w), new Int32Array(w)];
    for (let y = 0; y < h; y++) {
      for (let c = 0; c < 3; c++) {
        cur[c]?.set(next[c] ?? new Int32Array(w));
        next[c]?.fill(0);
      }
      for (let x = 0; x < w; x++) {
        const i = f * n + y * w + x;
        if ((on[i] ?? 0) === 0) continue;
        const want = [0, 0, 0];
        for (let c = 0; c < 3; c++) {
          want[c] = clampInt((rgb[i * 3 + c] ?? 0) + (cur[c]?.[x] ?? 0), 0, 255);
        }
        const k = nearestIndex(palette, want[0] ?? 0, want[1] ?? 0, want[2] ?? 0);
        index[i] = k;
        const got = palette[k - 1] ?? [0, 0, 0];
        for (let c = 0; c < 3; c++) {
          const d = (want[c] ?? 0) - (got[c] ?? 0);
          if (d === 0) continue;
          if (x + 1 < w) addErr(cur[c], x + 1, (d * 7) / 16);
          if (x > 0) addErr(next[c], x - 1, (d * 3) / 16);
          addErr(next[c], x, (d * 5) / 16);
          if (x + 1 < w) addErr(next[c], x + 1, d / 16);
        }
      }
    }
  }
  return index;
}

function addErr(row: Int32Array | undefined, at: number, v: number): void {
  if (!row) return;
  row[at] = (row[at] ?? 0) + Math.trunc(v);
}

// ---- the 16 KiB cap, and the two knobs that get under it ----

/** How many bytes a record of this shape would take. */
export function plannedBytes(o: ImportOptions, frames: number): number {
  const nameLen = new TextEncoder().encode(spriteName(o.name)).length;
  return 12 + nameLen + 3 * clampInt(o.colors, 0, SPRITE_MAX_COLORS) + o.w * o.h * Math.max(1, frames);
}

/** What [`fitUnderCap`] changed, as the sentence the dialog shows. */
export interface CapFix {
  options: ImportOptions;
  /** Empty when nothing had to change. */
  said: string;
}

/**
 * The two knobs, turned far enough — `keepEvery` first (frames are what a
 * record spends its bytes on), then the palette, and the target size only as
 * the last resort. Returns the options unchanged when it already fits, and
 * never returns something that does not.
 */
export function fitUnderCap(
  src: SourceImage,
  opts: ImportOptions,
  maxBytes = SPRITE_MAX_BYTES,
): CapFix {
  const o = resolveOptions(opts);
  const total = src.frames.length;
  const nameLen = new TextEncoder().encode(o.name).length;
  const fixed = 12 + nameLen;
  const said: string[] = [];

  let next = { ...o };
  const framesNow = Math.min(SPRITE_MAX_FRAMES, keptIndices(total, next.keepEvery).length);
  if (fixed + 3 * next.colors + next.w * next.h * framesNow <= maxBytes) {
    return { options: next, said: "" };
  }

  // 1. frames
  const room = maxBytes - fixed - 3 * next.colors;
  const perFrame = next.w * next.h;
  const fits = Math.floor(room / perFrame);
  if (fits >= 1 && total > 1) {
    const every = Math.max(1, Math.ceil(total / Math.min(fits, SPRITE_MAX_FRAMES)));
    if (every > next.keepEvery) {
      next = { ...next, keepEvery: every };
      said.push(`keeping every ${ordinal(every)} frame`);
    }
  }
  let frames = Math.min(SPRITE_MAX_FRAMES, keptIndices(total, next.keepEvery).length);
  if (fixed + 3 * next.colors + perFrame * Math.max(1, frames) <= maxBytes) {
    return { options: next, said: said.join(" and ") };
  }

  // 2. colours
  const left = maxBytes - fixed - perFrame * Math.max(1, frames);
  const colors = Math.floor(left / 3);
  if (colors >= 2) {
    next = { ...next, colors: Math.min(next.colors, colors) };
    said.push(`${next.colors} colours`);
    return { options: next, said: said.join(" and ") };
  }

  // 3. size — only when one frame of the target does not fit at all
  let w = next.w;
  let h = next.h;
  while (w > 1 && h > 1 && fixed + 6 + w * h * Math.max(1, frames) > maxBytes) {
    w = Math.max(1, w - 1);
    h = Math.max(1, Math.floor((next.h * w) / next.w + 0.5));
    frames = Math.min(SPRITE_MAX_FRAMES, keptIndices(total, next.keepEvery).length);
  }
  const room2 = maxBytes - fixed - w * h * Math.max(1, frames);
  next = { ...next, w, h, colors: clampInt(Math.floor(room2 / 3), 1, SPRITE_MAX_COLORS) };
  said.push(`${w}×${h} and ${next.colors} colours`);
  return { options: next, said: said.join(", ") };
}

function ordinal(n: number): string {
  if (n === 2) return "2nd";
  if (n === 3) return "3rd";
  return `${n}th`;
}

function clampInt(v: number, lo: number, hi: number): number {
  const n = Math.floor(v);
  if (!Number.isFinite(n)) return lo;
  return n < lo ? lo : n > hi ? hi : n;
}
