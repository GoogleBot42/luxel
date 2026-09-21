// Geometry: the ONE Layout every preview, tile, thumbnail and playlist row
// renders through (Gitea #463, proposal §0–§2).
//
// There used to be three sources of truth — the preview rig (`layout`), the
// device's pixel count (`devicePixels`) and the installed map (`deviceMap`) —
// combined in one strip→grid-only function (`deriveRig`) once per pattern
// load. This module replaces all of it with a reconciler:
//
//     device Layout (console)          [stores/device.ts `deviceLayout`]
//   × "Preview as" choice (playground) [`previewAs`, persisted]
//   × the compiled pattern's dims      [`patternDims`, from Engine.preferredDims()]
//   × the projection defaults          [`projection`]
//   ────────────────────────────────────────────────────────────────────────
//   = `layout`                          — and NOTHING else is geometry.
//
// `devicePixels` / `deviceMap` stay in `stores/device.ts` as raw wire state
// that only the adapter there reads. Consumers read this file.
//
// The derivation itself is pure and lives in `lib/geometry.ts`, so it is unit
// tested (`web/tests/geometry.test.mjs`) rather than driven through a browser.

import { derived, get, writable, type Readable, type Writable } from "svelte/store";
import {
  AUTO_LATTICE,
  AUTO_MATRIX,
  cloudLayout,
  DEFAULT_PREVIEW_AS,
  DEFAULT_STRIP_PIXELS,
  DEFAULT_PROJECTION,
  effectiveFor,
  guessPatternDims,
  layoutKey,
  layoutLabel,
  pixelCount as layoutPixels,
  projectionCaption,
  projectionCompatible,
  projectionLabel,
  projectionOptions,
  reconcileLayout,
  thumbLayout,
  tileShape,
  wiringCoords,
  withProjectionOverride,
  type Dims,
  type Effective,
  type Layout,
  type PatternDims,
  type PreviewAs,
  type Projection,
  type ProjectionMode,
  type TileShape,
} from "../lib/geometry";
import { Engine, type Diagnostic, type Luxel } from "../lib/luxel";
import { loadPreviewAs, savePreviewAs } from "../lib/store";
import { device, deviceLayout, deviceProjection } from "./device";

export {
  AUTO_LATTICE,
  AUTO_MATRIX,
  cloudLayout,
  DEFAULT_PREVIEW_AS,
  DEFAULT_STRIP_PIXELS,
  effectiveFor,
  guessPatternDims,
  layoutKey,
  layoutLabel,
  projectionCaption,
  projectionCompatible,
  projectionLabel,
  projectionOptions,
  thumbLayout,
  tileShape,
  withProjectionOverride,
};
export type {
  Dims,
  Effective,
  Layout,
  PatternDims,
  PreviewAs,
  Projection,
  ProjectionMode,
  TileShape,
};

// ---- the inputs ----

/** The playground's "Preview as" choice (§4) — the ONE new control this
 *  ticket adds. Persisted like the old rig choice was, so it outlives a
 *  reload AND a pattern load: it is the user's, not the pattern's. It is the
 *  PLAYGROUND's alone: a console renders through the device's Layout and the
 *  reconciler does not consult this at all (Gitea #539). */
export const previewAs: Writable<PreviewAs> = writable(loadPreviewAs() ?? DEFAULT_PREVIEW_AS);
previewAs.subscribe((v) => savePreviewAs(v));

/** What the pattern in the editor DECLARES (`Engine.patternDims()`):
 *  0 = dimensionless (an index-space `renderFrame` — native on every Layout),
 *  1 = `render`, 2 = `render2D` or a grid-space `renderFrame`, 3 = `render3D`.
 *  Auto follows it (D7). Written by the editor after each successful compile
 *  — never parsed out of the source text.
 *
 *  Not `preferredDims()`, which answers "does this pattern want a map" and
 *  folds `render` and `renderFrame` into the same 0: the two pick the same
 *  Auto rig, but only this one tells a 1D pattern (projectable along an axis)
 *  from a dimensionless one (nothing to project). */
export const patternDims: Writable<PatternDims> = writable(0);

/** Coordinates the custom map program produced, when one has been run. Null
 *  until then, which is why "Custom map program" previews as a strip of the
 *  same size until the program runs. */
export const mapCoords: Writable<number[][] | null> = writable(null);

/** The projection defaults in force: the device's while connected (it owns
 *  them — `/api/map`'s `proj*` triple today, `/api/layout` at A4), the
 *  engine's no-op defaults in the playground. Per-item overrides are A9/A12
 *  and layer on top of this. */
export const projection: Readable<Projection> = derived(
  [device, deviceProjection],
  ([d, p]) => (d ? p : DEFAULT_PROJECTION),
);

// ---- the output ----

/** The Layout for a pattern of `dims` — what a gallery tile that is not the
 *  editor's pattern renders through. On the console every pattern gets the
 *  device's shape; in the playground under Auto each one gets its own. */
export function layoutFor(dims: PatternDims): Layout {
  return reconcileLayout({
    connected: get(device) !== null,
    geom: get(deviceLayout),
    previewAs: get(previewAs),
    patternDims: dims,
    mapCoords: get(mapCoords),
    projection: get(projection),
  });
}

/**
 * The Layout the playground's "Auto" would pick for a pattern of `dims` — its
 * OWN shape, with no device consulted (#538). It is what the Patterns page's
 * "Not for this layout" group renders through: a 2D pattern the strip cannot
 * show is drawn as the little matrix it wants to be, not squashed onto a rig
 * that has no room for it.
 */
export function autoLayoutFor(dims: PatternDims): Layout {
  return reconcileLayout({
    connected: false,
    geom: null,
    previewAs: DEFAULT_PREVIEW_AS,
    patternDims: dims,
    mapCoords: null,
    projection: get(projection),
  });
}

/** THE Layout: the one the editor's preview, the header chip and every
 *  consumer that isn't showing some other pattern renders through. */
export const layout: Readable<Layout> = derived(
  [device, deviceLayout, previewAs, patternDims, mapCoords, projection],
  ([d, geom, choice, dims, coords, proj]) =>
    reconcileLayout({
      connected: d !== null,
      geom,
      previewAs: choice,
      patternDims: dims,
      mapCoords: coords,
      projection: proj,
    }),
);

/** Pixels the current Layout addresses — what an engine is compiled at. */
export function pixelCount(l: Layout = get(layout)): number {
  return layoutPixels(l);
}

/** Reactive form of `pixelCount`. */
export const pixelTotal: Readable<number> = derived(layout, (l) => layoutPixels(l));

/** The shape every preview/tile/thumbnail draws (bar · grid · cloud · scatter). */
export const shape: Readable<TileShape> = derived(layout, (l) => tileShape(l));

/** "64×64 matrix" / "300 px strip" — the console header chip and the
 *  "Preview as" button label. */
export const layoutName: Readable<string> = derived(layout, (l) => layoutLabel(l));

/** The dim caption for a pattern that is not native to the current Layout
 *  (`1D · along x`), or null when it is native. */
export function captionFor(dims: PatternDims, l: Layout = get(layout)): string | null {
  return projectionCaption(dims, l);
}

// ---- the one place an engine is told about geometry ----

/**
 * Point `engine` at `l`: its map (or lack of one) and its projection, in that
 * order. EVERY engine in the app goes through here — the editor's preview,
 * the gallery tiles, the row thumbnails — so there is exactly one place that
 * knows an irregular Layout needs `setMap` and a matrix does not.
 *
 * Call it immediately after `compile()`, before the first frame: under an
 * along-axis projection `pixelCount` becomes the strip length, and the
 * pattern's top-level init has already run (docs/spec/projection.md §3).
 */
export function configureEngine(engine: Engine, l: Layout = get(layout)): void {
  const coords = wiringCoords(l);
  if (coords && coords.length > 0) engine.setMap(coords);
  else if (l.dims === 2 && l.regular && l.w > 0 && l.h > 0) engine.setMapGrid(l.w, l.h);
  else engine.setStripLayout();
  engine.setProjection(l.projection);
}

/** Record what the map program computed (it becomes the Layout when
 *  "Custom map program" is the choice). */
export function setMapCoords(coords: number[][] | null): void {
  mapCoords.set(coords);
}

/** Switch the playground's (or, until A8, the console's) Layout choice. */
export function setPreviewAs(choice: PreviewAs): void {
  previewAs.set(choice);
}

/**
 * Compile and run the map program headlessly, and make its coordinates the
 * Layout. The map editor screen (`pages/MapEditor.svelte`, A10/#471) keeps its
 * OWN engine so it can debug and step the program; this is for the boot path,
 * where a pre-#463 share link carried a map program and no screen is open to
 * run it. Returns the point count, 0 when the program did not compile or
 * plotted nothing.
 */
export function runMapProgram(lx: Luxel, src: string, pixels: number): number {
  const eng = lx.compileMap(src, pixels);
  if (!(eng instanceof Engine)) return 0;
  const { coords } = eng.runMap();
  eng.free();
  if (coords.length === 0) return 0;
  setMapCoords(coords);
  setPreviewAs({ mode: "map", pixels: coords.length });
  return coords.length;
}

/** Pixel caps for the small surfaces: a gallery tile and a row thumbnail keep
 *  the Layout's shape but not its size — forty live 64×64 engines is 160 k
 *  render calls a frame, and a 96 px tile cannot show them anyway. */
export const TILE_MAX_CELLS = 1024;
export const THUMB_MAX_CELLS = 400;

/**
 * Compile `src` onto the Layout it will actually be shown on, and configure
 * the engine for it — the one entry point for every surface that renders a
 * pattern it is not editing (gallery tiles, row thumbnails).
 *
 * Two passes only where they are needed: the Layout depends on the pattern's
 * dimensionality under playground Auto, and the only honest source of that is
 * the COMPILED program (`preferredDims`), not a regex over the source. On a
 * console — where the device's shape is the Layout whatever the pattern is —
 * the first compile is always the right one.
 *
 * `maxCells > 0` shrinks the Layout for a thumbnail (see `thumbLayout`).
 * `proj` is a per-item projection override (a playlist row's, §5.4d): it
 * replaces the slot for this pattern's dims, so the thumbnail shows what the
 * device will actually render rather than the device default.
 * `on` renders against a Layout that is NOT the app's: a fixed one for the
 * Settings page's "this is the lattice you are about to install" preview
 * (#538), which by definition is not the shape the device has yet, or a
 * function of the pattern's dims — `autoLayoutFor` for the Patterns page's
 * "Not for this layout" group (#538), which shows each pattern in its own
 * shape instead of the device's.
 * Returns the compiler's Diagnostic when the pattern does not compile.
 */
export function compileForLayout(
  lx: Luxel,
  src: string,
  maxCells = 0,
  proj: ProjectionMode | null = null,
  on: Layout | ((dims: PatternDims) => Layout) | null = null,
): { engine: Engine; layout: Layout; dims: PatternDims } | Diagnostic {
  const shrink = (l: Layout): Layout => (maxCells > 0 ? thumbLayout(l, maxCells) : l);
  const rigFor = (d: PatternDims): Layout =>
    on === null ? layoutFor(d) : typeof on === "function" ? on(d) : on;
  const first = shrink(rigFor(0));
  let engine = lx.compile(src, first.pixels);
  if (!(engine instanceof Engine)) return engine;
  // The DECLARED dims, not `preferredDims()`: both pick the same Auto rig
  // (0 and 1 are the same strip), but the caption, the #538 filter and the
  // projection override downstream need a dimensionless pattern to stay 0.
  const dims = engine.patternDims();
  const want = withProjectionOverride(shrink(rigFor(dims)), dims, proj);
  if (want.pixels !== first.pixels) {
    engine.free();
    const again = lx.compile(src, want.pixels);
    if (!(again instanceof Engine)) return again;
    engine = again;
  }
  configureEngine(engine, want);
  return { engine, layout: want, dims };
}

/** `layoutKey` of the current Layout — what a surface compares to know
 *  whether its engines are still valid. */
export const layoutSignature: Readable<string> = derived(layout, (l) => layoutKey(l));
