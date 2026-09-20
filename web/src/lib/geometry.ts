// The Layout vocabulary and the pure reconciler behind `stores/geometry.ts`
// (Gitea #463, proposal §0–§2).
//
// ONE Layout owns geometry. It is reconciled from three inputs and nothing
// else:
//
//   device Layout (console)  ×  "Preview as" choice (playground)  ×  the
//   compiled pattern's own dimensionality (`Engine.preferredDims()`)
//
// and every preview, tile, thumbnail and playlist row renders through the
// result. This module is deliberately free of Svelte, of `fetch` and of the
// wasm binding so the derivation can be unit-tested directly
// (`web/tests/geometry.test.mjs`) — the store is only the wiring.
//
// It also owns the projection vocabulary (`lib/luxel.ts` re-exports it), so
// the tables below are the TypeScript mirror of
// `crates/luxel-core/src/projection.rs` (docs/spec/projection.md §1). The
// engine stays the authority at run time — `Luxel.projectionOptions()` reads
// the same tables out of wasm, and `web/tests/geometry.test.mjs` asserts the
// two agree cell by cell, so a change on either side fails there.

// ---- projection (mirrors luxel_core::projection) ----

/** A projection wire token (`luxel_core::projection::ProjectionMode`). */
export type ProjectionMode = "index" | "x" | "y" | "z" | "xy" | "xz" | "yz";

/** FFI code per token — the numbers the engine, firmware and mirror share. */
export const PROJECTION_CODES: Record<ProjectionMode, number> = {
  index: 0,
  x: 1,
  y: 2,
  z: 3,
  xy: 4,
  xz: 5,
  yz: 6,
};

/** A device's projection defaults: one choice per PATTERN dimensionality.
 *  Which of them is in play is decided by the Layout. Wire names match
 *  `/api/layout` (#465). */
export interface Projection {
  proj1d: ProjectionMode;
  proj2d: ProjectionMode;
  proj3d: ProjectionMode;
}

/** The engine's own defaults — a no-op on every (pattern, Layout) pair. */
export const DEFAULT_PROJECTION: Projection = { proj1d: "index", proj2d: "z", proj3d: "xy" };

/** A Layout's dimensionality. */
export type Dims = 1 | 2 | 3;

/** What a compiled pattern asks for: `Engine.preferredDims()`'s 0 (no
 *  preference — a `renderFrame` in index space) reads as 1. */
export type PatternDims = 0 | 1 | 2 | 3;

/** Normalize a dimensionality the way `luxel_core::projection::dims` does. */
export function normDims(d: number): Dims {
  return d >= 3 ? 3 : d === 2 ? 2 : 1;
}

const OPT_1_ON_2: ProjectionMode[] = ["index", "x", "y"];
const OPT_1_ON_3: ProjectionMode[] = ["index", "x", "y", "z"];
const OPT_2_ON_3: ProjectionMode[] = ["z", "y", "x"];

/** Whether this Layout can show a pattern of `patternDims` at all (#538): a
 *  Layout shows its own dimensionality and lower, never higher. A host never
 *  OFFERS an incompatible pattern, but the engine still renders one it is
 *  handed (an old playlist entry, a share link, HA) — `/api/status`'s
 *  `geom.compatible` and `EffectiveGeometry.compatible` report this. */
export function projectionCompatible(patternDims: number, layoutDims: number): boolean {
  return normDims(patternDims) <= normDims(layoutDims);
}

/** The projection choices that mean anything for a pattern of `patternDims`
 *  on a Layout of `layoutDims` — one row of the §5.4d table, display order,
 *  first = default. Empty when there is nothing to project: the pattern is
 *  native to the Layout, or it is incompatible with it. */
export function projectionOptions(patternDims: number, layoutDims: number): ProjectionMode[] {
  const pd = normDims(patternDims);
  const ld = normDims(layoutDims);
  if (pd === 1 && ld === 2) return OPT_1_ON_2;
  if (pd === 1 && ld === 3) return OPT_1_ON_3;
  if (pd === 2 && ld === 3) return OPT_2_ON_3;
  return [];
}

/** The human label for one cell (`Along x`, `Repeat along z`, …) —
 *  `Native` for a pair with no choice. */
export function projectionLabel(
  mode: ProjectionMode,
  patternDims: number,
  layoutDims: number,
): string {
  const pd = normDims(patternDims);
  const ld = normDims(layoutDims);
  if (pd === 1 && ld !== 1) {
    if (mode === "index") return "By index";
    if (mode === "x") return "Along x";
    if (mode === "y") return "Along y";
    if (mode === "z" && ld === 3) return "Along z";
  }
  if (pd === 2 && ld === 3) {
    if (mode === "x") return "Repeat along x";
    if (mode === "y") return "Repeat along y";
    if (mode === "z") return "Repeat along z";
  }
  return "Native";
}

/** The mode actually in force for this pair: null when there is nothing to
 *  project (the pattern is native to the Layout, or incompatible with it),
 *  otherwise the stored choice — or the pair's first option when the stored
 *  choice is not one this pair offers. */
export function effectiveProjection(
  p: Projection,
  patternDims: number,
  layoutDims: number,
): ProjectionMode | null {
  const opts = projectionOptions(patternDims, layoutDims);
  const first = opts[0];
  if (first === undefined) return null;
  const want = normDims(patternDims) === 3 ? p.proj3d : normDims(patternDims) === 2 ? p.proj2d : p.proj1d;
  return opts.includes(want) ? want : first;
}

// ---- the Layout ----

/** Where the reconciled Layout came from — what the UI says about it, and
 *  which surface owns changing it.
 *   * `device` — the console: the hardware's own geometry (`/api/status`
 *     `geom`, later `/api/layout`).
 *   * `user` — an explicit playground "Preview as" choice.
 *   * `pattern` — playground Auto, following the compiled pattern's dims.
 *   * `default` — nothing said anything: the 60 px starter strip. */
export type LayoutSource = "device" | "user" | "pattern" | "default";

/** The ONE geometry object. Everything that renders pixels takes this. */
export interface Layout {
  /** 1 strip · 2 matrix or 2D map · 3 lattice or 3D map. */
  dims: Dims;
  /** Addressable as a `w`×`h`(×`d`) lattice (a strip is 1D-regular). False
   *  for an irregular coordinate cloud — what gates Scenes/text later. */
  regular: boolean;
  source: LayoutSource;
  /** The lattice. `h`/`d` are 1 for a strip, `d` is 1 for a matrix. Both 0
   *  when `regular` is false. */
  w: number;
  h: number;
  d: number;
  /** Pixels the Layout addresses — what an engine is compiled at. */
  pixels: number;
  /** One [x,y] or [x,y,z] per pixel, when the renderer needs positions: a
   *  3D lattice, a custom map, or a serpentine console preview. Absent for a
   *  strip and for a row-major grid, which are drawn from `w`/`h` alone. */
  coords?: number[][];
  /** The device's real wiring walks alternate rows backwards. Undefined =
   *  row-major, which is what the playground always assumes (§4) and what
   *  the console falls back to until `/api/layout` reports it (#465). */
  serpentine?: boolean;
  /** How a pattern of another dimensionality is shown here (§5.4d). */
  projection: Projection;
}

/** The shape a preview, tile or thumbnail draws (proposal §2 visibility
 *  table): a bar on a strip, a grid on a matrix, a rotating cloud in 3D, a
 *  scatter for an irregular 2D map. */
export type TileShape = "bar" | "grid" | "cloud" | "scatter";

export function tileShape(l: Layout): TileShape {
  if (l.dims === 1) return "bar";
  if (l.dims === 3) return "cloud";
  return l.regular ? "grid" : "scatter";
}

/** The playground's Layout choice — the "Preview as" chip (§4). `auto`
 *  follows the pattern (3D › 2D › 1D, decision D7); everything else is the
 *  user's, and outlives pattern loads. Shape only: no wiring (that is
 *  hardware, and lives in the console's LED layout settings). */
export type PreviewAs =
  | { mode: "auto" }
  | { mode: "strip"; pixels: number }
  | { mode: "matrix"; w: number; h: number }
  | { mode: "lattice"; n: number }
  /** The custom map program; `pixels` is how many points it plots. */
  | { mode: "map"; pixels: number };

export const DEFAULT_PREVIEW_AS: PreviewAs = { mode: "auto" };

/** The playground's starter strip, and the rigs Auto picks for a pattern
 *  that wants more than one dimension. */
export const DEFAULT_STRIP_PIXELS = 60;
export const AUTO_MATRIX = 16;
export const AUTO_LATTICE = 5;

/** What the device says its geometry is: `/api/status`'s `geom` (#464)
 *  today, `/api/layout` (#465) when it lands — one adapter fills this in, so
 *  nothing downstream knows which endpoint answered. */
export interface DeviceGeom {
  dims: Dims;
  regular: boolean;
  w: number;
  h: number;
  source: "user" | "board" | "default";
  /** Pixels the hardware drives (`/api/status` `pixels`). */
  pixels: number;
  /** Coordinates, when the device's map is irregular AND they are known —
   *  a map this session installed, or `/api/layout`'s embedded map (#465).
   *  `GET /api/map` reports only a count, so this can be absent. */
  coords?: number[][];
  /** Alternate rows run backwards (#465 reports it; undefined = row-major). */
  serpentine?: boolean;
}

/** Everything the reconciler reads. Assembled by `stores/geometry.ts` from
 *  `stores/device.ts` (wire state), the "Preview as" choice and the compiled
 *  pattern. */
export interface GeometryInput {
  /** A device session is live — the console. */
  connected: boolean;
  /** The device's Layout, when it has told us. */
  geom: DeviceGeom | null;
  /** The playground's chip (and, until A8, the console's legacy shape
   *  override in the editor's playback bar). */
  previewAs: PreviewAs;
  /** `Engine.preferredDims()` of the pattern being shown. 0 = no preference. */
  patternDims: PatternDims;
  /** Coordinates from the custom map program, when one has been run. */
  mapCoords: number[][] | null;
  /** The projection defaults in force (device defaults, later per-item). */
  projection: Projection;
}

function latticeSide(n: number): number {
  return Math.max(2, Math.min(32, Math.round(n) || AUTO_LATTICE));
}

/** `n`×`n`×`n` lattice coordinates, row-major in x, then y, then z — the rig
 *  a 3D pattern previews on. */
export function latticeCoords(w: number, h: number, d: number): number[][] {
  const coords: number[][] = [];
  for (let z = 0; z < d; z++)
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) coords.push([x, y, z]);
  return coords;
}

/** The index→coordinate map of a `w`×`h` grid whose wiring snakes: row 0 runs
 *  left→right, row 1 right→left, and so on. This is what makes the console's
 *  preview show what the panel shows for a by-index 1D pattern (§4); the
 *  playground never uses it. */
export function serpentineCoords(w: number, h: number): number[][] {
  const coords: number[][] = [];
  for (let y = 0; y < h; y++) {
    for (let i = 0; i < w; i++) coords.push([y % 2 === 0 ? i : w - 1 - i, y]);
  }
  return coords;
}

/** The coordinates an engine must be given for this Layout, or null when the
 *  Layout is a plain strip / row-major grid the engine can build itself. */
export function wiringCoords(l: Layout): number[][] | null {
  if (l.dims === 2 && l.regular && l.serpentine === true) return serpentineCoords(l.w, l.h);
  return l.coords ?? null;
}

function strip(pixels: number, source: LayoutSource, projection: Projection): Layout {
  const px = Math.max(1, Math.round(pixels) || 1);
  return { dims: 1, regular: true, source, w: px, h: 1, d: 1, pixels: px, projection };
}

function matrix(w: number, h: number, source: LayoutSource, projection: Projection): Layout {
  const cw = Math.max(1, Math.round(w) || 1);
  const ch = Math.max(1, Math.round(h) || 1);
  return { dims: 2, regular: true, source, w: cw, h: ch, d: 1, pixels: cw * ch, projection };
}

function lattice(n: number, source: LayoutSource, projection: Projection): Layout {
  const s = latticeSide(n);
  return {
    dims: 3,
    regular: true,
    source,
    w: s,
    h: s,
    d: s,
    pixels: s * s * s,
    coords: latticeCoords(s, s, s),
    projection,
  };
}

function cloud(coords: number[][], source: LayoutSource, projection: Projection): Layout {
  const dims: Dims = (coords[0]?.length ?? 2) >= 3 ? 3 : 2;
  return {
    dims,
    regular: false,
    source,
    w: 0,
    h: 0,
    d: 0,
    pixels: coords.length,
    coords,
    projection,
  };
}

/** A Layout that is nothing but coordinates — what the map program's own
 *  scatter/cloud preview draws through (A10, #471). The reconciler builds the
 *  same shape for an installed custom map. */
export function cloudLayout(coords: number[][], projection: Projection = DEFAULT_PROJECTION): Layout {
  return cloud(coords, "user", projection);
}

/** The Layout the device itself is rendering through. */
function fromDevice(g: DeviceGeom, projection: Projection): Layout {
  if (g.dims === 1) {
    const l = strip(g.pixels || g.w, "device", projection);
    l.serpentine = g.serpentine;
    return l;
  }
  if (!g.regular) {
    // An irregular device map. Its coordinates are only known when this
    // session installed them or `/api/layout` embedded them (#465) — without
    // them the shape is honest but positionless, and the renderers fall back
    // to index order.
    const l = g.coords
      ? cloud(g.coords, "device", projection)
      : {
          dims: g.dims,
          regular: false,
          source: "device" as const,
          w: 0,
          h: 0,
          d: 0,
          pixels: g.pixels,
          projection,
        };
    return l;
  }
  if (g.dims === 3) {
    const l = lattice(Math.round(Math.cbrt(g.pixels)) || AUTO_LATTICE, "device", projection);
    return l;
  }
  const l = matrix(g.w, g.h, "device", projection);
  l.serpentine = g.serpentine;
  // The device's pixel count is hardware truth even when its grid does not
  // multiply out to it (a truncated map).
  l.pixels = g.pixels || l.pixels;
  return l;
}

/** The ONE reconciler: device Layout × "Preview as" × the pattern's dims.
 *
 *  Console (`connected` with a `geom`): the DEVICE owns the geometry, full
 *  stop — "Preview as" is not consulted at all.
 *  Playground: Auto follows the compiled pattern (3D › 2D › 1D, D7);
 *  anything else is exactly what the user asked for. */
export function reconcileLayout(i: GeometryInput): Layout {
  const p = i.projection;
  const choice = i.previewAs;

  // A console renders through the device's own Layout and nothing else.
  // "Preview as" is the PLAYGROUND's control — since A8 (#469) the chip is
  // only mounted there — but the choice is persisted in localStorage, so one
  // left behind by a playground session on the same origin, or written by the
  // pre-v2 editor's layout select (which used this same key), outlived the
  // device and silently re-shaped the console. A stored `map` choice was the
  // worst of it: `mapCoords` is never persisted, so the fall-through below
  // reconciled a 64x64 panel down to a 4096 px strip — every pattern drawn in
  // 1D and Settings offering the projections of a strip (Gitea #539).
  if (i.connected && i.geom !== null) return fromDevice(i.geom, p);

  if (choice.mode === "auto") {
    const pd = normDims(i.patternDims);
    if (pd === 3) return lattice(AUTO_LATTICE, "pattern", p);
    if (pd === 2) return matrix(AUTO_MATRIX, AUTO_MATRIX, "pattern", p);
    return strip(DEFAULT_STRIP_PIXELS, "default", p);
  }
  if (choice.mode === "strip") return strip(choice.pixels, "user", p);
  if (choice.mode === "matrix") return matrix(choice.w, choice.h, "user", p);
  if (choice.mode === "lattice") return lattice(choice.n, "user", p);
  // Custom map program: its computed coordinates ARE the Layout. Until it has
  // run there is nothing to draw, so the rig stays a strip of the same size.
  if (i.mapCoords && i.mapCoords.length > 0) return cloud(i.mapCoords, "user", p);
  return strip(choice.pixels, "user", p);
}

/** Pixels this Layout addresses — what an engine is compiled at. */
export function pixelCount(l: Layout): number {
  return l.pixels;
}

/** "64×64 matrix" · "300 px strip" · "5×5×5 lattice" · "125 px custom map" —
 *  the header chip and the "Preview as" button label. */
export function layoutLabel(l: Layout): string {
  if (l.dims === 1) return `${l.pixels} px strip`;
  if (!l.regular) return `${l.pixels} px custom map`;
  if (l.dims === 3) return `${l.w}×${l.h}×${l.d} lattice`;
  return `${l.w}×${l.h} matrix`;
}

/** What the pattern actually sees on this Layout once the projection is
 *  applied — the pure twin of `Engine.effectiveGeometry()`, for the surfaces
 *  that caption a pattern without holding an engine for it. */
export interface Effective {
  patternDims: Dims;
  layoutDims: Dims;
  /** null when the pattern is native to the Layout — and also when it is
   *  incompatible with it, which has no projection to be in force. */
  mode: ProjectionMode | null;
  label: string | null;
  /** False when the Layout cannot show a pattern of this dimensionality at
   *  all (#538) — the twin of `/api/status`'s `geom.compatible`. */
  compatible: boolean;
  /** What `pixelCount` reads inside the pattern: the strip length under an
   *  along-axis projection of a 1D pattern, the Layout's count otherwise. */
  pixelCount: number;
  /** The grid the pattern's grid-space builtins see; 0 when there is none. */
  w: number;
  h: number;
}

export function effectiveFor(patternDims: PatternDims, l: Layout): Effective {
  const pd = normDims(patternDims);
  const mode = effectiveProjection(l.projection, pd, l.dims);
  const eff: Effective = {
    patternDims: pd,
    layoutDims: l.dims,
    mode,
    label: mode === null ? null : projectionLabel(mode, pd, l.dims),
    compatible: projectionCompatible(pd, l.dims),
    pixelCount: l.pixels,
    w: l.dims === 2 && l.regular ? l.w : 0,
    h: l.dims === 2 && l.regular ? l.h : 0,
  };
  if (pd === 2 && l.dims === 1) {
    // incompatible, but a grid-space renderFrame still owns the buffer and
    // gets a w×1 grid so its grid-space builtins describe the strip (#538)
    eff.w = l.pixels;
    eff.h = 1;
    return eff;
  }
  if (pd === 1 && mode !== null && mode !== "index" && l.regular) {
    // along-axis: the pattern renders ONE strip, replicated
    eff.pixelCount = mode === "x" ? l.w : mode === "y" ? l.h : l.d;
    eff.w = 0;
    eff.h = 0;
  }
  return eff;
}

/** The dim caption a tile or a row carries when the pattern is not native to
 *  the Layout — `1D · along x` (§5.4d). null when it is native, and null for
 *  an incompatible pattern too: there is no projection to name, and the
 *  surface should be flagging `compatible` instead (#538). */
export function projectionCaption(patternDims: PatternDims, l: Layout): string | null {
  const e = effectiveFor(patternDims, l);
  if (e.label === null) return null;
  return `${e.patternDims}D · ${e.label.toLowerCase()}`;
}

/**
 * The same Layout with ONE projection slot replaced — the per-item override
 * a playlist row (and, at #481, a scene layer) carries beside its control
 * values (§5.4d). `mode` null/undefined gives the Layout back untouched, so
 * "no override" is not a special case at the call site.
 *
 * The slot is chosen by the PATTERN's dimensionality, exactly as
 * `Projection::set` does on the device, so one stored token survives a
 * Layout change without clobbering the user's other choices.
 */
export function withProjectionOverride(
  l: Layout,
  patternDims: PatternDims,
  mode: ProjectionMode | null | undefined,
): Layout {
  if (!mode) return l;
  const pd = normDims(patternDims);
  const field = pd === 3 ? "proj3d" : pd === 2 ? "proj2d" : "proj1d";
  return { ...l, projection: { ...l.projection, [field]: mode } };
}

/** The same Layout, small enough to run dozens of live engines on: a tile or
 *  a thumbnail keeps the Layout's dims, aspect and projection but caps how
 *  many pixels it renders. Irregular maps are left alone — subsampling a
 *  cloud would move its pixels, and a device map is bounded anyway. */
export function thumbLayout(l: Layout, maxCells = 1024): Layout {
  if (l.pixels <= maxCells) return l;
  if (l.dims === 1) return { ...l, w: maxCells, pixels: maxCells };
  if (!l.regular) return l;
  if (l.dims === 3) {
    const s = Math.max(2, Math.floor(Math.cbrt(maxCells)));
    return { ...l, w: s, h: s, d: s, pixels: s * s * s, coords: latticeCoords(s, s, s) };
  }
  const k = Math.sqrt(maxCells / l.pixels);
  const w = Math.max(1, Math.floor(l.w * k));
  const h = Math.max(1, Math.floor(l.h * k));
  return { ...l, w, h, pixels: w * h, coords: undefined };
}

/** Read a persisted "Preview as" choice back, tolerating anything (including
 *  a pre-#463 working copy's `{kind:"grid",w,h}` rig). null = nothing usable,
 *  so the caller keeps the default. */
export function parsePreviewAs(raw: unknown): PreviewAs | null {
  if (typeof raw !== "object" || raw === null) return null;
  const o = raw as Record<string, unknown>;
  const num = (v: unknown, fallback: number): number =>
    typeof v === "number" && Number.isFinite(v) && v > 0 ? Math.round(v) : fallback;
  switch (o.mode ?? o.kind) {
    case "auto":
      return { mode: "auto" };
    case "strip":
      return { mode: "strip", pixels: num(o.pixels, DEFAULT_STRIP_PIXELS) };
    case "matrix":
    case "grid":
      return { mode: "matrix", w: num(o.w, AUTO_MATRIX), h: num(o.h, AUTO_MATRIX) };
    case "lattice":
      return { mode: "lattice", n: num(o.n, AUTO_LATTICE) };
    case "map":
      return {
        mode: "map",
        pixels: num(o.pixels, Array.isArray(o.coords) ? o.coords.length : DEFAULT_STRIP_PIXELS),
      };
    default:
      return null;
  }
}

/** A cheap identity for a Layout: everything that changes how an engine must
 *  be built, without stringifying a 4096-point map every time a poll ticks.
 *  Surfaces compare this to decide whether to rebuild their engines. */
export function layoutKey(l: Layout): string {
  const p = l.projection;
  return [
    l.dims,
    `${l.w}x${l.h}x${l.d}`,
    l.pixels,
    l.regular ? "r" : "-",
    l.serpentine ? "s" : "-",
    l.coords?.length ?? 0,
    `${p.proj1d}/${p.proj2d}/${p.proj3d}`,
  ].join(":");
}
