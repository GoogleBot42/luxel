// Unit tests for the Settings visibility gating (src/lib/settingsCaps.ts) —
// the rule the whole page obeys: a control is ABSENT unless the device
// advertises the thing it acts on (proposal §5.7, Gitea #469).
//
// The four fixtures below are the four columns of the §5.3 capability table:
// a strip board, a HUB75 panel, a regular 2D matrix built from strips, and a
// 3D / irregular coordinate map. Driving them here rather than through a
// browser is the point — one board at a time in chromium would need four
// mirrors and would still only prove what is on screen, not what the rule is.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import {
  chainOrder,
  estimatedRefreshHz,
  FALLBACK_CAPS,
  offsetLabel,
  outputRanges,
  outputsSum,
  PANEL_DRIVER_DEFAULT,
  settingsVisibility,
  squarish,
  uiLayoutKind,
  zoneLabel,
  zonesByRegion,
  zoneOffsetMinutes,
} from "../src/lib/settingsCaps.ts";

/** A device's `caps` block, with the fields a fixture doesn't care about
 *  filled in from the conservative fallback. */
const caps = (over) => ({ ...FALLBACK_CAPS, ...over });

// ---- the four fixtures (proposal §5.3's capability table) ----

/** SK9822 strip board: everything strip-shaped, one output. */
const STRIP = {
  caps: caps({ strip_driver: true, panel: false, reboot: true, ota: true }),
  layout: { kind: "strip", dims: 1, regular: true, panels: 1 },
};

/** Seengreat HUB75 S3, one 64×64 panel: no strip driver, no power model, and
 *  no blur/glow (the two spatial stages overrun the compose window, D12). */
const PANEL = {
  caps: caps({
    strip_driver: false,
    panel: true,
    power_cap: false,
    blur_glow: false,
    psram: true,
    reboot: true,
    ota: true,
    layers: 2,
  }),
  layout: { kind: "matrix", dims: 2, regular: true, panels: 1 },
};

/** A WS2812 matrix built from strip: a strip driver AND a grid, two outputs
 *  (the Athom), so the Outputs table exists. */
const MATRIX_FROM_STRIPS = {
  caps: caps({ strip_driver: true, panel: false, outputs: 2, reboot: true, ota: true }),
  layout: { kind: "matrix", dims: 2, regular: true, panels: 4 },
};

/** A 3D / irregular coordinate map on a strip board: no neighbour table, so
 *  no blur/glow, and nothing panel-shaped. */
const MAP_3D = {
  caps: caps({ strip_driver: true, panel: false, blur_glow: false, reboot: true, ota: true }),
  layout: { kind: "map", dims: 3, regular: false, panels: 1 },
};

/** The 3D LATTICE kind (Gitea #538): a strip board driving a `w×h×d` cube of
 *  pixels. The wire calls it `map`; the picker calls it `3D`. */
const LATTICE = {
  caps: caps({ strip_driver: true, panel: false, blur_glow: false, reboot: true, ota: true }),
  layout: { kind: "lattice", dims: 3, regular: true, panels: 1 },
};

/** An irregular 2D map — a ring, a sculpture (mockup S3h). */
const MAP_2D = {
  caps: caps({ strip_driver: true, panel: false, blur_glow: false, reboot: true, ota: true }),
  layout: { kind: "map", dims: 2, regular: false, panels: 1 },
};

const vis = (f) => settingsVisibility(f.caps, f.layout);

// ---- LED layout ----

test("strip board: the kind picker offers all three, strip fields are real", () => {
  const v = vis(STRIP);
  assert.equal(v.kindPicker, true);
  assert.deepEqual(v.kindOptions, ["strip", "matrix", "lattice", "map"]);
  assert.equal(v.stripFields, true);
  // a strip has no panel anything
  assert.equal(v.panelSize, false);
  assert.equal(v.panelScan, false);
  assert.equal(v.arrangement, false);
  assert.equal(v.estimatedRefresh, false);
  assert.equal(v.panelDriver, false);
  assert.equal(v.outputsTable, false);
});

test("HUB75 panel: NO kind picker — the board offers no choice (§5.7)", () => {
  const v = vis(PANEL);
  assert.equal(v.kindPicker, false, "the disabled Strip option in mockup S3 was the bug");
  assert.deepEqual(v.kindOptions, ["matrix"]);
  assert.equal(v.stripFields, false, "no LED type / colour order / data pin on a panel");
  assert.equal(v.panelSize, true);
  assert.equal(v.panelScan, true);
  assert.equal(v.estimatedRefresh, true);
  assert.equal(v.panelDriver, true);
});

test("matrix built from strips: panel size, pixel wiring, no scan divisor", () => {
  const v = vis(MATRIX_FROM_STRIPS);
  assert.equal(v.stripFields, true);
  assert.equal(v.panelSize, true);
  assert.equal(v.panelScan, false, "scan is a HUB75 field");
  assert.equal(v.estimatedRefresh, false, "no panel driver to estimate");
  assert.equal(v.wiringRow, true);
  assert.equal(v.wiringIsPixels, false, "4 tiles — the row describes the CHAIN");
  assert.equal(v.rot180, true);
});

test("a single-tile matrix's wiring row is PIXEL wiring, and has no rot180", () => {
  const v = settingsVisibility(MATRIX_FROM_STRIPS.caps, {
    ...MATRIX_FROM_STRIPS.layout,
    panels: 1,
  });
  assert.equal(v.wiringIsPixels, true);
  assert.equal(v.rot180, false, "nothing to alternate");
});

test("the Outputs table follows caps.outputs, not the board or the kind", () => {
  assert.equal(vis(MATRIX_FROM_STRIPS).outputsTable, true);
  assert.equal(vis(STRIP).outputsTable, false);
  assert.equal(vis(PANEL).outputsTable, false);
  assert.equal(vis(MAP_3D).outputsTable, false);
});

// ---- Advanced ----

test("power cap is absent on a panel: a fixed load on a supply sized for it", () => {
  assert.equal(vis(STRIP).powerCap, true);
  assert.equal(vis(PANEL).powerCap, false);
  assert.equal(vis(MATRIX_FROM_STRIPS).powerCap, true);
  assert.equal(vis(MAP_3D).powerCap, true);
});

test("blur/glow need neighbours AND the frame budget", () => {
  assert.equal(vis(STRIP).blurGlow, true);
  assert.equal(vis(PANEL).blurGlow, false, "the compose window is one rescan (D12)");
  assert.equal(vis(MATRIX_FROM_STRIPS).blurGlow, true);
  assert.equal(vis(MAP_3D).blurGlow, false, "no neighbour table for a cloud");
});

test("blur/glow is worded for the fixture it acts on", () => {
  assert.equal(vis(STRIP).blurGlowScope, "strip");
  assert.equal(vis(PANEL).blurGlowScope, "grid");
  assert.equal(vis(MATRIX_FROM_STRIPS).blurGlowScope, "grid");
});

test("PSRAM / OTA / reboot rows are exactly what the device advertises", () => {
  assert.equal(vis(PANEL).psram, true);
  assert.equal(vis(STRIP).psram, false);
  // the mirror: it implements neither, and says so
  const mirror = settingsVisibility(caps({ reboot: false, ota: false }), STRIP.layout);
  assert.equal(mirror.ota, false);
  assert.equal(mirror.reboot, false);
});

test("firmware older than caps falls back conservatively, never to a board name", () => {
  const v = settingsVisibility(null, { kind: "strip", dims: 1, regular: true, panels: 1 });
  assert.equal(v.stripFields, true, "every build has always had a strip driver");
  assert.equal(v.powerCap, true);
  assert.equal(v.blurGlow, true);
  assert.equal(v.panel ?? false, false);
  assert.equal(v.panelDriver, false);
  assert.equal(v.psram, false);
  assert.equal(v.ota, false, "a field that needs newer firmware reads false");
  assert.equal(v.outputsTable, false);
});

// ---- estimated refresh (the model the bench measurements fit) ----

test("estimated refresh reproduces the measured bench numbers", () => {
  // firmware/src/hub75.rs: one 64×64 panel, 1/32 scan, 7 planes.
  const panel = { pw: 64, ph: 64, panels: 1, scan: 32 };
  const at = (mhz, planes = 7) =>
    Math.round(estimatedRefreshHz(panel, { clockHz: mhz * 1e6, planes }));
  assert.equal(at(20), 77, "the esp-hub75 example's clock");
  assert.equal(at(30), 115, "the value the firmware ships");
  assert.equal(at(40), 154, "out of spec, but linear");
  assert.equal(at(30, 8), 57, "the refresh halves per extra plane");
  assert.equal(at(30, 6), 233, "the firmware comment rounds this to ~231");
});

test("estimated refresh divides by the chain length, and scan defaults to ph/2", () => {
  const four = estimatedRefreshHz({ pw: 64, ph: 64, panels: 4, scan: 32 });
  const one = estimatedRefreshHz({ pw: 64, ph: 64, panels: 1, scan: 32 });
  assert.ok(Math.abs(four * 4 - one) < 1e-6, "four panels in one chain is a quarter the rate");
  assert.equal(
    estimatedRefreshHz({ pw: 64, ph: 64, panels: 1, scan: 0 }),
    estimatedRefreshHz({ pw: 64, ph: 64, panels: 1, scan: 32 }),
    "scan 0 = the board's own, which is ph/2",
  );
  assert.equal(estimatedRefreshHz({ pw: 0, ph: 0, panels: 0, scan: 0 }), 0);
  assert.equal(PANEL_DRIVER_DEFAULT.planes, 7);
});

test("PANEL_DRIVER_DEFAULT is the FALLBACK, not the model (Gitea #401/#525)", () => {
  // The clock and bit depth are device SETTINGS now: `/api/layout`'s `driver`
  // block carries what this board has stored, and the console computes the
  // estimate from THAT (`lib/panelDriver.ts`, `tests/panelDriver.test.mjs`).
  // This constant stays as the reading for firmware that reports no block,
  // which is also `estimatedRefreshHz`'s default parameter — so the two must
  // not drift apart.
  const panel = { pw: 64, ph: 64, panels: 1, scan: 32 };
  assert.deepEqual(PANEL_DRIVER_DEFAULT, { clockHz: 30_000_000, planes: 7 });
  assert.equal(estimatedRefreshHz(panel), estimatedRefreshHz(panel, PANEL_DRIVER_DEFAULT));
});

test("the Panel driver ROW is caps-gated, whatever the driver block says", () => {
  // The disclosure exists because the board has a panel (`caps.panel`); what
  // is inside it — an editable form or this build's constants — is the
  // `driver` block's business, not the visibility rule's (§5.7).
  assert.equal(vis(PANEL).panelDriver, true);
  assert.equal(vis(MATRIX_FROM_STRIPS).panelDriver, false, "a matrix of strips has no HUB75 driver");
});

// ---- the chain order the arrangement SVG draws ----

test("the chain threads a 2×2 top-left, row-major, snaked", () => {
  const t = chainOrder(2, 2, "tl", "row", true);
  assert.deepEqual(
    t.map((x) => [x.col, x.row]),
    [
      [0, 0],
      [1, 0],
      [1, 1],
      [0, 1],
    ],
  );
  // the return leg scans mirrored, which is what the ↻ marker is about
  assert.deepEqual(
    t.map((x) => x.flipX),
    [false, false, true, true],
  );
});

test("without a snake every line restarts on the same side", () => {
  const t = chainOrder(3, 2, "tl", "row", false);
  assert.deepEqual(
    t.map((x) => [x.col, x.row]),
    [
      [0, 0],
      [1, 0],
      [2, 0],
      [0, 1],
      [1, 1],
      [2, 1],
    ],
  );
  assert.ok(t.every((x) => x.flipX === false));
});

test("the start corner picks which sides the chain begins on", () => {
  const br = chainOrder(2, 2, "br", "row", false);
  assert.deepEqual(br[0] && [br[0].col, br[0].row], [1, 1], "bottom-right is tile 1");
  assert.equal(br[0]?.flipX, true, "and it scans right-to-left");
  const col = chainOrder(2, 2, "bl", "col", true);
  assert.deepEqual(
    col.map((x) => [x.col, x.row]),
    [
      [0, 1],
      [0, 0],
      [1, 0],
      [1, 1],
    ],
    "column-major from the bottom-left, snaking back down",
  );
});

test("a 1×1 chain is one tile, unflipped, whatever the fields say", () => {
  const t = chainOrder(1, 1, "tl", "row", true);
  assert.equal(t.length, 1);
  assert.deepEqual(t[0] && [t[0].col, t[0].row, t[0].flipX], [0, 0, false]);
});

// ---- output ranges (D11: a consecutive run of ONE index space) ----

test("outputs compute their own range so nobody adds up offsets", () => {
  assert.deepEqual(outputRanges([300, 300], "pixels"), [
    { from: 0, to: 299 },
    { from: 300, to: 599 },
  ]);
  assert.deepEqual(outputRanges([2, 2], "panels"), [
    { from: 1, to: 2 },
    { from: 3, to: 4 },
  ]);
  assert.equal(outputsSum([300, 300]), 600);
});

test("an empty run does not swallow the next one's first index", () => {
  assert.deepEqual(outputRanges([0, 60], "pixels"), [
    { from: 0, to: 0 },
    { from: 0, to: 59 },
  ]);
});

// ---- Strip → Matrix keeps the fixture the same size ----

test("picking Matrix factors the pixel count instead of rounding it up", () => {
  assert.deepEqual(squarish(120), { w: 12, h: 10 }, "120 px is 12×10, not 11×11");
  assert.deepEqual(squarish(4096), { w: 64, h: 64 });
  assert.deepEqual(squarish(256), { w: 16, h: 16 });
  assert.deepEqual(squarish(300), { w: 20, h: 15 });
});

test("a count with no usable factor pair falls back to a square", () => {
  // 61 is prime: every pair is 1×61, which is a line, not a matrix
  assert.deepEqual(squarish(61), { w: 8, h: 8 });
  assert.deepEqual(squarish(1), { w: 1, h: 1 });
  assert.deepEqual(squarish(0), { w: 1, h: 1 });
});

test("a tile knows which chain LINE it is on (rot180 is per line, not per row)", () => {
  // dir "col": a line is a COLUMN of tiles, so the 180° mark follows columns
  const col = chainOrder(3, 2, "tl", "col", true);
  assert.deepEqual(
    col.map((t) => t.line),
    [0, 0, 1, 1, 2, 2],
  );
  assert.deepEqual(
    col.map((t) => [t.col, t.row]),
    [
      [0, 0],
      [0, 1],
      [1, 1],
      [1, 0],
      [2, 0],
      [2, 1],
    ],
  );
  const row = chainOrder(2, 3, "tl", "row", true);
  assert.deepEqual(
    row.map((t) => t.line),
    [0, 0, 1, 1, 2, 2],
  );
});


// ---- the Projection section (Gitea #538) ----
//
// A Layout never shows a pattern BIGGER than itself, so the section exists
// only where something is left to choose: a strip offers nothing and the
// section is absent — with no copy explaining why.

test("projection: a 1D layout has no section at all", () => {
  assert.equal(vis(STRIP).projection, false, "a strip shows 1D patterns and nothing else");
});

test("projection: a 2D layout has a section (the 1D row)", () => {
  assert.equal(vis(PANEL).projection, true);
  assert.equal(vis(MATRIX_FROM_STRIPS).projection, true);
  assert.equal(vis(MAP_2D).projection, true, "an irregular 2D map still projects 1D patterns");
});

test("projection: a 3D layout has a section (the 1D and 2D rows)", () => {
  assert.equal(vis(LATTICE).projection, true);
  assert.equal(vis(MAP_3D).projection, true);
});

// ---- the 3D lattice kind ----

test("the 3D kind is offered wherever a strip driver is", () => {
  assert.deepEqual(vis(STRIP).kindOptions, ["strip", "matrix", "lattice", "map"]);
  assert.deepEqual(vis(PANEL).kindOptions, ["matrix"], "a HUB75 board drives no cube");
});

test("the lattice fields appear only when 3D is the picked kind", () => {
  assert.equal(vis(LATTICE).latticeFields, true);
  assert.equal(vis(STRIP).latticeFields, false);
  assert.equal(vis(MAP_3D).latticeFields, false, "a 3D map that is not a lattice");
});

test("uiLayoutKind: only a 3D map is the picker's `3D`", () => {
  assert.equal(uiLayoutKind("map", 3), "lattice");
  assert.equal(uiLayoutKind("map", 2), "map");
  assert.equal(uiLayoutKind("strip", 1), "strip");
  assert.equal(uiLayoutKind("matrix", 2), "matrix");
});

// ---- time zones (mockup S3 `Clock & time zone`; Gitea #538) ----

test("zoneOffsetMinutes reads a zone's CURRENT offset, DST included", () => {
  assert.equal(zoneOffsetMinutes("UTC"), 0);
  // Denver is -7h in January and -6h in July; both must come back exactly
  assert.equal(zoneOffsetMinutes("America/Denver", new Date("2026-01-15T12:00:00Z")), -420);
  assert.equal(zoneOffsetMinutes("America/Denver", new Date("2026-07-15T12:00:00Z")), -360);
  // a half-hour zone, and one that is not a whole hour either
  assert.equal(zoneOffsetMinutes("Asia/Kolkata", new Date("2026-01-15T12:00:00Z")), 330);
  assert.equal(zoneOffsetMinutes("Asia/Kathmandu", new Date("2026-01-15T12:00:00Z")), 345);
  assert.equal(zoneOffsetMinutes("Europe/Berlin", new Date("2026-01-15T12:00:00Z")), 60);
  // an unknown zone is 0, never a throw — the select is built from whatever
  // this browser knows, but a persisted name can outlive an engine update
  assert.equal(zoneOffsetMinutes("Middle/Earth"), 0);
});

test("zoneLabel drops the region and the underscores", () => {
  assert.equal(zoneLabel("America/Denver"), "Denver");
  assert.equal(zoneLabel("America/Indiana/Knox"), "Indiana · Knox");
  assert.equal(zoneLabel("Europe/Isle_of_Man"), "Isle of Man");
  assert.equal(zoneLabel("UTC"), "UTC");
});

test("zonesByRegion groups and sorts for the optgroups", () => {
  const g = zonesByRegion(["Europe/Paris", "America/Denver", "UTC", "Europe/Berlin"]);
  assert.deepEqual(
    g.map((x) => x.region),
    ["America", "Europe", "Other"],
  );
  assert.deepEqual(g[1].zones, ["Europe/Berlin", "Europe/Paris"]);
  assert.deepEqual(g[2].zones, ["UTC"], "a zone with no region lands in Other");
});

test("offsetLabel is the status line's words", () => {
  assert.equal(offsetLabel(-360), "UTC-6");
  assert.equal(offsetLabel(0), "UTC+0");
  assert.equal(offsetLabel(345), "UTC+5:45");
  assert.equal(offsetLabel(-270), "UTC-4:30");
});
