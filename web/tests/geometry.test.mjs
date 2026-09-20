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
  cloudLayout,
  deviceGeometry,
  effectiveFor,
  LAYOUT_BODY_BUDGET,
  latticeCoords,
  latticeDimsOf,
  latticeMapFits,
  latticeMapLine,
  maxLatticeSide,
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

// ---- a console's Layout comes from the DEVICE, and from nothing else ----
//
// The invariant (#539, #573): on a console the Layout is the FIXTURE's, so
// nothing the browser holds may shape it. `deviceGeometry` is where that is
// made structural — it takes device readings only — and the table below is
// one case per input that has leaked in, or could.

/** The device readings of a 300 px strip board with no map installed. */
function stripDevice(over = {}) {
  return {
    layout: {
      dims: 1,
      regular: true,
      w: 300,
      h: 1,
      pixels: 300,
      source: "regular",
    },
    status: { dims: 1, regular: true, w: 300, h: 1, source: "board", patternDims: 0 },
    pixels: 300,
    map: { installed: false, dims: 0, count: 0 },
    coords: null,
    ...over,
  };
}

/** What `/api/status` reports once that strip has been handed a `render2D`
 *  program: the engine's FABRICATED ceil(√300)×ceil(300/18) grid, which
 *  describes the running program and no fixture at all. */
const FABRICATED = { dims: 2, regular: true, w: 18, h: 17, source: "default", patternDims: 2 };

test("#573: the engine's fabricated grid never becomes the console's Layout", () => {
  const fixture = deviceGeometry(stripDevice());
  const running2D = deviceGeometry(stripDevice({ status: FABRICATED }));
  assert.deepEqual(running2D, fixture, "a running program does not reshape the fixture");
  assert.equal(layoutLabel(reconcileLayout(input({ connected: true, geom: running2D }))), "300 px strip");
});

test("#573: a strip on firmware with no /api/layout still reads as a strip", () => {
  // the pre-#465 path: `geom` is all there is, and its `source:"default"`
  // reading is dropped for the hardware's own pixel count
  const g = deviceGeometry(stripDevice({ layout: null, status: FABRICATED }));
  assert.equal(g.dims, 1);
  assert.equal(g.pixels, 300);
  assert.equal(g.source, "board");
  assert.equal(layoutLabel(reconcileLayout(input({ connected: true, geom: g }))), "300 px strip");
});

test("#573 reverse: a 64×64 panel handed a 1D program is still a 64×64 panel", () => {
  const panel = {
    layout: { dims: 2, regular: true, w: 64, h: 64, pixels: 4096, source: "regular" },
    status: { dims: 2, regular: true, w: 64, h: 64, source: "board", patternDims: 1 },
    pixels: 4096,
    map: { installed: false, dims: 0, count: 0 },
    coords: null,
  };
  const g = deviceGeometry(panel);
  assert.equal(layoutLabel(reconcileLayout(input({ connected: true, geom: g }))), "64×64 matrix");
  // and a 1D working copy in the editor does not flatten it either
  for (const patternDims of [0, 1, 2, 3]) {
    const l = reconcileLayout(input({ connected: true, geom: g, patternDims }));
    assert.equal(layoutLabel(l), "64×64 matrix", `pattern dims ${patternDims}`);
  }
});

test("#573: /api/layout is the fixture and always wins over /api/status geom", () => {
  // even a geom that claims to BE a fixture (`source:"user"`) loses: the
  // Layout object is the whole geometry, wiring and embedded map included
  const g = deviceGeometry(
    stripDevice({ status: { dims: 2, regular: true, w: 10, h: 30, source: "user", patternDims: 2 } }),
  );
  assert.equal(g.dims, 1);
  assert.equal(g.pixels, 300);
});

test("#573: the console's Layout ignores every browser-held input", () => {
  const geom = deviceGeometry(stripDevice({ status: FABRICATED }));
  const base = { connected: true, geom };
  const want = reconcileLayout(input(base));
  assert.equal(layoutLabel(want), "300 px strip");
  const leaks = {
    // #539: a "Preview as" choice a playground session left in localStorage
    previewAs: [
      { mode: "matrix", w: 18, h: 17 },
      { mode: "lattice", n: 5 },
      { mode: "map", pixels: 90 },
      { mode: "strip", pixels: 60 },
    ],
    // #573: the dimensionality of the working copy resumed at boot
    patternDims: [0, 1, 2, 3],
    // the map program's coordinates (the playground's map editor)
    mapCoords: [null, latticeCoords(3, 3, 3), [[0, 0], [1, 1]]],
  };
  for (const [name, values] of Object.entries(leaks)) {
    for (const v of values) {
      const l = reconcileLayout(input({ ...base, [name]: v }));
      assert.deepEqual(l, want, `${name} = ${JSON.stringify(v)} must not shape a console`);
    }
  }
});

test("#573: a console with no device answer yet falls back to the starter strip", () => {
  // NOT to the pattern's shape and NOT to a persisted playground choice —
  // that window is how #539 got in. `deviceGeometry` returns null only here.
  assert.equal(deviceGeometry({ layout: null, status: null, pixels: 0, map: { installed: false, dims: 0, count: 0 }, coords: null }), null);
  const l = reconcileLayout(
    input({ connected: true, geom: null, patternDims: 2, previewAs: { mode: "matrix", w: 8, h: 8 } }),
  );
  assert.equal(l.dims, 1);
  assert.equal(l.pixels, DEFAULT_STRIP_PIXELS);
  assert.equal(l.source, "default");
});

test("#573: the pre-/api/layout map fallbacks survive the rewrite", () => {
  const grid = deviceGeometry({
    layout: null,
    status: null,
    pixels: 128,
    map: { installed: true, dims: 2, count: 128, kind: "grid", w: 16, h: 8 },
    coords: null,
  });
  assert.deepEqual(grid, { dims: 2, regular: true, w: 16, h: 8, source: "user", pixels: 128 });
  const coords = [
    [0, 0],
    [1, 0.5],
  ];
  const cloudy = deviceGeometry({
    layout: null,
    status: null,
    pixels: 2,
    map: { installed: true, dims: 2, count: 2, kind: "coords" },
    coords,
  });
  assert.equal(cloudy.regular, false);
  assert.deepEqual(cloudy.coords, coords);
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


// ---- the 3D lattice a console can install (Gitea #538) ----

test("latticeMapLine is a `map 3 …` body of lattice INDICES", () => {
  const line = latticeMapLine(2, 2, 2);
  assert.equal(line, "map 3 0 0 0 1 0 0 0 1 0 1 1 0 0 0 1 1 0 1 0 1 1 1 1 1");
});

test("an 8×8×8 lattice fits one POST body; 9³ does not", () => {
  // Indices, not 16.16 fractions, are what makes this fit at all: the same
  // 512 points as fractions of 1.0 are 8,257 bytes.
  assert.equal(latticeMapLine(8, 8, 8).length, 3077);
  assert.ok(latticeMapFits(8, 8, 8));
  assert.ok(!latticeMapFits(9, 9, 9), `9³ is ${latticeMapLine(9, 9, 9).length} bytes`);
  assert.ok(latticeMapLine(8, 8, 8).length <= LAYOUT_BODY_BUDGET);
  assert.equal(maxLatticeSide(), 8, "what the `w × h × d` fields cap each side at");
});

test("latticeDimsOf recognises the lattice this console installed", () => {
  assert.deepEqual(latticeDimsOf(latticeCoords(8, 8, 8)), { w: 8, h: 8, d: 8 });
  assert.deepEqual(latticeDimsOf(latticeCoords(4, 6, 2)), { w: 4, h: 6, d: 2 });
  // …and nothing else
  assert.equal(latticeDimsOf([]), null);
  assert.equal(latticeDimsOf(latticeCoords(8, 8, 8).slice(0, 500)), null, "truncated");
  const scrambled = latticeCoords(4, 4, 4);
  [scrambled[5], scrambled[9]] = [scrambled[9], scrambled[5]];
  assert.equal(latticeDimsOf(scrambled), null, "out of order");
  const ring = Array.from({ length: 16 }, (_, i) => [Math.cos(i), Math.sin(i), 0]);
  assert.equal(latticeDimsOf(ring), null, "a ring is not a lattice");
  assert.equal(latticeDimsOf(latticeCoords(8, 8, 8).map((c) => [c[0], c[1]])), null, "2D");
});

test("a cloud that IS a lattice reports as a regular 3D Layout", () => {
  const l = cloudLayout(latticeCoords(8, 8, 8));
  assert.equal(l.dims, 3);
  assert.equal(l.regular, true);
  assert.equal(l.pixels, 512);
  assert.equal(layoutLabel(l), "8×8×8 lattice", "not `512 px custom map`");
  assert.equal(tileShape(l), "cloud");
  // a 3D cloud that is not a lattice keeps saying so
  const ring = Array.from({ length: 64 }, (_, i) => [Math.cos(i), Math.sin(i), i / 64]);
  assert.equal(cloudLayout(ring).regular, false);
  assert.equal(layoutLabel(cloudLayout(ring)), "64 px custom map");
});

test("a 3D layout offers the 1D and 2D rows, a 1D layout offers none (#538)", () => {
  const l = cloudLayout(latticeCoords(8, 8, 8));
  assert.deepEqual(projectionOptions(1, l.dims), ["index", "x", "y", "z"]);
  assert.deepEqual(projectionOptions(2, l.dims), ["z", "y", "x"]);
  assert.deepEqual(projectionOptions(3, l.dims), [], "native");
  assert.deepEqual(projectionOptions(2, 1), []);
  assert.deepEqual(projectionOptions(3, 1), []);
  assert.deepEqual(projectionOptions(3, 2), []);
});
