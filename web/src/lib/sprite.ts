// A SPRITE is a sprite-tagged PATTERN (Gitea #481, docs/spec/scenes.md §4) —
// not a new store record. This module is the browser's codec for that format:
// it reads the three HSV arrays back out of a pattern's SOURCE, and writes a
// canonical sprite pattern out again after the tool row has painted a pixel.
//
// Why the source and not the compiled engine: drawing has to WRITE, and the
// only thing the store holds for a pattern is its source (plus the bytecode
// the browser recompiles from it). `luxel_core::compose::sprite_view` reads
// the same three arrays out of the compiled const pool, so a round trip
// through `parseSprite` → paint → `emitSprite` → compile has to agree with it
// texel for texel — `web/tests/sprite.test.mjs` pins that against the real
// wasm compositor.
//
// The ≤ 16-colour palette is an EDITOR rule, not a format rule (contract §4):
// nothing here refuses a source with more, it is the tool row that stops you
// adding a seventeenth.

import { hsvToRgb, rgbToHsv, type Hsv, type Rgb } from "./color.ts";
import { parseSpriteTag, SPRITE_MAX_EDGE, SPRITE_MAX_FPS, type SpriteTag } from "./scene.ts";

export { SPRITE_MAX_EDGE, SPRITE_MAX_FPS };
export type { SpriteTag };

/** The editor's palette rule: a sprite may use at most this many distinct
 *  opaque colours (proposal §5.5b / S7c's 16-cell swatch grid). */
export const SPRITE_MAX_COLORS = 16;

/** The three arrays the format names, in declaration order. */
export const SPRITE_ARRAYS = ["sprH", "sprS", "sprV"] as const;

/** A sprite's pixels: `w*h*frames` HSV triples, row-major within a frame and
 *  frame-major overall. `v === 0` is TRANSPARENT (the `blit` mode-3 key). */
export interface Sprite {
  tag: SpriteTag;
  h: number[];
  s: number[];
  v: number[];
}

/** Texel count for a tag — what every array's length must be. */
export function spriteLength(tag: SpriteTag): number {
  return tag.w * tag.h * tag.frames;
}

/** Index of (col, row) in `frame` — the format's row-major, frame-major
 *  order, and the same index `blit_sprite` walks. */
export function spriteIndex(tag: SpriteTag, frame: number, col: number, row: number): number {
  return frame * tag.w * tag.h + row * tag.w + col;
}

// ---- reading ----

/** One top-level `var <name> = [ … ]` numeric literal, or null.
 *
 *  Deliberately a scan and not a parse: the browser never needs to understand
 *  the rest of the pattern, only to find three literals it wrote itself (or
 *  that a library sprite wrote in the same shape). Anything else — a computed
 *  array, a name that is not at the top level — reads as "not a sprite this
 *  editor can draw on", which is exactly the answer the tool row wants. */
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
    const n = Number(t);
    if (!Number.isFinite(n)) return null;
    out.push(n);
  }
  return out;
}

/**
 * The sprite a pattern source carries, or null when it is not one the editor
 * can draw on — no tag, a missing array, or a length that disagrees with the
 * tag. `parseSpriteTag` alone answers the weaker question ("is this layer a
 * sprite at all"), which is what the layer list and the inspector's header
 * use; this answers "can I paint on it".
 */
export function parseSprite(source: string): Sprite | null {
  const tag = parseSpriteTag(source);
  if (!tag) return null;
  const h = readArray(source, "sprH");
  const s = readArray(source, "sprS");
  const v = readArray(source, "sprV");
  if (!h || !s || !v) return null;
  const n = spriteLength(tag);
  if (h.length !== n || s.length !== n || v.length !== n) return null;
  return { tag, h, s, v };
}

// ---- the palette ----

/** The distinct opaque colours a sprite uses, in first-appearance order —
 *  stable, so repainting one pixel never reshuffles the swatch row. A
 *  transparent texel (`v === 0`) is not a colour and never takes a slot. */
export function spritePalette(sprite: Sprite): Hsv[] {
  const seen = new Set<string>();
  const out: Hsv[] = [];
  for (let i = 0; i < sprite.v.length; i++) {
    const v = sprite.v[i] ?? 0;
    if (v === 0) continue;
    const hsv: Hsv = [sprite.h[i] ?? 0, sprite.s[i] ?? 0, v];
    const key = hsvKey(hsv);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(hsv);
  }
  return out;
}

/** Two texels are the SAME colour when they round to the same stored
 *  literal — the emitter writes 4 decimals, so that is the grain the palette
 *  counts in. */
export function hsvKey(hsv: Hsv): string {
  return `${num(hsv[0])},${num(hsv[1])},${num(hsv[2])}`;
}

/** Would painting `hsv` need a seventeenth colour? The tool row's disabled
 *  reason (S7c: the palette is 16 cells and no more). */
export function paletteWouldOverflow(sprite: Sprite, hsv: Hsv): boolean {
  if (hsv[2] === 0) return false; // the eraser is never a colour
  const pal = spritePalette(sprite);
  if (pal.length < SPRITE_MAX_COLORS) return false;
  const key = hsvKey(hsv);
  return !pal.some((p) => hsvKey(p) === key);
}

export function paletteCss(hsv: Hsv): string {
  const rgb = hsvToRgb(hsv);
  return `rgb(${rgb.map((c) => Math.round(c * 255)).join(",")})`;
}

export function hsvFromRgb(rgb: Rgb): Hsv {
  return rgbToHsv(rgb);
}

// ---- writing ----

/** A brand-new, fully transparent sprite. */
export function newSprite(w: number, h: number, frames = 1, fps = 0): Sprite {
  const tag: SpriteTag = {
    w: clampInt(w, 1, SPRITE_MAX_EDGE),
    h: clampInt(h, 1, SPRITE_MAX_EDGE),
    frames: Math.max(1, Math.round(frames)),
    fps: clampInt(fps, 0, SPRITE_MAX_FPS),
  };
  const n = spriteLength(tag);
  return { tag, h: new Array<number>(n).fill(0), s: new Array<number>(n).fill(0), v: new Array<number>(n).fill(0) };
}

/** Grow, shrink or re-frame a sprite, keeping the texels that still fit. The
 *  inspector's `w`/`h`/`Frames` fields (S7c) are the only callers. */
export function resizeSprite(sprite: Sprite, next: SpriteTag): Sprite {
  const out = newSprite(next.w, next.h, next.frames, next.fps);
  for (let f = 0; f < Math.min(sprite.tag.frames, out.tag.frames); f++) {
    for (let row = 0; row < Math.min(sprite.tag.h, out.tag.h); row++) {
      for (let col = 0; col < Math.min(sprite.tag.w, out.tag.w); col++) {
        const from = spriteIndex(sprite.tag, f, col, row);
        const to = spriteIndex(out.tag, f, col, row);
        out.h[to] = sprite.h[from] ?? 0;
        out.s[to] = sprite.s[from] ?? 0;
        out.v[to] = sprite.v[from] ?? 0;
      }
    }
  }
  return out;
}

/** Paint one texel. Returns a NEW sprite — the editor's records are replaced,
 *  never mutated, so Svelte sees the change. */
export function paintSprite(sprite: Sprite, frame: number, col: number, row: number, hsv: Hsv | null): Sprite {
  const i = spriteIndex(sprite.tag, frame, col, row);
  if (i < 0 || i >= sprite.v.length) return sprite;
  const next: Sprite = { tag: sprite.tag, h: [...sprite.h], s: [...sprite.s], v: [...sprite.v] };
  next.h[i] = hsv ? hsv[0] : 0;
  next.s[i] = hsv ? hsv[1] : 0;
  next.v[i] = hsv ? hsv[2] : 0;
  return next;
}

/** Flood fill from (col, row) within ONE frame, four-connected, over every
 *  texel that is the same colour as the seed. */
export function fillSprite(sprite: Sprite, frame: number, col: number, row: number, hsv: Hsv | null): Sprite {
  const { w, h } = sprite.tag;
  if (col < 0 || row < 0 || col >= w || row >= h) return sprite;
  const seed = spriteIndex(sprite.tag, frame, col, row);
  const target = hsvKey([sprite.h[seed] ?? 0, sprite.s[seed] ?? 0, sprite.v[seed] ?? 0]);
  const paint: Hsv = hsv ?? [0, 0, 0];
  if (target === hsvKey(paint)) return sprite;
  const next: Sprite = { tag: sprite.tag, h: [...sprite.h], s: [...sprite.s], v: [...sprite.v] };
  const stack: [number, number][] = [[col, row]];
  const done = new Set<number>();
  while (stack.length > 0) {
    const cell = stack.pop();
    if (!cell) break;
    const [c, r] = cell;
    if (c < 0 || r < 0 || c >= w || r >= h) continue;
    const i = spriteIndex(sprite.tag, frame, c, r);
    if (done.has(i)) continue;
    done.add(i);
    if (hsvKey([next.h[i] ?? 0, next.s[i] ?? 0, next.v[i] ?? 0]) !== target) continue;
    next.h[i] = paint[0];
    next.s[i] = paint[1];
    next.v[i] = paint[2];
    stack.push([c + 1, r], [c - 1, r], [c, r + 1], [c, r - 1]);
  }
  return next;
}

/**
 * The canonical sprite pattern for `sprite`: the tag line, the three arrays,
 * and a `renderFrame` body that plays it ALONE — so a sprite opened in the
 * pattern editor, put in a playlist or pushed to a bare device behaves like
 * any other pattern (docs/spec/scenes.md §4).
 *
 * The single-frame body is one `blit` call. A multi-frame sprite copies the
 * frame it is on into three scratch arrays first, because `blit` takes no
 * offset into its source arrays — the compositor does its own frame cycling
 * natively and never runs this body at all.
 */
export function emitSprite(sprite: Sprite, name?: string): string {
  const { w, h, frames, fps } = sprite.tag;
  const lines: string[] = [];
  lines.push(`// @sprite w=${w} h=${h} frames=${frames} fps=${fps}`);
  if (name) lines.push(`// ${name}`);
  lines.push("");
  lines.push(`var sprH = [${sprite.h.map(num).join(", ")}]`);
  lines.push(`var sprS = [${sprite.s.map(num).join(", ")}]`);
  lines.push(`var sprV = [${sprite.v.map(num).join(", ")}]`);
  lines.push("");
  lines.push(`var sprW = ${w}`);
  lines.push(`var sprHt = ${h}`);
  if (frames > 1) {
    lines.push(`var sprFrames = ${frames}`);
    lines.push(`var sprFps = ${fps}`);
    lines.push("var sprT = 0");
    lines.push("var sprBase = 0");
    lines.push("var sprI = 0");
    lines.push("var frH = array(sprW * sprHt)");
    lines.push("var frS = array(sprW * sprHt)");
    lines.push("var frV = array(sprW * sprHt)");
    lines.push("");
    lines.push("export function beforeRender(delta) {");
    lines.push("  sprT = sprT + delta / 1000");
    lines.push("  sprBase = mod(floor(sprT * sprFps), sprFrames) * sprW * sprHt");
    lines.push("  for (sprI = 0; sprI < sprW * sprHt; sprI++) {");
    lines.push("    frH[sprI] = sprH[sprBase + sprI]");
    lines.push("    frS[sprI] = sprS[sprBase + sprI]");
    lines.push("    frV[sprI] = sprV[sprBase + sprI]");
    lines.push("  }");
    lines.push("}");
    lines.push("");
    lines.push("export function renderFrame() {");
    lines.push("  blit(frH, frS, frV, sprW, sprHt, 0, 0, 3)");
    lines.push("}");
  } else {
    lines.push("");
    lines.push("export function renderFrame() {");
    lines.push("  blit(sprH, sprS, sprV, sprW, sprHt, 0, 0, 3)");
    lines.push("}");
  }
  return lines.join("\n") + "\n";
}

// ---- helpers ----

/** Four decimals, no trailing zeroes — small enough that a 64×64 sprite's
 *  three arrays stay well inside the store, exact enough that HSV round trips
 *  through `hsvToRgb` unchanged at 8-bit output. */
function num(v: number): string {
  if (!Number.isFinite(v)) return "0";
  const r = Math.round(v * 10000) / 10000;
  return String(r);
}

function clampInt(v: number, lo: number, hi: number): number {
  const n = Math.round(v);
  if (!Number.isFinite(n)) return lo;
  return n < lo ? lo : n > hi ? hi : n;
}

/** The three drawing tools of the sprite tool row (S7c). */
export type SpriteTool = "pencil" | "eraser" | "fill";
