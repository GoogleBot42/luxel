// Unit tests for the Layout reconciler (src/lib/geometry.ts) — the ONE
// derivation every preview, tile, thumbnail and playlist row renders through
// (Gitea #463). It is pure on purpose: this file drives every cell of the
// (console shape × pattern dims × "Preview as") table directly, instead of
// inferring it from pixels in a browser.
//
// Run: `npm test` from web/ (node's built-in runner + type stripping, so the
// .ts module is imported directly — no build step, no test dependency).
//
// The projection tables are a TypeScript MIRROR of
// crates/luxel-core/src/projection.rs, so the last test here checks them
// against the engine itself through the built wasm (skipped when
// web/public/luxel.wasm is absent, e.g. a fresh worktree before `npm run
// wasm`; CI always builds it before running the tests).
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  AUTO_LATTICE,
  AUTO_MATRIX,
  DEFAULT_PROJECTION,
  DEFAULT_STRIP_PIXELS,
  effectiveFor,
  latticeCoords,
  layoutKey,
  layoutLabel,
  parsePreviewAs,
  projectionCaption,
  projectionLabel,
  projectionOptions,
  reconcileLayout,
  serpentineCoords,
  thumbLayout,
  tileShape,
  wiringCoords,
  withProjectionOverride,
} from "../src/lib/geometry.ts";

/** A reconciler input with everything at its quiet default. */
function input(over = {}) {
  return {
    connected: false,
    geom: null,
    previewAs: { mode: "auto" },
    patternDims: 0,
    mapCoords: null,
    projection: DEFAULT_PROJECTION,
    ...over,
  };
}

const STRIP_CONSOLE = {
  dims: 1,
  regular: true,
  w: 300,
  h: 1,
  source: "board",
  pixels: 300,
};

const PANEL_CONSOLE = {
  dims: 2,
  regular: true,
  w: 64,
  h: 64,
  source: "board",
  pixels: 4096,
};

// ---- the console: the device owns the geometry ----

test("strip console: every pattern renders on the device's 300 px strip", () => {
  for (const patternDims of [0, 1, 2, 3]) {
    const l = reconcileLayout(input({ connected: true, geom: STRIP_CONSOLE, patternDims }));
    assert.equal(l.dims, 1, `pattern dims ${patternDims}`);
    assert.equal(l.pixels, 300);
    assert.equal(l.source, "device");
    assert.equal(tileShape(l), "bar");
    assert.equal(layoutLabel(l), "300 px strip");
  }
});

test("strip console: a 2D or 3D pattern is incompatible, never captioned (#538)", () => {
  const l = reconcileLayout(input({ connected: true, geom: STRIP_CONSOLE, patternDims: 2 }));
  for (const pd of [2, 3]) {
    assert.equal(projectionCaption(pd, l), null, `${pd}D has no projection to name`);
    assert.equal(effectiveFor(pd, l).compatible, false, `${pd}D on a strip`);
    assert.equal(effectiveFor(pd, l).mode, null);
  }
  for (const pd of [0, 1]) {
    assert.equal(projectionCaption(pd, l), null);
    assert.equal(effectiveFor(pd, l).compatible, true);
  }
});

test("matrix console: every pattern renders on the device's 64×64 grid", () => {
  for (const patternDims of [0, 1, 2, 3]) {
    const l = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE, patternDims }));
    assert.equal(l.dims, 2);
    assert.equal(l.w, 64);
    assert.equal(l.h, 64);
    assert.equal(l.pixels, 4096);
    assert.equal(tileShape(l), "grid");
    assert.equal(layoutLabel(l), "64×64 matrix");
  }
});

test("matrix console: the 1D tile says how it is projected, the 2D one is native", () => {
  const l = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE, patternDims: 1 }));
  assert.equal(projectionCaption(1, l), "1D · by index");
  assert.equal(projectionCaption(2, l), null);
  // #538: a plane never shows a 3D pattern
  assert.equal(projectionCaption(3, l), null);
  assert.equal(effectiveFor(3, l).compatible, false);
  assert.equal(effectiveFor(2, l).compatible, true);
  // by index renders one call per layout pixel; along x renders one strip
  assert.equal(effectiveFor(1, l).pixelCount, 4096);
  const alongX = reconcileLayout(
    input({
      connected: true,
      geom: PANEL_CONSOLE,
      patternDims: 1,
      projection: { ...DEFAULT_PROJECTION, proj1d: "x" },
    }),
  );
  assert.equal(projectionCaption(1, alongX), "1D · along x");
  assert.equal(effectiveFor(1, alongX).pixelCount, 64);
});

test("a custom device map that detects as a grid is a matrix; one that doesn't is a scatter", () => {
  const regular = reconcileLayout(
    input({
      connected: true,
      geom: { dims: 2, regular: true, w: 16, h: 8, source: "user", pixels: 128 },
      patternDims: 2,
    }),
  );
  assert.equal(tileShape(regular), "grid");
  assert.equal(regular.regular, true);

  const coords = [
    [0, 0],
    [1, 0.5],
    [2, 1.7],
    [3, 0.2],
  ];
  const irregular = reconcileLayout(
    input({
      connected: true,
      geom: { dims: 2, regular: false, w: 0, h: 0, source: "user", pixels: 4, coords },
      patternDims: 2,
    }),
  );
  assert.equal(tileShape(irregular), "scatter");
  assert.equal(irregular.pixels, 4);
  assert.deepEqual(irregular.coords, coords);
  assert.equal(layoutLabel(irregular), "4 px custom map");
});

test("a 3D device map is a cloud", () => {
  const coords = latticeCoords(3, 3, 3);
  const l = reconcileLayout(
    input({
      connected: true,
      geom: { dims: 3, regular: false, w: 0, h: 0, source: "user", pixels: 27, coords },
      patternDims: 1,
    }),
  );
  assert.equal(tileShape(l), "cloud");
  assert.equal(l.pixels, 27);
  assert.equal(projectionCaption(1, l), "1D · by index");
});

test("an irregular device map whose coordinates we don't have keeps its dims", () => {
  // /api/map reports a count, not positions (closed by /api/layout, #465)
  const l = reconcileLayout(
    input({
      connected: true,
      geom: { dims: 2, regular: false, w: 0, h: 0, source: "user", pixels: 90 },
      patternDims: 2,
    }),
  );
  assert.equal(l.dims, 2);
  assert.equal(l.regular, false);
  assert.equal(l.coords, undefined);
  assert.equal(l.pixels, 90);
});

// ---- the playground: Auto follows the pattern (D7) ----

test("playground Auto: 3D › 2D › 1D", () => {
  const strip = reconcileLayout(input({ patternDims: 1 }));
  assert.equal(strip.dims, 1);
  assert.equal(strip.pixels, DEFAULT_STRIP_PIXELS);
  assert.equal(strip.source, "default");

  const bulk = reconcileLayout(input({ patternDims: 0 })); // renderFrame, index space
  assert.equal(bulk.dims, 1);

  const grid = reconcileLayout(input({ patternDims: 2 }));
  assert.equal(grid.dims, 2);
  assert.equal(grid.w, AUTO_MATRIX);
  assert.equal(grid.h, AUTO_MATRIX);
  assert.equal(grid.source, "pattern");
  assert.equal(tileShape(grid), "grid");

  const cube = reconcileLayout(input({ patternDims: 3 }));
  assert.equal(cube.dims, 3);
  assert.equal(cube.pixels, AUTO_LATTICE ** 3);
  assert.equal(tileShape(cube), "cloud");
  assert.equal(cube.coords.length, AUTO_LATTICE ** 3);
  // native everywhere: Auto never projects
  for (const [dims, l] of [
    [1, strip],
    [2, grid],
    [3, cube],
  ]) {
    assert.equal(projectionCaption(dims, l), null);
  }
});

test("playground: an explicit choice outranks the pattern and every dims gets it", () => {
  for (const patternDims of [0, 1, 2, 3]) {
    const l = reconcileLayout(
      input({ previewAs: { mode: "matrix", w: 64, h: 64 }, patternDims }),
    );
    assert.equal(l.dims, 2);
    assert.equal(l.pixels, 4096);
    assert.equal(l.source, "user");
  }
  const strip = reconcileLayout(input({ previewAs: { mode: "strip", pixels: 144 } }));
  assert.equal(strip.pixels, 144);
  assert.equal(layoutLabel(strip), "144 px strip");

  const lat = reconcileLayout(input({ previewAs: { mode: "lattice", n: 4 }, patternDims: 1 }));
  assert.equal(lat.dims, 3);
  assert.equal(lat.pixels, 64);
  assert.equal(layoutLabel(lat), "4×4×4 lattice");
  assert.equal(projectionCaption(1, lat), "1D · by index");
});

test("custom map program: a strip of the same size until the program has run", () => {
  const before = reconcileLayout(input({ previewAs: { mode: "map", pixels: 90 } }));
  assert.equal(before.dims, 1);
  assert.equal(before.pixels, 90);
  const coords = Array.from({ length: 90 }, (_, i) => [Math.cos(i), Math.sin(i)]);
  const after = reconcileLayout(
    input({ previewAs: { mode: "map", pixels: 90 }, mapCoords: coords }),
  );
  assert.equal(after.dims, 2);
  assert.equal(after.regular, false);
  assert.equal(after.pixels, 90);
  assert.equal(tileShape(after), "scatter");
});

// A console renders through the device's Layout and NOTHING else (#539): the
// "Preview as" choice is persisted, the chip that sets it is playground-only,
// and a leftover choice used to re-shape the console for good. The `map` case
// is the one Jeremy hit — `mapCoords` is never persisted, so a stored `map`
// choice fell through to a bare strip of the hardware's pixel count and a
// 64x64 panel presented as a 4096 px strip.
for (const [name, choice] of [
  ["strip", { mode: "strip", pixels: 7 }],
  ["matrix", { mode: "matrix", w: 8, h: 8 }],
  ["lattice", { mode: "lattice", n: 5 }],
  ["map (no coordinates)", { mode: "map", pixels: 4096 }],
]) {
  test(`on a console a stored "${name}" Preview-as choice is ignored`, () => {
    const l = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE, previewAs: choice }));
    assert.equal(l.dims, 2);
    assert.equal(l.regular, true);
    assert.equal(l.w, 64);
    assert.equal(l.h, 64);
    assert.equal(l.pixels, 4096);
    assert.equal(l.source, "device");
    assert.equal(layoutLabel(l), "64×64 matrix");
  });
}

test("a stored console choice still shapes the PLAYGROUND", () => {
  const l = reconcileLayout(input({ previewAs: { mode: "strip", pixels: 7 } }));
  assert.equal(l.dims, 1);
  assert.equal(l.pixels, 7);
});

// ---- helpers every consumer leans on ----

test("wiring: row-major by default, serpentine when the device reports it", () => {
  const rowMajor = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE }));
  assert.equal(wiringCoords(rowMajor), null); // the engine builds the grid itself
  const snake = reconcileLayout(
    input({ connected: true, geom: { ...PANEL_CONSOLE, w: 4, h: 2, pixels: 8, serpentine: true } }),
  );
  const coords = wiringCoords(snake);
  assert.equal(coords.length, 8);
  assert.deepEqual(coords.slice(0, 4), [
    [0, 0],
    [1, 0],
    [2, 0],
    [3, 0],
  ]);
  assert.deepEqual(coords.slice(4), [
    [3, 1],
    [2, 1],
    [1, 1],
    [0, 1],
  ]);
  assert.deepEqual(serpentineCoords(2, 2), [
    [0, 0],
    [1, 0],
    [1, 1],
    [0, 1],
  ]);
});

test("thumbnails keep the shape and lose the size", () => {
  const panel = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE }));
  const thumb = thumbLayout(panel, 1024);
  assert.equal(thumb.dims, 2);
  assert.equal(thumb.w, 32);
  assert.equal(thumb.h, 32);
  assert.equal(thumb.pixels, 1024);
  // aspect survives
  const wide = thumbLayout({ ...panel, w: 128, h: 32, pixels: 4096 }, 1024);
  assert.equal(wide.w / wide.h, 4);
  // a strip is capped, a small layout is untouched
  assert.equal(thumbLayout({ ...panel, dims: 1, w: 2048, h: 1, pixels: 2048 }, 400).pixels, 400);
  const small = reconcileLayout(input({ patternDims: 2 }));
  assert.equal(thumbLayout(small, 1024), small);
  // a 3D lattice shrinks to a smaller lattice, coordinates and all
  const cube = reconcileLayout(input({ previewAs: { mode: "lattice", n: 16 } }));
  const cubeThumb = thumbLayout(cube, 1024);
  assert.equal(cubeThumb.pixels, 1000);
  assert.equal(cubeThumb.coords.length, 1000);
});

test("layoutKey changes exactly when an engine would have to be rebuilt", () => {
  const a = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE }));
  const b = reconcileLayout(input({ connected: true, geom: { ...PANEL_CONSOLE } }));
  assert.equal(layoutKey(a), layoutKey(b)); // the 1 Hz poll must not rebuild
  const c = reconcileLayout(input({ connected: true, geom: { ...PANEL_CONSOLE, w: 32, h: 128 } }));
  assert.notEqual(layoutKey(a), layoutKey(c));
  const d = reconcileLayout(
    input({
      connected: true,
      geom: PANEL_CONSOLE,
      projection: { ...DEFAULT_PROJECTION, proj1d: "x" },
    }),
  );
  assert.notEqual(layoutKey(a), layoutKey(d));
});

test("a persisted choice is read back, and a pre-#463 rig migrates", () => {
  assert.deepEqual(parsePreviewAs({ mode: "auto" }), { mode: "auto" });
  assert.deepEqual(parsePreviewAs({ mode: "matrix", w: 8, h: 4 }), { mode: "matrix", w: 8, h: 4 });
  // the old working copy's rig
  assert.deepEqual(parsePreviewAs({ kind: "strip", pixels: 300 }), { mode: "strip", pixels: 300 });
  assert.deepEqual(parsePreviewAs({ kind: "grid", w: 64, h: 64 }), { mode: "matrix", w: 64, h: 64 });
  assert.deepEqual(parsePreviewAs({ kind: "map", coords: [[0, 0], [1, 1]] }), {
    mode: "map",
    pixels: 2,
  });
  assert.equal(parsePreviewAs(null), null);
  assert.equal(parsePreviewAs({ mode: "nonsense" }), null);
  assert.equal(parsePreviewAs("strip"), null);
});

// ---- per-item projection override (Gitea #470) ----

test("withProjectionOverride replaces only the slot for the pattern's dims", () => {
  const l = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE, patternDims: 1 }));
  assert.deepEqual(l.projection, DEFAULT_PROJECTION, "the Layout starts on the defaults");

  const one = withProjectionOverride(l, 1, "x");
  assert.equal(one.projection.proj1d, "x");
  assert.equal(one.projection.proj2d, DEFAULT_PROJECTION.proj2d, "other slots untouched");
  assert.equal(one.projection.proj3d, DEFAULT_PROJECTION.proj3d);
  assert.equal(l.projection.proj1d, DEFAULT_PROJECTION.proj1d, "the input is not mutated");
  assert.equal(one.pixels, l.pixels, "geometry itself is unchanged");

  assert.equal(withProjectionOverride(l, 3, "yz").projection.proj3d, "yz");
  assert.equal(withProjectionOverride(l, 2, "x").projection.proj2d, "x");
  // preferredDims() reports 0 for "no preference"; that is the 1D slot
  assert.equal(withProjectionOverride(l, 0, "y").projection.proj1d, "y");

  // "no override" is not a special case at the call site
  assert.equal(withProjectionOverride(l, 1, null), l);
  assert.equal(withProjectionOverride(l, 1, undefined), l);
});

test("an overridden 1D pattern on a matrix renders as that strip", () => {
  const l = reconcileLayout(input({ connected: true, geom: PANEL_CONSOLE, patternDims: 1 }));
  assert.equal(effectiveFor(1, l).pixelCount, 4096, "by index: one call per pixel");
  const along = withProjectionOverride(l, 1, "x");
  assert.equal(effectiveFor(1, along).pixelCount, 64, "along x: one call per column");
  assert.equal(projectionCaption(1, along), "1D · along x");
});

// ---- parity with the engine ----

test("the projection tables match the engine's, cell by cell", async () => {
  const wasmPath = fileURLToPath(new URL("../public/luxel.wasm", import.meta.url));
  if (!existsSync(wasmPath)) {
    // a worktree that has not run `npm run wasm` yet; CI always has it
    console.log("skipped: web/public/luxel.wasm not built");
    return;
  }
  const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
  const e = instance.exports;
  const response = () =>
    new TextDecoder().decode(
      new Uint8Array(e.memory.buffer, e.lx_response_ptr(), e.lx_response_len()),
    );
  for (let pd = 1; pd <= 3; pd++) {
    for (let ld = 1; ld <= 3; ld++) {
      const n = e.lx_projection_options(pd, ld);
      const fromEngine = n > 0 ? JSON.parse(response()) : [];
      const mine = projectionOptions(pd, ld);
      assert.deepEqual(
        mine,
        fromEngine.map((o) => o.mode),
        `options for a ${pd}D pattern on a ${ld}D layout`,
      );
      assert.deepEqual(
        mine.map((m) => projectionLabel(m, pd, ld)),
        fromEngine.map((o) => o.label),
        `labels for a ${pd}D pattern on a ${ld}D layout`,
      );
    }
  }
});
