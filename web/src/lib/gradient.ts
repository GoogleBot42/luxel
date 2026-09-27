// The colour-ramp editor's MODEL — everything in the control that is a rule
// rather than widgetry. Gitea #787.
//
// The #787 bug inventory found 21 defects in the old `GradientEditor.svelte`,
// and almost every one of them was a decision with no canvas, no pointer and
// no device in it: where a new stop goes, whether a removal is allowed, what a
// trailing commit carries, what an emptied field means, which stops are
// reachable when two sit on top of each other, what a legal ramp even is. They
// live here, with no DOM and no Svelte, so `web/tests/gradient.test.mjs` holds
// them — `web/tests` cannot import a `.svelte` file, and a rule nothing can
// test is a rule that comes back.
//
// `components/ColorRamp.svelte` is the one component that mounts this model,
// in both of the app's homes for it (Settings › Output › Color ramp and the
// scene layer's Color ramp).

import { MAX_RAMP_STOPS } from "./scene.ts";

/** One editable stop: a byte position along the brightness axis, and a colour.
 *  This is the wire's own domain — `[pos, rrggbb]` in a scene `Ramp`, four
 *  bytes per stop in `POST /api/output/palette` — not a percentage. */
export interface GradientStop {
  /** 0..255. */
  pos: number;
  /** `rrggbb`, lowercase, no leading `#` (`#${hex}` for CSS). */
  hex: string;
}

/**
 * A stop carrying an opaque identity.
 *
 * Inventory item 8: the old editor held the selection as an INDEX, so when the
 * list changed under it (a re-sort mid-drag, a device poll, a removal) the
 * panel went on editing whatever stop had slid into that slot — and every edit
 * through it silently did nothing. The selection is an `id` here, so a stop
 * that disappears takes its row with it instead of aliasing its neighbour.
 */
export interface IdStop extends GradientStop {
  /** Unique within one editing session. Never sent anywhere. */
  id: number;
}

/** The cap, from the ONE constant (`lib/scene.ts`, itself pinned to
 *  `outpipe::MAX_OUTPUT_PALETTE_STOPS`). Never write `32` again. */
export const MAX_STOPS = MAX_RAMP_STOPS;

/**
 * The one legality rule, which the UI prints: a ramp is **0 stops (off) or at
 * least 2 — never 1**.
 *
 * Inventory item 21: there were three rules. The scene mount passed
 * `minStops=2`, the device palette passed `minStops=0`,
 * `crates/luxel-core/src/scene.rs` refuses a 1-stop scene ramp outright, and
 * `POST /api/output/palette "100 128 32 192 255"` happily accepted one. None
 * of them was on screen.
 */
export const MIN_STOPS = 2;

/** Is this a ramp the engine and both mounts will accept? */
export function legalStopCount(n: number): boolean {
  return n === 0 || n >= MIN_STOPS;
}

const clampInt = (v: number, lo: number, hi: number): number =>
  Number.isFinite(v) ? Math.max(lo, Math.min(hi, Math.round(v))) : lo;

/** 0..255, rounded — the engine's position domain. */
export const clampByte = (v: number): number => clampInt(v, 0, 255);

/** 0..100, rounded — a blend amount. */
export const clampPct = (v: number): number => clampInt(v, 0, 100);

/** `rrggbb`, lowercase, no `#`, however it arrived. */
export function normalizeHex(hex: string): string {
  const t = hex.replace(/^#/, "").toLowerCase();
  return /^[0-9a-f]{6}$/.test(t) ? t : "000000";
}

// ── identities ────────────────────────────────────────────────────────────

let nextId = 1;

/** Give a plain stop list fresh identities (on open, on Load theirs, on a
 *  preset). Positions are clamped and the list is sorted, because everything
 *  downstream — `sample_palette` included — assumes ascending. */
export function assignIds(list: readonly GradientStop[]): IdStop[] {
  return sortStops(
    list.map((s) => ({ pos: clampByte(s.pos), hex: normalizeHex(s.hex), id: nextId++ })),
  );
}

/** Back to the wire's shape. */
export function stripIds(list: readonly IdStop[]): GradientStop[] {
  return list.map((s) => ({ pos: s.pos, hex: s.hex }));
}

/** Ascending by position, STABLY (ties keep their order), which is what makes
 *  dragging one stop past another keep hold of the one you are dragging. */
export function sortStops<T extends GradientStop>(list: readonly T[]): T[] {
  return list
    .map((s, n) => ({ s, n }))
    .sort((a, b) => a.s.pos - b.s.pos || a.n - b.n)
    .map((t) => t.s);
}

/** Do two stop lists say the same thing? Used to tell "the device changed
 *  underneath me" from "the device is echoing back what I just sent", and to
 *  light the preset chip that matches. */
export function sameStops(a: readonly GradientStop[], b: readonly GradientStop[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((s, i) => {
    const o = b[i];
    return o !== undefined && s.pos === o.pos && normalizeHex(s.hex) === normalizeHex(o.hex);
  });
}

// ── the engine's 256-entry table ──────────────────────────────────────────
//
// The bar and both preview cells are the engine's own luma → colour LUT, not a
// CSS `linear-gradient`: the table is 16.16 fixed point with truncating
// divides, it CLAMPS at BOTH ends since #787 (Jeremy's decision, 2026-09-27 —
// option A, frame S8c), and it quantizes with `floor(v·255)`. A gradient
// cannot express any of that, which is why the pre-#734 preview visibly
// disagreed with the device.
//
// Since #748 the table comes from wasm — `lx_palette_lut`, cooked by
// `outpipe::fill_palette_lut` and blended by `outpipe::palette_remap_frame`
// itself, so there is ONE implementation of a wire-visible rule. What follows
// is the synchronous FALLBACK for the first paint, before the module is up;
// `web/tests/paletteLut.test.mjs` pins it against the real export over every
// shape that hurts, so the two cannot drift silently.

const FX_ONE = 65536;

/** `Fx::mul` — the full product shifted right 16. Rust's `>>` on i64 is
 *  ARITHMETIC, so this floors; the channel deltas below go negative and a
 *  `trunc` here would be off by one on every falling edge. */
const fxMul = (a: number, b: number): number => Math.floor((a * b) / FX_ONE);

/** `Fx::div` — truncated, and 0 on a zero divisor (oracle-verified). */
function fxDiv(a: number, b: number): number {
  if (b === 0) return 0;
  if ((b & 0xffff) === 0) return Math.trunc(a / (b >> 16));
  return Math.trunc((a * FX_ONE) / b);
}

/** A 0..255 byte → 16.16 over 0..1, truncating (`fill_palette_lut`'s `b`). */
const fxByte = (v: number): number => Math.trunc((v * FX_ONE) / 255);

/** `engine::quantize`: `floor(v·255)` on a clamped unit value. */
const quant = (v: number): number =>
  Math.floor(((v < 0 ? 0 : v > FX_ONE ? FX_ONE : v) * 255) / FX_ONE);

type FxStop = [number, [number, number, number]];

/**
 * `luxel_core::vm::sample_palette` with the RAMP's edge rule.
 *
 * The one difference from the Rust function of that name is the top edge, and
 * it is deliberate on both sides: `vm::sample_palette` returns BLACK above the
 * last stop because that is what a real Pixel Blaze's `paint()` does,
 * bug-for-bug (oracle 2026-08-22, `palette_edges_match_pixelblaze`). The RAMP
 * stage is a Luxel extension with no PB behaviour to match, and since #787 it
 * continues the last colour instead — `outpipe::fill_palette_lut` clamps its
 * sample position to the last stop, which is the one line that decides it.
 */
function sampleRamp(pal: readonly FxStop[], v: number): [number, number, number] {
  const first = pal[0];
  const last = pal[pal.length - 1];
  if (first === undefined || last === undefined) return [v, v, v];
  // `fill_palette_lut` clamps the SAMPLE POSITION to the last stop and then
  // runs `vm::sample_palette` unchanged — which is NOT the same as returning
  // the last colour. When two stops share a position, the clamped position
  // falls into `sample_palette`'s `v <= first` branch, so the FIRST of them
  // wins everywhere above it; returning `last[1]` here disagreed with the
  // engine (caught by `web/tests/paletteLut.test.mjs`'s zero-width case).
  const p = v > last[0] ? last[0] : v;
  if (p <= first[0]) return first[1];
  if (p >= last[0]) return last[1];
  for (let i = 0; i + 1 < pal.length; i++) {
    const lo = pal[i];
    const hi = pal[i + 1];
    if (lo === undefined || hi === undefined) break;
    if (p <= hi[0]) {
      const span = hi[0] - lo[0];
      const t = span === 0 ? 0 : fxDiv(p - lo[0], span);
      return [
        lo[1][0] + fxMul(hi[1][0] - lo[1][0], t),
        lo[1][1] + fxMul(hi[1][1] - lo[1][1], t),
        lo[1][2] + fxMul(hi[1][2] - lo[1][2], t),
      ];
    }
  }
  return last[1];
}

/** The four bytes per stop the wasm export (and the device) take: `pos r g b`,
 *  sorted ascending. */
export function stopBytes(stops: readonly GradientStop[]): Uint8Array {
  const sorted = sortStops(stops);
  const out = new Uint8Array(sorted.length * 4);
  sorted.forEach((s, i) => {
    const n = Number.parseInt(normalizeHex(s.hex), 16);
    out[i * 4] = clampByte(s.pos);
    out[i * 4 + 1] = (n >> 16) & 0xff;
    out[i * 4 + 2] = (n >> 8) & 0xff;
    out[i * 4 + 3] = n & 0xff;
  });
  return out;
}

/**
 * The 768-byte table the device builds from these stops, blended at
 * `amountPct` the way `palette_remap_frame` blends it over a greyscale frame —
 * which is exactly what the bar draws, because `luma([i,i,i]) === i`
 * (54+183+19 = 256). Entry `i` is literally "what the device turns a pixel of
 * brightness `i` into at this amount".
 *
 * An EMPTY stop list is not special-cased: the sampler returns the identity
 * for one, so the bar goes grey — which is what "no ramp" means.
 *
 * THE FALLBACK, not the source of truth: `Luxel.paletteLut` is. See the block
 * comment above.
 */
export function rampLut(stops: readonly GradientStop[], amountPct = 100): Uint8Array {
  const bytes = stopBytes(stops);
  const pal: FxStop[] = [];
  for (let i = 0; i + 3 < bytes.length; i += 4) {
    pal.push([
      fxByte(bytes[i] ?? 0),
      [fxByte(bytes[i + 1] ?? 0), fxByte(bytes[i + 2] ?? 0), fxByte(bytes[i + 3] ?? 0)],
    ]);
  }
  // `pct as u32 * 256 / 100`, then `amount.min(256)` (compose.rs / outpipe.rs)
  const a = Math.min(256, Math.trunc((clampPct(amountPct) * 256) / 100));
  const out = new Uint8Array(768);
  for (let i = 0; i < 256; i++) {
    const c = sampleRamp(pal, fxByte(i));
    const t = [quant(c[0]), quant(c[1]), quant(c[2])];
    for (let k = 0; k < 3; k++) {
      const target = t[k] ?? 0;
      out[i * 3 + k] = a >= 256 ? target : (i * (256 - a) + target * a) >> 8;
    }
  }
  return out;
}

/** `outpipe::luma` — the brightness the ramp is indexed by. */
export const luma = (r: number, g: number, b: number): number =>
  (r * 54 + g * 183 + b * 19) >> 8;

/**
 * `outpipe::palette_remap_frame` over a copy: a finished frame recoloured
 * through a 100 %-cooked table, blended against the pattern's OWN pixel at
 * `amountPct`. This is what the right-hand preview cell shows — the LEDs.
 *
 * Note the asymmetry with [`rampLut`], which bakes the amount in against a
 * GREYSCALE frame because that is what the bar stands for. Here the frame is
 * real, so the blend has to happen per pixel against it, exactly as the engine
 * does it.
 */
export function remapFrame(px: Uint8Array, lut100: Uint8Array, amountPct: number): Uint8Array {
  const a = Math.min(256, Math.trunc((clampPct(amountPct) * 256) / 100));
  const out = new Uint8Array(px.length);
  const n = Math.floor(px.length / 3);
  for (let i = 0; i < n; i++) {
    const r = px[i * 3] ?? 0;
    const g = px[i * 3 + 1] ?? 0;
    const b = px[i * 3 + 2] ?? 0;
    if (a === 0) {
      out[i * 3] = r;
      out[i * 3 + 1] = g;
      out[i * 3 + 2] = b;
      continue;
    }
    const k = luma(r, g, b) * 3;
    const t = [lut100[k] ?? 0, lut100[k + 1] ?? 0, lut100[k + 2] ?? 0];
    const src = [r, g, b];
    for (let c = 0; c < 3; c++) {
      const target = t[c] ?? 0;
      out[i * 3 + c] =
        a >= 256 ? target : (((src[c] ?? 0) * (256 - a) + target * a) >> 8) as number;
    }
  }
  return out;
}

/** A greyscale wedge frame — what stands in for a pattern with nothing
 *  running (mock S8h, `brightness 0 → 255`). */
export function lumaWedge(n = 256): Uint8Array {
  const out = new Uint8Array(n * 3);
  for (let i = 0; i < n; i++) {
    const v = n === 1 ? 0 : Math.round((i * 255) / (n - 1));
    out[i * 3] = v;
    out[i * 3 + 1] = v;
    out[i * 3 + 2] = v;
  }
  return out;
}

const pair = (n: number): string => n.toString(16).padStart(2, "0");

/** The colour a 768-byte table holds at brightness `i`, as `rrggbb`. */
export function lutHex(lut: Uint8Array, i: number): string {
  const k = clampByte(i) * 3;
  return `${pair(lut[k] ?? 0)}${pair(lut[k + 1] ?? 0)}${pair(lut[k + 2] ?? 0)}`;
}

/**
 * Paint a 768-byte table into a canvas: 256 columns, one per entry, stretched
 * by CSS with `image-rendering: pixelated`, so the control literally cannot
 * disagree with the render.
 */
export function paintLut(cv: HTMLCanvasElement | null | undefined, lut: Uint8Array): void {
  if (!cv) return;
  const ctx = cv.getContext("2d");
  if (!ctx) return;
  if (cv.width !== 256) cv.width = 256;
  if (cv.height !== 1) cv.height = 1;
  const img = ctx.createImageData(256, 1);
  for (let i = 0; i < 256; i++) {
    img.data[i * 4] = lut[i * 3] ?? 0;
    img.data[i * 4 + 1] = lut[i * 3 + 1] ?? 0;
    img.data[i * 4 + 2] = lut[i * 3 + 2] ?? 0;
    img.data[i * 4 + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
}

// ── adding, moving, removing ──────────────────────────────────────────────

/**
 * Add a stop at `pos`, taking the colour the ramp ALREADY has there — so
 * adding one never changes the gradient.
 *
 * That was the old component's stated contract and it was false above the last
 * stop, where the table went black: inventory item 10, a click at 85 % added a
 * BLACK stop and moved 88 of 256 LUT entries. Since the ramp clamps at both
 * ends (#787, option A) it is true everywhere on the bar, by construction.
 *
 * `null` at the cap — the caller says why, it does not silently do nothing
 * (item 12).
 */
export function addStopAt(
  list: readonly IdStop[],
  pos: number,
  lut: Uint8Array,
): { stops: IdStop[]; id: number } | null {
  if (list.length >= MAX_STOPS) return null;
  const p = clampByte(pos);
  const id = nextId++;
  // In the CLAMPED zones the answer is the end stop's own hex, not the table's
  // reading of it: the LUT is quantized with `floor(v·255)`, so `f7e08a` reads
  // back as `f6df89` and re-inserting that would tilt the plateau by one unit
  // per channel over ~40 entries. Inside the span the table IS the truth (it is
  // what the device will show) and its rounding is already in the picture.
  const sorted = sortStops(list);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  const hex =
    first !== undefined && p <= first.pos
      ? first.hex
      : last !== undefined && p >= last.pos
        ? last.hex
        : lutHex(lut, p);
  return { stops: sortStops([...list, { pos: p, hex, id }]), id };
}

/**
 * Where `+ Add stop` puts one: the MIDDLE OF THE WIDEST GAP between two
 * existing stops.
 *
 * Inventory item 5: it used to be `min(255, last.pos + 64)`, so from the fifth
 * stop on every new one landed at 255, exactly on top of the previous one.
 * Thirty clicks produced "32 stops", two visible handles, `add stop` disabled
 * at the cap, and no way for the user to find the other thirty. "Never past the
 * last stop" is the other half of the rule — the widest gap is always between
 * two stops, so a new one is always visible and always draggable.
 *
 * `null` when there is no gap to add into: at the cap, or below the two stops a
 * gap needs.
 */
export function widestGapPos(list: readonly IdStop[]): number | null {
  if (list.length >= MAX_STOPS || list.length < MIN_STOPS) return null;
  const s = sortStops(list);
  let best = -1;
  let at = 0;
  for (let i = 0; i + 1 < s.length; i++) {
    const lo = s[i];
    const hi = s[i + 1];
    if (lo === undefined || hi === undefined) continue;
    const w = hi.pos - lo.pos;
    if (w > best) {
      best = w;
      at = lo.pos + Math.floor(w / 2);
    }
  }
  return best < 0 ? null : clampByte(at);
}

/**
 * Move the stop with `id` to `pos`. The list is re-sorted stably, so the stop
 * being dragged keeps its identity across a crossing — the caller's selection
 * needs no fixing up, which is the whole point of `IdStop`.
 */
export function moveStopTo(list: readonly IdStop[], id: number, pos: number): IdStop[] {
  return sortStops(list.map((s) => (s.id === id ? { ...s, pos: clampByte(pos) } : s)));
}

/** Recolour the stop with `id`. */
export function setStopHex(list: readonly IdStop[], id: number, hex: string): IdStop[] {
  const h = normalizeHex(hex);
  return list.map((s) => (s.id === id ? { ...s, hex: h } : s));
}

/**
 * Remove stop `i` — or REFUSE (`null`) when the list is already at its floor.
 *
 * Gitea #787 §3: below `minStops` the editor used to fall through to
 * `clearAll()`, so pressing Delete on one of a scene ramp's two stops dropped
 * the layer's whole `Ramp` record. `Edit…` then reseeds black→white, which
 * makes the ramp the user had unrecoverable — no confirmation, no undo. A
 * removal now removes a stop or does nothing; destroying the ramp is a
 * separate, named, confirmed action.
 *
 * `picked` comes back because an index-based caller has to follow the
 * shortened list. Since #787 step 3 the component selects by identity and uses
 * [`removeStopId`] instead; this signature stays because it is the tested
 * shape of the rule and `minStops` is not always 2 (a device palette's last
 * stop was legitimately removable before the one printed rule replaced the
 * three unprinted ones).
 */
export function removeStopAt<T>(
  list: readonly T[],
  i: number,
  minStops: number,
): { stops: T[]; picked: number } | null {
  if (!Number.isInteger(i) || i < 0 || i >= list.length) return null;
  if (list.length <= minStops) return null;
  const stops = list.filter((_, n) => n !== i);
  return { stops, picked: Math.max(0, Math.min(i, stops.length - 1)) };
}

/** [`removeStopAt`] by identity, at the one printed floor: the list, plus the
 *  id the selection should move to (the neighbour), or `null` if refused. */
export function removeStopId(
  list: readonly IdStop[],
  id: number,
): { stops: IdStop[]; select: number | null } | null {
  const i = list.findIndex((s) => s.id === id);
  if (i < 0) return null;
  const r = removeStopAt(list, i, MIN_STOPS);
  if (!r) return null;
  return { stops: r.stops, select: r.stops[r.picked]?.id ?? null };
}

// ── stops that sit on top of each other ───────────────────────────────────

/**
 * Group stops whose handles would overlap into clusters, so the bar can draw
 * ONE handle with a count badge and offer a *next of N* button that cycles the
 * selection.
 *
 * Inventory item 15: two stops at one position were drawn exactly on top of
 * each other with no z-order cue, no badge and no way to grab the one
 * underneath — which combined with item 5 is how thirty invisible stops
 * accumulated at 255. `tolerance` is in POSITION units; the component derives
 * it from the mock's 3 px over the bar's measured width.
 *
 * The input is sorted; the output is in ascending position order, each cluster
 * holding its members' ids in list order.
 */
export function clusterStops(
  list: readonly IdStop[],
  tolerance: number,
): { pos: number; ids: number[] }[] {
  const out: { pos: number; ids: number[] }[] = [];
  for (const s of sortStops(list)) {
    const last = out[out.length - 1];
    if (last !== undefined && s.pos - last.pos <= tolerance) last.ids.push(s.id);
    else out.push({ pos: s.pos, ids: [s.id] });
  }
  return out;
}

/** The id *after* `id` inside its cluster, wrapping — what *next of N* picks. */
export function nextInCluster(
  clusters: readonly { pos: number; ids: number[] }[],
  id: number,
): number | null {
  for (const c of clusters) {
    const i = c.ids.indexOf(id);
    if (i >= 0) return c.ids[(i + 1) % c.ids.length] ?? null;
  }
  return null;
}

// ── presets ───────────────────────────────────────────────────────────────

/**
 * The six named ramps the mock offers (S8a's `.represets`). A preset REPLACES
 * the stops, is undoable, and never touches the amount — inventory item 7 was
 * `clear` silently zeroing the amount, so rebuilding a palette afterwards was
 * invisible.
 *
 * *To black* is where the pre-#787 engine behaviour lives now: Jeremy's
 * decision (2026-09-27) made the ramp clamp at both ends, and "the brightest
 * pixels go dark" is a thing you say with a black last stop rather than
 * something you discover.
 */
export const RAMP_PRESETS: { name: string; stops: GradientStop[] }[] = [
  {
    name: "Mono",
    stops: [
      { pos: 0, hex: "000000" },
      { pos: 255, hex: "ffffff" },
    ],
  },
  {
    name: "Sunset",
    stops: [
      { pos: 0, hex: "000000" },
      { pos: 87, hex: "2a1060" },
      { pos: 173, hex: "c23a6b" },
      { pos: 255, hex: "f7e08a" },
    ],
  },
  {
    name: "Ice",
    stops: [
      { pos: 0, hex: "03121f" },
      { pos: 128, hex: "1d6f8f" },
      { pos: 255, hex: "9fe4f0" },
    ],
  },
  {
    name: "Fire",
    stops: [
      { pos: 0, hex: "1a0500" },
      { pos: 128, hex: "c43b07" },
      { pos: 255, hex: "ffd86b" },
    ],
  },
  {
    name: "Spectrum",
    stops: [
      { pos: 0, hex: "ff0000" },
      { pos: 51, hex: "ffff00" },
      { pos: 102, hex: "00ff00" },
      { pos: 153, hex: "00ffff" },
      { pos: 204, hex: "0000ff" },
      { pos: 255, hex: "ff00ff" },
    ],
  },
  {
    name: "To black",
    stops: [
      { pos: 0, hex: "f7e08a" },
      { pos: 128, hex: "c23a6b" },
      { pos: 255, hex: "000000" },
    ],
  },
];

/** Which preset the current stops ARE, or `null` — lights the chip. */
export function matchingPreset(stops: readonly GradientStop[]): string | null {
  const sorted = sortStops(stops);
  return RAMP_PRESETS.find((p) => sameStops(sorted, p.stops))?.name ?? null;
}

// ── undo ──────────────────────────────────────────────────────────────────

/** A bounded undo stack over whole stop lists. */
export interface History<T> {
  /** Remember the state BEFORE an edit. */
  push(value: T): void;
  /** The last remembered state, removed — or `null` with nothing to undo. */
  undo(): T | null;
  canUndo(): boolean;
  /** Start over from `value` (the editor opening, or *Reset*). */
  reset(value: T): void;
  /** The state the editor opened with — what *Reset* returns to. */
  base(): T;
  depth(): number;
}

/**
 * Inventory item 18: the old control had no undo, no presets and no reset, and
 * `clear` and `remove` were both irreversible. Undo covers every edit since
 * the editor opened, bounded so a colour drag's storm of frames cannot grow it
 * without limit (the component pushes once per gesture, not per pointermove).
 */
export function history<T>(initial: T, limit = 64): History<T> {
  let base = initial;
  let stack: T[] = [];
  return {
    push(value: T): void {
      stack.push(value);
      if (stack.length > limit) stack = stack.slice(stack.length - limit);
    },
    undo(): T | null {
      return stack.length === 0 ? null : (stack.pop() ?? null);
    },
    canUndo: (): boolean => stack.length > 0,
    reset(value: T): void {
      base = value;
      stack = [];
    },
    base: (): T => base,
    depth: (): number => stack.length,
  };
}

// ── the trailing commit ───────────────────────────────────────────────────

/** A pending trailing commit. `pending()` is for tests and assertions. */
export interface TrailingCommit {
  /** (Re)start the timer from now. */
  arm(): void;
  /** Drop a pending commit — a later edit supersedes it. */
  cancel(): void;
  /** Fire it NOW if one is pending (the colour popover closing). */
  flush(): void;
  /** Is a commit still in flight? */
  pending(): boolean;
}

/**
 * The trailing commit behind a colour storm (Gitea #787 §2).
 *
 * `ColorPicker` emits on every pointermove across its saturation field, so a
 * colour edit has to commit once the hand STOPS — Settings would otherwise
 * POST the whole palette per mouse move. The defect was what the pending
 * commit carried: the list captured when it was armed. A drag or a Delete
 * inside the 250 ms window was therefore undone 250 ms later — the drag lost,
 * the deleted stop back.
 *
 * Three rules fix it and all three are properties of this latch:
 *   1. it commits whatever `current()` says at the moment it FIRES,
 *   2. any other edit `cancel()`s it, so the later gesture wins, and
 *   3. `flush()` lands it immediately when the gesture visibly ends, which is
 *      the mock's "one write when the picker closes" (S8b).
 */
export function trailingCommit<T>(
  delayMs: number,
  current: () => T,
  commit: (value: T) => void,
): TrailingCommit {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancel = (): void => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  };
  return {
    arm(): void {
      cancel();
      timer = setTimeout(() => {
        timer = undefined;
        commit(current());
      }, delayMs);
    },
    cancel,
    flush(): void {
      if (timer === undefined) return;
      cancel();
      commit(current());
    },
    pending: (): boolean => timer !== undefined,
  };
}

// ── the two number fields ─────────────────────────────────────────────────

/**
 * What a number field the user CLEARED means: nothing (Gitea #787 §4).
 *
 * `Number("")` is 0, and it was unguarded — clearing `position` slammed the
 * stop to 0, and clearing `amount` set the blend to 0, i.e. silently turned
 * the palette off with no message. `null` is "no change", and the caller puts
 * the value in effect back in the box so the field never shows a value the
 * model refused (item 9).
 */
export function fieldNumber(raw: string): number | null {
  const t = raw.trim();
  if (t === "") return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : null;
}

// ── re-reading the owner's value while the editor is open ─────────────────

/** What to do with a value that arrived from outside the editor. */
export type SyncVerdict =
  /** Nothing new — or an echo of something this editor itself put out. */
  | "ignore"
  /** Take it: the editor has no local work to lose. */
  | "adopt"
  /** It moved under local edits: hold, show the strip, send NOTHING. */
  | "conflict";

/**
 * The three-way decision behind inventory item 1 (the worst thing in the
 * inventory: the old editor read the device's palette once at mount, and an
 * edit twenty minutes later pushed the STALE stops and the STALE amount over
 * whatever the device actually held).
 *
 * It is a rule, not a widget, so it lives here with tests on it — and it needs
 * them, because it has THREE inputs that look like two:
 *
 * * `loaded` — the external value the editor last adopted or last COMMITTED.
 *   The device echoing back a committed write matches this.
 * * `mine` — the last list the editor put into the world, committed **or
 *   not**. A mount that hands an `input` straight back through its prop (the
 *   Settings card does, optimistically) matches this and nothing else, and
 *   treating that as a foreign change raised the conflict strip against the
 *   editor itself — after which every later write was muted. Every emitter has
 *   to record `mine`; the one that forgot broke the whole control.
 * * `edited` — whether there is local work a conflict would protect.
 *
 * An EMPTY incoming ramp carries no amount worth comparing: the stage is off
 * and `DELETE /api/output/palette` deliberately stores 0 with it, so the
 * caller passes its own working amount as `incomingAmount` (see `syncIn`).
 */
export function syncVerdict(s: {
  incoming: readonly GradientStop[];
  incomingAmount: number;
  loaded: readonly GradientStop[] | null;
  loadedAmount: number;
  mine: readonly GradientStop[] | null;
  workAmount: number;
  edited: boolean;
}): SyncVerdict {
  if (s.loaded !== null && sameStops(s.incoming, s.loaded) && s.incomingAmount === s.loadedAmount) {
    return "ignore";
  }
  if (s.mine !== null && sameStops(s.incoming, s.mine) && s.incomingAmount === s.workAmount) {
    return "ignore";
  }
  return s.edited ? "conflict" : "adopt";
}

// ── which of #563's modes this mount is in ────────────────────────────────

/**
 * Inventory item 20: the scene mount wired `on:input` and `on:change` to one
 * handler and threw the live-vs-committed split away. #563 settled that
 * opening a thing is not a device action; the ramp editor is the same question
 * one level down, and the answer is a VISIBLE property of the control — so
 * wiring it wrong is visible too.
 */
export type RampMode = "live" | "save" | "preview";

/** The mode line's copy, quoted from S8i. */
export function modeText(mode: RampMode): string {
  switch (mode) {
    case "live":
      return "Changes go to the LEDs as you make them.";
    case "save":
      return "Saved with the scene — the LEDs follow when you save.";
    default:
      return "Preview only — nothing is connected.";
  }
}
