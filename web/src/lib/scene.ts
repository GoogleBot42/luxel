// The scene record, in TypeScript — a faithful mirror of
// `luxel_core::scene` (crates/luxel-core/src/scene.rs, spec
// docs/spec/scenes.md). The wire format is what `POST /api/scenes` takes and
// what the wasm compositor's `setScene()` parses, so the browser has to be
// able to build it EXACTLY: a stray default emitted, or one omitted, and the
// device stores something the round-trip test in luxel-core says is
// impossible.
//
// Two codecs live here and they are not the same thing:
//   * `parseScene` / `serializeScene` — the WIRE (what is stored and pushed).
//     `serializeScene` omits every field that equals its default, exactly as
//     Rust's `serialize` does, so `parse ∘ serialize` is the identity.
//   * `sceneFromJson` / `sceneJson` — the JSON shape `GET /api/scenes`
//     returns. Control values are decimal there and raw 16.16 on the wire.
//
// Both are pinned against the Rust fixtures in `web/tests/scene.test.mjs`.
// Keep this file pure: no stores, no fetch, no Svelte. (Gitea #480.)

import type { ProjectionMode } from "./geometry";

/** 16.16 fixed point, the scale every raw control value is in. */
const RAW = 65536;

export const MAX_SCENE_NAME = 64;
export const MAX_LAYER_NAME = 32;
export const MAX_RAMP_STOPS = 32;
/** `BLOB_MAX` — the whole scene store is one blob of this many bytes. */
export const SCENES_BLOB_MAX = 3840;

export type LayerKind = "pat" | "text" | "sprite" | "color";
export type Blend = "normal" | "add" | "lighten" | "multiply" | "mask";
export type LayerKey = "none" | "black" | "luma";
export type Fit = "fill" | "contain" | "tile";
export type SceneFont = "tiny" | "regular" | "large";
export type Align = "l" | "c" | "r";
export type Scroll = "none" | "left" | "right" | "up" | "down" | "bounce";
export type ClockFmt = "HH:MM" | "HH:MM:SS" | "hh:MM" | "hh:MM:SS" | "MM-DD" | "YYYY-MM-DD";

export const LAYER_KINDS: readonly LayerKind[] = ["pat", "text", "sprite", "color"];
export const BLENDS: readonly Blend[] = ["normal", "add", "lighten", "multiply", "mask"];
export const KEYS: readonly LayerKey[] = ["none", "black", "luma"];
export const FITS: readonly Fit[] = ["fill", "contain", "tile"];
export const FONTS: readonly SceneFont[] = ["tiny", "regular", "large"];
export const ALIGNS: readonly Align[] = ["l", "c", "r"];
export const SCROLLS: readonly Scroll[] = ["none", "left", "right", "up", "down", "bounce"];
export const CLOCK_FMTS: readonly ClockFmt[] = [
  "HH:MM",
  "HH:MM:SS",
  "hh:MM",
  "hh:MM:SS",
  "MM-DD",
  "YYYY-MM-DD",
];
/** `luxel_core::text::SLOTS`. */
export const TEXT_SLOTS = 8;

/** A layer's box in layout pixels. `w` or `h` = 0 means "the whole layout". */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LayerStyle {
  rect: Rect;
  blend: Blend;
  /** 0..100. */
  opacity: number;
  key: LayerKey;
  fit: Fit;
  visible: boolean;
  flipx: boolean;
  flipy: boolean;
  rot180: boolean;
}

export type TextSource =
  | { kind: "lit"; text: string }
  | { kind: "clock"; fmt: ClockFmt }
  | { kind: "slot"; slot: number };

export interface TextLayer {
  source: TextSource;
  font: SceneFont;
  /** `rrggbb`, lowercase, no `#` — the wire's spelling. */
  color: string;
  align: Align;
  scroll: Scroll;
  /** px/s; only meaningful when `scroll !== "none"`. */
  speed: number;
}

export interface Ramp {
  /** How much of the ramp to apply, 0..100. */
  pct: number;
  /** `[position 0..255, rrggbb]`, ascending by position, 2..32 of them. */
  stops: [number, string][];
}

export interface PatternLayer {
  /** Store pattern id (8 hex), or "" while the layer has no pattern yet. */
  id: string;
  /** name → control values, DECIMAL (raw 16.16 only on the wire). */
  controls: Record<string, number[]>;
  proj: ProjectionMode | null;
  ramp: Ramp | null;
}

export type LayerBody =
  | { kind: "pat"; pat: PatternLayer }
  | { kind: "text"; text: TextLayer }
  /** A sprite layer names a stored SPRITE record by id. Since Gitea #740 that
   *  is a first-class record with its own store and its own id namespace, no
   *  longer a sprite-tagged pattern — the WIRE is untouched (`L sprite` /
   *  `I <id>`), only what the id resolves to moved. Codec: `lib/sprite.ts`;
   *  store: `stores/sprites.ts`. */
  | { kind: "sprite"; id: string }
  | { kind: "color"; color: string };

export interface Layer {
  name: string;
  style: LayerStyle;
  body: LayerBody;
}

export interface Scene {
  /** 8 hex, or "" for a scene the host has not assigned an id to yet. */
  id: string;
  name: string;
  /** Bottom → top: `layers[0]` is the base, as on the wire. */
  layers: Layer[];
}

export type SceneParse = { ok: true; scene: Scene } | { ok: false; error: string };

// ---- defaults (the values `serializeScene` omits) ----

export const DEFAULT_TEXT: TextLayer = {
  source: { kind: "lit", text: "" },
  font: "regular",
  color: "ffffff",
  align: "l",
  scroll: "none",
  speed: 0,
};

/** The speed a text layer gets the first time a scroll mode is CHOSEN for it.
 *
 *  `DEFAULT_TEXT.speed` is the WIRE default and has to stay 0, because it is
 *  what `serializeScene` omits and what `TextLayer::default()` in luxel-core
 *  parses an absent `F` line as — the two must agree byte for byte. But 0
 *  px/s means a layer that says "scroll left" and then sits perfectly still,
 *  so every freshly chosen scroll mode did nothing at all until the user
 *  happened to drag the Speed slider (Gitea #733). A picker that switches a
 *  layer off `none` must bump a zero speed to this. */
export const DEFAULT_SCROLL_SPEED = 16;

/** The display name a layer carries when no `N` line names it. A pattern
 *  layer's is EMPTY — only the host knows a pattern's name. */
export function defaultLayerName(kind: LayerKind): string {
  return kind === "pat" ? "" : kind === "text" ? "Text" : kind === "sprite" ? "Sprite" : "Color";
}

export function defaultStyle(): LayerStyle {
  return {
    rect: { x: 0, y: 0, w: 0, h: 0 },
    blend: "normal",
    opacity: 100,
    key: "none",
    fit: "fill",
    visible: true,
    flipx: false,
    flipy: false,
    rot180: false,
  };
}

/** A fresh layer of `kind`, everything at its default. */
export function newLayer(kind: LayerKind): Layer {
  const style = defaultStyle();
  if (kind === "pat") return { name: "", style, body: { kind: "pat", pat: newPatternBody() } };
  if (kind === "text")
    return { name: "Text", style, body: { kind: "text", text: { ...DEFAULT_TEXT } } };
  if (kind === "sprite") return { name: "Sprite", style, body: { kind: "sprite", id: "" } };
  return { name: "Color", style, body: { kind: "color", color: "ffffff" } };
}

/**
 * `scene` with `patternId` added as its TOP layer — the `Add to scene ▸`
 * shortcut (proposal §5.4b, mock S2e). Layers are listed bottom → top, so the
 * new one goes last: a full-layout box, normal blend, 100 %, unkeyed, carrying
 * the values the pattern is being shown at.
 *
 * Pure, and it never mutates its input — the caller hands the result to
 * `saveScene` (`stores/scenes.ts`), which is the only thing that writes.
 */
export function withPatternOnTop(
  scene: Scene,
  patternId: string,
  controls: Record<string, number[]> = {},
  proj: ProjectionMode | null = null,
): Scene {
  const layer = newLayer("pat");
  const body = layer.body as { kind: "pat"; pat: PatternLayer };
  body.pat = { ...body.pat, id: patternId, controls: { ...controls }, proj };
  return { ...scene, layers: [...scene.layers, layer] };
}

export function newPatternBody(): PatternLayer {
  return { id: "", controls: {}, proj: null, ramp: null };
}

export function emptyScene(name = "New scene"): Scene {
  return { id: "", name, layers: [] };
}

/** The layer's kind, read off its body — the `L <type>` token. */
export function layerKind(l: Layer): LayerKind {
  return l.body.kind;
}

/** The pattern or sprite id a layer draws, if it has one. */
export function layerPatternId(l: Layer): string {
  if (l.body.kind === "pat") return l.body.pat.id;
  if (l.body.kind === "sprite") return l.body.id;
  return "";
}

/** How many of the scene's layers cost an engine (`caps.layers` counts these
 *  and nothing else — sprites are blitted, text and colour are drawn). */
export function patternLayerCount(s: Scene): number {
  return s.layers.filter((l) => l.body.kind === "pat").length;
}

/** 8 lowercase hex — the id shape both stores use. */
export function validSceneId(s: string): boolean {
  return /^[0-9a-f]{8}$/.test(s);
}

// ---- flags ----

export const FLAG_VISIBLE = 1;
export const FLAG_FLIPX = 2;
export const FLAG_FLIPY = 4;
export const FLAG_ROT180 = 8;

function styleFlags(st: LayerStyle): number {
  return (
    (st.visible ? FLAG_VISIBLE : 0) |
    (st.flipx ? FLAG_FLIPX : 0) |
    (st.flipy ? FLAG_FLIPY : 0) |
    (st.rot180 ? FLAG_ROT180 : 0)
  );
}

// ---- small helpers, all mirroring scene.rs ----

function err(line: number, what: string): string {
  return `scene: line ${line}: ${what}`;
}

function errTok(line: number, what: string, tok: string): string {
  return `${err(line, what)} "${tok}"`;
}

/** The remainder of `line` after `skip` whitespace-separated tokens, with the
 *  separating whitespace removed. `""` when the line is shorter. */
function rest(line: string, skip: number): string {
  let s = line;
  for (let i = 0; i < skip; i++) {
    s = s.replace(/^\s+/, "");
    const m = /\s/.exec(s);
    if (!m) return "";
    s = s.slice(m.index);
  }
  return s.replace(/^\s+/, "");
}

function tokens(line: string): string[] {
  return line.split(/\s+/).filter((t) => t !== "");
}

/** UTF-8 byte length — every length limit on the wire is in bytes. */
export function utf8Len(s: string): number {
  return new TextEncoder().encode(s).length;
}

/** Truncate to `max` UTF-8 bytes on a character boundary. */
export function truncateUtf8(s: string, max: number): string {
  if (utf8Len(s) <= max) return s;
  let out = "";
  for (const ch of s) {
    if (utf8Len(out + ch) > max) break;
    out += ch;
  }
  return out;
}

function parseRgb(s: string): string | null {
  return /^[0-9a-fA-F]{6}$/.test(s) ? s.toLowerCase() : null;
}

function parseIntTok(tok: string, line: number, what: string): number {
  // Rust's `str::parse::<i32>()`: an optional sign then digits, nothing else.
  if (!/^[+-]?\d+$/.test(tok)) throw new ParseError(errTok(line, what, tok));
  const v = Number(tok);
  if (!Number.isSafeInteger(v) || v < -2147483648 || v > 2147483647) {
    throw new ParseError(errTok(line, what, tok));
  }
  return v;
}

class ParseError extends Error {}

function oneOf<T extends string>(list: readonly T[], tok: string): T | null {
  return (list as readonly string[]).includes(tok) ? (tok as T) : null;
}

// ---- the wire: parse ----

/** Parse one scene block. Line numbers are 1-based within `block`. */
export function parseScene(block: string): SceneParse {
  return parseAt(block, 1);
}

/** Parse a whole scene blob — blocks back to back, each starting at its own
 *  `S` line. Error line numbers are blob-global, like `parse_all`'s. */
export function parseScenes(blob: string): { ok: true; scenes: Scene[] } | { ok: false; error: string } {
  const out: Scene[] = [];
  let start: number | null = null;
  let buf = "";
  const lines = splitLines(blob);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const isS = line.replace(/^\s+/, "").startsWith("S ") || line.trim() === "S";
    if (isS) {
      if (start !== null) {
        const r = parseAt(buf, start);
        if (!r.ok) return r;
        out.push(r.scene);
      }
      buf = "";
      start = i + 1;
    }
    if (start !== null) buf += line + "\n";
  }
  if (start !== null) {
    const r = parseAt(buf, start);
    if (!r.ok) return r;
    out.push(r.scene);
  }
  return { ok: true, scenes: out };
}

/** `str::lines()`: split on \n, and a trailing \n yields no extra line. */
function splitLines(s: string): string[] {
  const parts = s.split("\n");
  if (parts.length > 0 && parts[parts.length - 1] === "") parts.pop();
  return parts;
}

function parseAt(block: string, firstLine: number): SceneParse {
  const scene: Scene = { id: "", name: "", layers: [] };
  let seenS = false;
  const lines = splitLines(block);
  try {
    for (let i = 0; i < lines.length; i++) {
      const line = (lines[i] ?? "").replace(/\r+$/, "");
      const n = firstLine + i;
      const tag = tokens(line)[0] ?? "";
      if (tag === "S") {
        if (seenS) throw new ParseError(err(n, "a second S line in one scene block"));
        seenS = true;
        const id = tokens(line)[1] ?? "";
        if (id !== "-" && !validSceneId(id)) throw new ParseError(errTok(n, "bad scene id", id));
        scene.id = id === "-" ? "" : id;
        const name = rest(line, 2);
        if (utf8Len(name) > MAX_SCENE_NAME) {
          throw new ParseError(err(n, "scene name is over 64 bytes"));
        }
        scene.name = name;
      } else if (tag === "L") {
        if (!seenS) throw new ParseError(err(n, "an L line before the S line"));
        scene.layers.push(parseLayer(line, n));
      } else if (tag === "") {
        // blank line
      } else {
        if (!seenS) throw new ParseError(err(n, "a binding line before the S line"));
        // Binding lines attach to the most recent L. One that arrives before
        // any layer, or that does not apply to the layer's type, reads as an
        // unknown line: ignored.
        const layer = scene.layers[scene.layers.length - 1];
        if (layer) parseBinding(layer, tag, line, n);
      }
    }
    if (!seenS) throw new ParseError(err(firstLine, "expected an S line"));
  } catch (e) {
    if (e instanceof ParseError) return { ok: false, error: e.message };
    throw e;
  }
  return { ok: true, scene };
}

function parseLayer(line: string, n: number): Layer {
  const t = tokens(line);
  let at = 1;
  const next = (what: string): string => {
    const v = t[at++];
    if (v === undefined) throw new ParseError(err(n, `L needs 10 fields, missing ${what}`));
    return v;
  };
  const kindTok = next("type");
  const kind = oneOf(LAYER_KINDS, kindTok);
  if (!kind) throw new ParseError(errTok(n, "unknown layer type", kindTok));
  const x = parseIntTok(next("x"), n, "x is not an integer");
  const y = parseIntTok(next("y"), n, "y is not an integer");
  const w = parseIntTok(next("w"), n, "w is not an integer");
  const h = parseIntTok(next("h"), n, "h is not an integer");
  const blendTok = next("blend");
  const blend = oneOf(BLENDS, blendTok);
  if (!blend) throw new ParseError(errTok(n, "unknown blend", blendTok));
  const opacityTok = next("opacity");
  const opacity = parseIntTok(opacityTok, n, "opacity is not an integer");
  if (opacity < 0 || opacity > 100) {
    throw new ParseError(errTok(n, "opacity must be 0..100", opacityTok));
  }
  const keyTok = next("key");
  const key = oneOf(KEYS, keyTok);
  if (!key) throw new ParseError(errTok(n, "unknown key", keyTok));
  const fitTok = next("fit");
  const fit = oneOf(FITS, fitTok);
  if (!fit) throw new ParseError(errTok(n, "unknown fit", fitTok));
  const flagsTok = next("flags");
  const flags = parseIntTok(flagsTok, n, "flags is not an integer");
  if (flags < 0 || flags > 15) throw new ParseError(errTok(n, "flags must be 0..15", flagsTok));
  if (x < -32768 || x > 32767 || y < -32768 || y > 32767) {
    throw new ParseError(err(n, "x and y must fit a signed 16-bit integer"));
  }
  if (w < 0 || w > 65535 || h < 0 || h > 65535) {
    throw new ParseError(err(n, "w and h must be 0..65535"));
  }
  const style: LayerStyle = {
    rect: { x, y, w, h },
    blend,
    opacity,
    key,
    fit,
    visible: (flags & FLAG_VISIBLE) !== 0,
    flipx: (flags & FLAG_FLIPX) !== 0,
    flipy: (flags & FLAG_FLIPY) !== 0,
    rot180: (flags & FLAG_ROT180) !== 0,
  };
  const body: LayerBody =
    kind === "pat"
      ? { kind: "pat", pat: newPatternBody() }
      : kind === "text"
        ? { kind: "text", text: { ...DEFAULT_TEXT } }
        : kind === "sprite"
          ? { kind: "sprite", id: "" }
          : { kind: "color", color: "000000" };
  return { name: defaultLayerName(kind), style, body };
}

function parseBinding(layer: Layer, tag: string, line: string, n: number): void {
  const t = tokens(line);
  switch (tag) {
    case "N": {
      const name = rest(line, 1);
      if (utf8Len(name) > MAX_LAYER_NAME) {
        throw new ParseError(err(n, "layer name is over 32 bytes"));
      }
      layer.name = name;
      break;
    }
    case "I": {
      const id = t[1] ?? "";
      if (!validSceneId(id)) throw new ParseError(errTok(n, "bad pattern id", id));
      if (layer.body.kind === "pat") layer.body.pat.id = id;
      else if (layer.body.kind === "sprite") layer.body.id = id;
      break;
    }
    case "C": {
      const name = t[1];
      if (layer.body.kind === "pat" && name !== undefined) {
        const raw = t
          .slice(2)
          .map((tok) => parseIntTok(tok, n, "control values are raw 16.16 integers"));
        layer.body.pat.controls[name] = raw.map((v) => v / RAW);
      }
      break;
    }
    case "P": {
      const tok = t[1] ?? "";
      const mode = oneOf<ProjectionMode>(["index", "x", "y", "z", "xy", "xz", "yz"], tok);
      if (!mode) throw new ParseError(errTok(n, "unknown projection", tok));
      if (layer.body.kind === "pat") layer.body.pat.proj = mode;
      break;
    }
    case "R": {
      const pctTok = t[1] ?? "";
      const pct = parseIntTok(pctTok, n, "ramp amount is not an integer");
      if (pct < 0 || pct > 100) throw new ParseError(errTok(n, "ramp amount must be 0..100", pctTok));
      const stops: [number, string][] = [];
      for (const tok of t.slice(2)) {
        const at = tok.indexOf(":");
        if (at === -1) throw new ParseError(errTok(n, "a ramp stop is <pos>:<rrggbb>", tok));
        const pos = parseIntTok(tok.slice(0, at), n, "a ramp stop position is 0..255");
        if (pos < 0 || pos > 255) {
          throw new ParseError(errTok(n, "a ramp stop position is 0..255", tok));
        }
        const rgb = parseRgb(tok.slice(at + 1));
        if (rgb === null) throw new ParseError(errTok(n, "a ramp stop colour is rrggbb", tok));
        if (stops.length === MAX_RAMP_STOPS) {
          throw new ParseError(err(n, "too many ramp stops (max 32)"));
        }
        stops.push([pos, rgb]);
      }
      if (stops.length < 2) throw new ParseError(err(n, "a ramp needs at least two stops"));
      for (let i = 1; i < stops.length; i++) {
        if ((stops[i - 1] as [number, string])[0] > (stops[i] as [number, string])[0]) {
          throw new ParseError(err(n, "ramp stops must ascend by position"));
        }
      }
      if (layer.body.kind === "pat") layer.body.pat.ramp = { pct, stops };
      break;
    }
    case "T": {
      const src = t[1] ?? "";
      let source: TextSource;
      if (src === "lit") {
        const text = rest(line, 2);
        if (utf8Len(text) > MAX_SCENE_NAME) throw new ParseError(err(n, "text is over 64 bytes"));
        source = { kind: "lit", text };
      } else if (src === "clock") {
        const f = t[2] ?? "";
        const fmt = oneOf(CLOCK_FMTS, f);
        if (!fmt) throw new ParseError(errTok(n, "unknown clock format", f));
        source = { kind: "clock", fmt };
      } else if (src === "slot") {
        const tok = t[2] ?? "";
        const v = parseIntTok(tok, n, "a text slot is 0..7");
        if (v < 0 || v >= TEXT_SLOTS) throw new ParseError(errTok(n, "a text slot is 0..7", tok));
        source = { kind: "slot", slot: v };
      } else {
        throw new ParseError(errTok(n, "unknown text source", src));
      }
      if (layer.body.kind === "text") layer.body.text.source = source;
      break;
    }
    case "F": {
      const fontTok = t[1] ?? "";
      const font = oneOf(FONTS, fontTok);
      if (!font) throw new ParseError(errTok(n, "unknown font", fontTok));
      const colorTok = t[2] ?? "";
      const color = parseRgb(colorTok);
      if (color === null) throw new ParseError(errTok(n, "text colour is rrggbb", colorTok));
      const alignTok = t[3] ?? "";
      const align = oneOf(ALIGNS, alignTok);
      if (!align) throw new ParseError(errTok(n, "unknown align", alignTok));
      const scrollTok = t[4] ?? "";
      const scroll = oneOf(SCROLLS, scrollTok);
      if (!scroll) throw new ParseError(errTok(n, "unknown scroll", scrollTok));
      const speedTok = t[5] ?? "";
      const speed = parseIntTok(speedTok, n, "scroll speed is not an integer");
      if (speed < 0 || speed > 65535) {
        throw new ParseError(errTok(n, "scroll speed must be 0..65535", speedTok));
      }
      if (layer.body.kind === "text") {
        layer.body.text = { ...layer.body.text, font, color, align, scroll, speed };
      }
      break;
    }
    case "K": {
      const tok = t[1] ?? "";
      const rgb = parseRgb(tok);
      if (rgb === null) throw new ParseError(errTok(n, "wash colour is rrggbb", tok));
      if (layer.body.kind === "color") layer.body.color = rgb;
      break;
    }
    default:
      // A line from a newer console: ignored.
      break;
  }
}

// ---- the wire: serialize ----

/** The wire block for `s`, defaults omitted — byte-identical to Rust's
 *  `scene::serialize`, so a round trip through the device changes nothing. */
export function serializeScene(s: Scene): string {
  let out = `S ${s.id === "" ? "-" : s.id}`;
  if (s.name !== "") out += ` ${s.name}`;
  out += "\n";
  for (const l of s.layers) {
    const st = l.style;
    const kind = layerKind(l);
    out += `L ${kind} ${st.rect.x} ${st.rect.y} ${st.rect.w} ${st.rect.h} ${st.blend} ${st.opacity} ${st.key} ${st.fit} ${styleFlags(st)}\n`;
    if (l.name !== defaultLayerName(kind)) out += `N ${l.name}\n`;
    if (l.body.kind === "pat") {
      const p = l.body.pat;
      if (p.id !== "") out += `I ${p.id}\n`;
      for (const [name, vals] of Object.entries(p.controls)) {
        out += `C ${name}${vals.map((v) => ` ${Math.round(v * RAW) | 0}`).join("")}\n`;
      }
      if (p.proj) out += `P ${p.proj}\n`;
      if (p.ramp) {
        out += `R ${p.ramp.pct}${p.ramp.stops.map(([pos, rgb]) => ` ${pos}:${rgb}`).join("")}\n`;
      }
    } else if (l.body.kind === "text") {
      const t = l.body.text;
      if (t.source.kind === "lit" && t.source.text !== "") out += `T lit ${t.source.text}\n`;
      else if (t.source.kind === "clock") out += `T clock ${t.source.fmt}\n`;
      else if (t.source.kind === "slot") out += `T slot ${t.source.slot}\n`;
      // `F` is emitted only when the STYLE differs from the default — the
      // source is already on its own line (Rust compares a TextLayer that
      // keeps this one's source against `TextLayer::default()`).
      if (
        t.font !== DEFAULT_TEXT.font ||
        t.color !== DEFAULT_TEXT.color ||
        t.align !== DEFAULT_TEXT.align ||
        t.scroll !== DEFAULT_TEXT.scroll ||
        t.speed !== DEFAULT_TEXT.speed
      ) {
        out += `F ${t.font} ${t.color} ${t.align} ${t.scroll} ${t.speed}\n`;
      }
    } else if (l.body.kind === "sprite") {
      if (l.body.id !== "") out += `I ${l.body.id}\n`;
    } else if (l.body.color !== "000000") {
      out += `K ${l.body.color}\n`;
    }
  }
  return out;
}

/** Every scene, concatenated — the `SCENES_KEY` blob's shape, and what the
 *  playground persists. */
export function serializeScenes(list: readonly Scene[]): string {
  return list.map(serializeScene).join("");
}

// ---- the JSON `GET /api/scenes` returns ----

export interface SceneLayerJson {
  type: LayerKind;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  blend: Blend;
  opacity: number;
  key: LayerKey;
  fit: Fit;
  visible: boolean;
  flipx: boolean;
  flipy: boolean;
  rot180: boolean;
  pat?: {
    id: string;
    controls: Record<string, number[]>;
    proj?: ProjectionMode;
    ramp?: { pct: number; stops: [number, string][] };
  };
  text?: {
    source: "lit" | "clock" | "slot";
    text?: string;
    fmt?: ClockFmt;
    slot?: number;
    font: SceneFont;
    color: string;
    align: Align;
    scroll: Scroll;
    speed: number;
  };
  sprite?: { id: string };
  color?: string;
}

export interface SceneJson {
  id: string;
  name: string;
  layers: SceneLayerJson[];
}

/** The `GET /api/scenes` envelope. */
export interface ScenesJson {
  active: string | null;
  layers_max: number;
  used: number;
  max: number;
  scenes: SceneJson[];
}

/** A scene from the device's JSON. Unknown enum tokens fall back to their
 *  defaults rather than throwing — a newer device must not blank the page. */
export function sceneFromJson(j: SceneJson): Scene {
  return {
    id: String(j.id ?? ""),
    name: String(j.name ?? ""),
    layers: (j.layers ?? []).map(layerFromJson),
  };
}

function layerFromJson(j: SceneLayerJson): Layer {
  const kind = oneOf(LAYER_KINDS, String(j.type)) ?? "color";
  const style: LayerStyle = {
    rect: { x: j.x | 0, y: j.y | 0, w: j.w | 0, h: j.h | 0 },
    blend: oneOf(BLENDS, String(j.blend)) ?? "normal",
    opacity: Math.max(0, Math.min(100, j.opacity | 0)),
    key: oneOf(KEYS, String(j.key)) ?? "none",
    fit: oneOf(FITS, String(j.fit)) ?? "fill",
    visible: j.visible !== false,
    flipx: j.flipx === true,
    flipy: j.flipy === true,
    rot180: j.rot180 === true,
  };
  let body: LayerBody;
  if (kind === "pat") {
    const p = j.pat;
    body = {
      kind: "pat",
      pat: {
        id: String(p?.id ?? ""),
        controls: { ...(p?.controls ?? {}) },
        proj: p?.proj ? (oneOf<ProjectionMode>(["index", "x", "y", "z", "xy", "xz", "yz"], p.proj) ?? null) : null,
        ramp: p?.ramp ? { pct: p.ramp.pct | 0, stops: p.ramp.stops.map(([q, c]) => [q | 0, c]) } : null,
      },
    };
  } else if (kind === "text") {
    const t = j.text;
    const source: TextSource =
      t?.source === "clock"
        ? { kind: "clock", fmt: oneOf(CLOCK_FMTS, String(t.fmt)) ?? "HH:MM" }
        : t?.source === "slot"
          ? { kind: "slot", slot: Math.max(0, Math.min(TEXT_SLOTS - 1, t.slot ?? 0)) }
          : { kind: "lit", text: String(t?.text ?? "") };
    body = {
      kind: "text",
      text: {
        source,
        font: oneOf(FONTS, String(t?.font)) ?? "regular",
        color: parseRgb(String(t?.color ?? "")) ?? "ffffff",
        align: oneOf(ALIGNS, String(t?.align)) ?? "l",
        scroll: oneOf(SCROLLS, String(t?.scroll)) ?? "none",
        speed: t?.speed ?? 0,
      },
    };
  } else if (kind === "sprite") {
    body = { kind: "sprite", id: String(j.sprite?.id ?? "") };
  } else {
    body = { kind: "color", color: parseRgb(String(j.color ?? "")) ?? "000000" };
  }
  return { name: String(j.name ?? defaultLayerName(kind)), style, body };
}

/** Exactly the bytes `luxel_core::scene::push_json` writes. Only the tests
 *  and the mirror-free playground path need this, but having it here is what
 *  lets `web/tests/scene.test.mjs` pin the two against one fixture. */
export function sceneJson(s: Scene): string {
  const parts = s.layers.map(layerJson).join(",");
  return `{"id":"${jsonEscape(s.id)}","name":"${jsonEscape(s.name)}","layers":[${parts}]}`;
}

function layerJson(l: Layer): string {
  const st = l.style;
  let o =
    `{"type":"${layerKind(l)}","name":"${jsonEscape(l.name)}",` +
    `"x":${st.rect.x},"y":${st.rect.y},"w":${st.rect.w},"h":${st.rect.h},` +
    `"blend":"${st.blend}","opacity":${st.opacity},"key":"${st.key}","fit":"${st.fit}",` +
    `"visible":${st.visible},"flipx":${st.flipx},"flipy":${st.flipy},"rot180":${st.rot180}`;
  if (l.body.kind === "pat") {
    const p = l.body.pat;
    const ctl = Object.entries(p.controls)
      .map(([k, v]) => `"${jsonEscape(k)}":[${v.map(decStr).join(",")}]`)
      .join(",");
    o += `,"pat":{"id":"${jsonEscape(p.id)}","controls":{${ctl}}`;
    if (p.proj) o += `,"proj":"${p.proj}"`;
    if (p.ramp) {
      const stops = p.ramp.stops.map(([pos, rgb]) => `[${pos},"${rgb}"]`).join(",");
      o += `,"ramp":{"pct":${p.ramp.pct},"stops":[${stops}]}`;
    }
    o += "}";
  } else if (l.body.kind === "text") {
    const t = l.body.text;
    const src =
      t.source.kind === "lit"
        ? `"lit","text":"${jsonEscape(t.source.text)}"`
        : t.source.kind === "clock"
          ? `"clock","fmt":"${t.source.fmt}"`
          : `"slot","slot":${t.source.slot}`;
    o +=
      `,"text":{"source":${src},"font":"${t.font}","color":"${t.color}",` +
      `"align":"${t.align}","scroll":"${t.scroll}","speed":${t.speed}}`;
  } else if (l.body.kind === "sprite") {
    o += `,"sprite":{"id":"${jsonEscape(l.body.id)}"}`;
  } else {
    o += `,"color":"${l.body.color}"`;
  }
  return o + "}";
}

function jsonEscape(s: string): string {
  let out = "";
  for (const c of s) {
    if (c === '"') out += '\\"';
    else if (c === "\\") out += "\\\\";
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else if (c.codePointAt(0)! < 0x20) out += `\\u${c.codePointAt(0)!.toString(16).padStart(4, "0")}`;
    else out += c;
  }
  return out;
}

/** `Fx::dec_str` — the EXACT decimal of a 16.16 value, which is what the
 *  device prints. `0.5` not `0.50000`, and no float formatter anywhere. */
export function decStr(v: number): string {
  const raw = Math.round(v * RAW) | 0;
  const sign = raw < 0 ? "-" : "";
  const mag = Math.abs(raw);
  let out = `${sign}${Math.floor(mag / RAW)}`;
  let frac = mag % RAW;
  if (frac !== 0) {
    out += ".";
    while (frac !== 0) {
      frac *= 10;
      out += String(Math.floor(frac / RAW));
      frac %= RAW;
    }
  }
  return out;
}
