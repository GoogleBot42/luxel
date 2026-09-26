// Unit tests for Advanced › Panel driver's pure half (src/lib/panelDriver.ts)
// — the HUB75 driver as a SETTING (Gitea #401/#525).
//
// Two readings of the same four values cross the wire: the CONFIGURED one the
// form edits and the LIVE one the DMA is running. Which of them is on the
// panel decides what the card says, and the interesting answers are exactly
// the ones a healthy bench panel never produces — a configured driver that
// did not fit in internal RAM, panel output that never came up at all. So the
// decision is a function over fixtures here rather than a state anybody waits
// for in chromium.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { estimatedRefreshHz, PANEL_DRIVER_DEFAULT } from "../src/lib/settingsCaps.ts";
import {
  BLANK_MAX,
  chipLabel,
  chipShort,
  CLOCK_CEILING_MHZ,
  CLOCK_CHOICES_DEFAULT,
  clampBlank,
  clockChoices,
  clockSupported,
  configuredDriver,
  driverWire,
  effectiveScan,
  liveSummary,
  panelDriverState,
  panelGeometryOf,
  panelLine,
  panelRefreshHz,
  PANEL_DRIVER_LINE_DEFAULT,
  panelModuleLine,
  phrase,
  PLANE_CHOICES,
  refreshDriver,
  scanOptions,
  scanShown,
  scanWire,
  snapClock,
} from "../src/lib/panelDriver.ts";

/** One 64×64 panel at 1/32 scan — the Seengreat bench panel. */
const MATRIX = { pw: 64, ph: 64, cols: 1, rows: 1, start: "tl", dir: "row", snake: 0, rot180: 0, scan: 32 };

/** What the firmware reports as running for that panel at the default driver. */
const LIVE = {
  planes: 7,
  clock_mhz: 30,
  chip: "shiftreg",
  blank: 1,
  w: 64,
  h: 64,
  scan: 32,
  fb_bytes: 28672,
  fallback: false,
};

const CHIPS = ["shiftreg", "fm6126a", "icn2038s", "dp3246"];

/** A panel-board Layout, with the `driver` block overridden per case. */
const wire = (driver, matrix = MATRIX) => ({
  kind: "matrix",
  source: "regular",
  dims: 2,
  regular: true,
  pixels: matrix.pw * matrix.ph * matrix.cols * matrix.rows,
  max: 4096,
  w: matrix.pw * matrix.cols,
  h: matrix.ph * matrix.rows,
  matrix,
  driver,
  outputs: [],
  proj: { proj1d: "repeat", proj2d: "fit", proj3d: "slice" },
  map: { installed: false, dims: 2, count: 0 },
});

/** The block as the firmware reports it, configured values overridden. */
const CLOCKS = [8, 10, 12, 15, 20, 24, 30];

const block = (over = {}, live = LIVE) => ({
  planes: 7,
  clock_mhz: 30,
  chip: "shiftreg",
  blank: 1,
  chips: CHIPS,
  clocks: CLOCKS,
  live,
  ...over,
});

// ---- the mismatch path: firmware with no `panel` line at all --------------
//
// The card no longer draws a read-only plaque here (Gitea #771) — the row is
// caps-gated to panel boards, so no `driver` block means the console is newer
// than the firmware and the card says exactly that. What still needs a default
// is the refresh ESTIMATE, which is all `PANEL_DRIVER_DEFAULT` is for now.

test("no driver block: the ESTIMATE falls back to this build's constants", () => {
  const w = wire(undefined);
  assert.equal(driverWire(w), null, "the card states a firmware mismatch on this reading");
  assert.deepEqual(configuredDriver(w), PANEL_DRIVER_LINE_DEFAULT);
  assert.deepEqual(configuredDriver(null), PANEL_DRIVER_LINE_DEFAULT);
  // and the fallback IS the firmware's constants, not a second copy of them
  assert.equal(PANEL_DRIVER_LINE_DEFAULT.planes, PANEL_DRIVER_DEFAULT.planes);
  assert.equal(PANEL_DRIVER_LINE_DEFAULT.clock_mhz, PANEL_DRIVER_DEFAULT.clockHz / 1e6);
  assert.equal(PANEL_DRIVER_LINE_DEFAULT.chip, "shiftreg");
  assert.equal(PANEL_DRIVER_LINE_DEFAULT.blank, 1);
});

test("no driver block: there is no live reading to disagree with", () => {
  const s = panelDriverState(driverWire(wire(undefined)), panelGeometryOf(wire(undefined)));
  assert.equal(s.status, "unknown");
  assert.deepEqual(s.changed, []);
  assert.equal(s.live, "");
});

// ---- the wire line the form writes ---------------------------------------

test("every edit is one `panel` line, in wire order", () => {
  assert.equal(panelLine(PANEL_DRIVER_LINE_DEFAULT), "panel 7 30 shiftreg 1");
  assert.equal(
    panelLine({ planes: 6, clock_mhz: 20, chip: "fm6126a", blank: 2 }),
    "panel 6 20 fm6126a 2",
  );
  // the card patches the CONFIGURED record, so an untouched field is resent
  // as-is rather than as a default
  const cfg = configuredDriver(wire(block({ planes: 8, clock_mhz: 24, chip: "dp3246", blank: 3 })));
  assert.equal(panelLine({ ...cfg, planes: 6 }), "panel 6 24 dp3246 3");
});

test("the form cannot post a clock or a blanking the firmware would refuse", () => {
  assert.equal(clampBlank(-3), 0);
  assert.equal(clampBlank(12), BLANK_MAX);
  assert.equal(clampBlank(2), 2);
  assert.deepEqual([...PLANE_CHOICES], [4, 5, 6, 7, 8]);
});

// ---- the pixel clock is a LIST, not a number field (Gitea #771) -----------
//
// Jeremy set 40 MHz "to see what happens" and got a mis-sampling panel; the
// values in between are not all reachable with an even LCD_CAM divide anyway.
// So the control is a `<select>` over the device's own `driver.clocks`, and
// nothing in the browser decides the list.

test("the offered clocks are the DEVICE's, and this build's only as a fallback", () => {
  assert.deepEqual([...CLOCK_CHOICES_DEFAULT], [8, 10, 12, 15, 20, 24, 30]);
  assert.equal(CLOCK_CEILING_MHZ, 30, "the FM6124 datasheet ceiling tops the list");
  assert.equal(CLOCK_CHOICES_DEFAULT[CLOCK_CHOICES_DEFAULT.length - 1], CLOCK_CEILING_MHZ);
  // a firmware that names its own list — even a different one — wins outright
  const newer = block({ clocks: [10, 20, 40], clock_mhz: 20 });
  assert.deepEqual(clockChoices(newer), [10, 20, 40]);
  assert.ok(clockSupported(newer, 40), "the device says it takes it");
  // #525-era firmware reports a driver block with no `clocks` at all
  const older = { ...block(), clocks: undefined };
  assert.deepEqual(clockChoices(older), [8, 10, 12, 15, 20, 24, 30]);
  // no device at all: the built-in list, so the select is never empty
  assert.deepEqual(clockChoices(null), [8, 10, 12, 15, 20, 24, 30]);
});

test("a stored clock the device no longer offers is still SHOWN, ascending", () => {
  // exactly Jeremy's device after the rollback: 40 MHz stored, 40 not offered
  const stuck = block({ clock_mhz: 40 });
  assert.deepEqual(clockChoices(stuck), [8, 10, 12, 15, 20, 24, 30, 40]);
  assert.equal(clockSupported(stuck, 40), false, "the card marks it unsupported");
  assert.equal(clockSupported(stuck, 30), true);
  // and a value that IS on the list is not duplicated
  assert.deepEqual(clockChoices(block({ clock_mhz: 20 })), [8, 10, 12, 15, 20, 24, 30]);
});

test("a typed or stale clock snaps to the nearest OFFERED value", () => {
  const d = block();
  assert.equal(snapClock(40, d), 30, "over the ceiling comes back to it");
  assert.equal(snapClock(2, d), 8, "under the floor comes up to it");
  assert.equal(snapClock(16, d), 15, "ties go to the lower, safer clock");
  assert.equal(snapClock(25, d), 24);
  assert.equal(snapClock(20, d), 20);
  assert.equal(snapClock(Number.NaN, d), 30, "nothing typed leaves the default");
  // it snaps into the DEVICE's list, not the browser's — 12 and 15 are on
  // this build's list and not on that device's
  const newer = block({ clocks: [10, 20, 40] });
  assert.equal(snapClock(12, newer), 10);
  assert.equal(snapClock(15, newer), 10, "a tie takes the lower, safer clock");
  assert.equal(snapClock(35, newer), 40);
  assert.equal(snapClock(9, null), 8);
  // whatever it returns is a value the device accepts
  for (const want of [0, 1, 7, 9, 11, 13, 17, 26, 31, 99])
    assert.ok(clockSupported(d, snapClock(want, d)), `${want} snapped off the list`);
});

// ---- live vs configured --------------------------------------------------

test("configured == live: the panel is running what the form shows", () => {
  const w = wire(block());
  const s = panelDriverState(driverWire(w), panelGeometryOf(w));
  assert.equal(s.status, "live");
  assert.deepEqual(s.changed, []);
  assert.equal(s.live, "7 planes · 30 MHz · plain shift register · blanking 1 · 64×64 1/32");
});

test("each BOOT-BUILT value on its own raises reboot-to-apply", () => {
  const cases = [
    [{ planes: 6 }, "bit planes"],
    [{ clock_mhz: 20 }, "the pixel clock"],
    [{ chip: "fm6126a" }, "the driver chip"],
  ];
  for (const [over, want] of cases) {
    const w = wire(block(over));
    const s = panelDriverState(driverWire(w), panelGeometryOf(w));
    assert.equal(s.status, "pending", `${want} is stored, not running`);
    assert.deepEqual(s.changed, [want]);
  }
});

// Latch blanking is the exception (Gitea #778): the firmware re-formats its
// framebuffers between frames, so `live.blank` legitimately lags the reply to
// the POST that changed it by ONE FRAME. Comparing it would put "reboot to
// apply" under the one control that does not need one — and the whole point of
// making it live is that it is tuned by watching the panel for ghosting.
test("latch blanking is never pending: it applies on the next frame", () => {
  for (const blank of [0, 2, 4, 8]) {
    const w = wire(block({ blank }));
    const s = panelDriverState(driverWire(w), panelGeometryOf(w));
    assert.equal(s.status, "live", `blank ${blank} does not wait for a boot`);
    assert.deepEqual(s.changed, []);
  }
  // …and it does not MASK one of the three that do
  const both = wire(block({ blank: 4, planes: 6 }));
  const s = panelDriverState(driverWire(both), panelGeometryOf(both));
  assert.equal(s.status, "pending");
  assert.deepEqual(s.changed, ["bit planes"], "only the boot-built field waits");
});

test("the GEOMETRY is part of it: a panel board sizes its framebuffer at boot", () => {
  // a 2×2 chain stored against a live 1×1 framebuffer
  const chained = { ...MATRIX, cols: 2, rows: 2 };
  const w = wire(block(), chained);
  const s = panelDriverState(driverWire(w), panelGeometryOf(w));
  assert.equal(s.status, "pending");
  assert.deepEqual(s.changed, ["the panel size"]);
  // a scan divisor change is its own row
  const rescanned = wire(block(), { ...MATRIX, scan: 16 });
  const s2 = panelDriverState(driverWire(rescanned), panelGeometryOf(rescanned));
  assert.deepEqual(s2.changed, ["the panel scan"]);
  // `scan: 0` is "the board's own", i.e. ph/2 — the same 32, so nothing waits
  const stock = wire(block(), { ...MATRIX, scan: 0 });
  assert.equal(effectiveScan(panelGeometryOf(stock)), 32);
  assert.equal(panelDriverState(driverWire(stock), panelGeometryOf(stock)).status, "live");
});

test("with no matrix line the driver values are still compared", () => {
  const w = wire(block({ planes: 5 }), MATRIX);
  const s = panelDriverState(driverWire(w), null);
  assert.equal(s.status, "pending");
  assert.deepEqual(s.changed, ["bit planes"]);
});

test("several changes read as a sentence, in the card's own order", () => {
  const w = wire(block({ planes: 8, clock_mhz: 20, chip: "dp3246" }), { ...MATRIX, scan: 16 });
  const s = panelDriverState(driverWire(w), panelGeometryOf(w));
  assert.deepEqual(s.changed, [
    "bit planes",
    "the pixel clock",
    "the driver chip",
    "the panel scan",
  ]);
  assert.equal(
    phrase(s.changed),
    "bit planes, the pixel clock, the driver chip and the panel scan",
  );
  assert.equal(phrase([]), "");
  assert.equal(phrase(["bit planes"]), "bit planes");
});

// ---- the two states a healthy panel never shows --------------------------

test("fallback: the configured driver did not fit, the board default is running", () => {
  // 8 planes on a 2×2 chain, and the firmware booted its own 64×64 at 7
  const w = wire(block({ planes: 8 }, { ...LIVE, fallback: true }), { ...MATRIX, cols: 2, rows: 2 });
  const s = panelDriverState(driverWire(w), panelGeometryOf(w));
  assert.equal(s.status, "fallback", "a reboot will not fix this — it has to get smaller");
  assert.deepEqual(s.changed, ["bit planes", "the panel size"], "what to point the user at");
  assert.ok(s.live.includes("7 planes"), "the card names what IS running");
});

test("live null: panel output is off, whatever is stored", () => {
  const w = wire(block({ planes: 6 }, null));
  const s = panelDriverState(driverWire(w), panelGeometryOf(w));
  assert.equal(s.status, "disabled");
  assert.deepEqual(s.changed, [], "nothing is waiting on a reboot — nothing is running");
  assert.equal(s.live, "");
  assert.equal(liveSummary(null), "");
});

// ---- the collapsed Panel module row (Gitea #778) --------------------------
//
// The row is inside the LED layout card now, and its one line is the whole
// module: the scan ratio, the chip, the clock, the bit depth and the blanking,
// in that order. No refresh rate — the estimated/measured readout sits
// immediately beneath the row, so repeating it would print the same number
// twice on one screen.

test("the collapsed row states the whole module, in the order it is read", () => {
  const w = wire(block({ clock_mhz: 20, planes: 6 }, { ...LIVE, clock_mhz: 20, planes: 6 }));
  const s = panelDriverState(driverWire(w), panelGeometryOf(w));
  assert.equal(panelModuleLine(w, s), "1/32 scan · plain shift register · 20 MHz · 6 planes · blanking 1");
  // the scan is the EFFECTIVE one, so `scan 0` reads as the ratio it means
  const stock = wire(block(), { ...MATRIX, scan: 0 });
  assert.match(panelModuleLine(stock), /^1\/32 scan · /);
  // a 1/16-scan 64-row module, and a different chip and blanking
  const other = wire(
    block({ chip: "dp3246", blank: 3 }, { ...LIVE, chip: "dp3246", blank: 3, scan: 16 }),
    { ...MATRIX, scan: 16 },
  );
  assert.equal(panelModuleLine(other), "1/16 scan · DP3246 · 30 MHz · 7 planes · blanking 3");
});

test("the collapsed row says when the two readings cannot agree", () => {
  const pending = wire(block({ planes: 6 }));
  assert.match(
    panelModuleLine(pending, panelDriverState(driverWire(pending), panelGeometryOf(pending))),
    /reboot to apply$/,
  );
  const fell = wire(block({ planes: 8 }, { ...LIVE, fallback: true }));
  assert.match(
    panelModuleLine(fell, panelDriverState(driverWire(fell), panelGeometryOf(fell))),
    /not applied$/,
  );
  const off = wire(block({}, null));
  assert.match(
    panelModuleLine(off, panelDriverState(driverWire(off), panelGeometryOf(off))),
    /output off$/,
  );
  // a blanking that is stored but not yet on the panel is NOT a disagreement:
  // the firmware applies it on its next frame (#778)
  const blanked = wire(block({ blank: 4 }));
  assert.equal(
    panelModuleLine(blanked, panelDriverState(driverWire(blanked), panelGeometryOf(blanked))),
    "1/32 scan · plain shift register · 30 MHz · 7 planes · blanking 4",
  );
  // no driver block: the row states the MISMATCH, never this build's constants
  // dressed up as the device's (#771)
  assert.equal(panelModuleLine(wire(undefined)), "1/32 scan · firmware too old");
  assert.equal(panelModuleLine(null), "firmware too old");
});

// ---- the Scan rate field (Gitea #778) ------------------------------------
//
// It used to say `board default` for `scan 0`. Jeremy: "Is that even possible
// for luxel to know?" — it is not. HUB75 is write-only, and `0` means the
// usual ratio for the height, `ph / 2`. So the options are the ratios as they
// are printed on the module (`1/32S`), with the usual one named as such.

test("the scan options are the ratios that divide ph/2, largest first", () => {
  assert.deepEqual(
    scanOptions(64).map((o) => o.label),
    ["1/32 (usual for 64 rows)", "1/16", "1/8", "1/4"],
  );
  assert.deepEqual(
    scanOptions(64).map((o) => o.scan),
    [32, 16, 8, 4],
  );
  assert.deepEqual(scanOptions(64).map((o) => o.usual), [true, false, false, false]);
  // a 32-row module is 16 address rows, and 32 does not divide 16
  assert.deepEqual(
    scanOptions(32).map((o) => o.label),
    ["1/16 (usual for 32 rows)", "1/8", "1/4"],
  );
  // …nor does anything the height cannot tile: a 1/8 module offers 1/8 and 1/4
  assert.deepEqual(
    scanOptions(16).map((o) => o.label),
    ["1/8 (usual for 16 rows)", "1/4"],
  );
  // the usual ratio is always present even when the candidate set has no such
  // entry — a 128-row wall is 1/64
  assert.deepEqual(
    scanOptions(128).map((o) => o.scan),
    [64, 32, 16, 8, 4],
  );
  assert.equal(scanOptions(128)[0].label, "1/64 (usual for 128 rows)");
  // a stored ratio this list does not carry is still shown rather than being
  // silently re-read as something else
  assert.deepEqual(
    scanOptions(64, 2).map((o) => o.scan),
    [32, 16, 8, 4, 2],
  );
  // never the words "board default", on any height
  for (const ph of [8, 16, 32, 64, 128])
    for (const o of scanOptions(ph)) assert.doesNotMatch(o.label, /default/i);
});

test("picking the usual ratio sends 0, so it follows a height change", () => {
  assert.equal(scanWire(64, 32), 0, "the usual one is `0` on the wire");
  assert.equal(scanWire(64, 16), 16);
  assert.equal(scanWire(64, 4), 4);
  assert.equal(scanWire(32, 16), 0, "…and `0` is per height, not per number");
  assert.equal(scanWire(32, 8), 8);
  // and what the select SHOWS is the effective ratio either way
  assert.equal(scanShown(64, 0), 32);
  assert.equal(scanShown(64, 16), 16);
  assert.equal(scanShown(32, 0), 16);
  // every option round-trips: shown → wire → shown
  for (const ph of [16, 32, 64, 128])
    for (const o of scanOptions(ph)) assert.equal(scanShown(ph, scanWire(ph, o.scan)), o.scan);
});

// ---- the estimate the card shows beside the measurement ------------------

test("the estimate follows the CONFIGURED driver, not the build's constants", () => {
  const w = wire(block({ clock_mhz: 20, planes: 7 }));
  const d = refreshDriver(w);
  assert.deepEqual(d, { clockHz: 20e6, planes: 7 });
  const at = (layout) =>
    Math.round(estimatedRefreshHz({ pw: 64, ph: 64, panels: 1, scan: 32 }, refreshDriver(layout)));
  // the bench numbers from firmware/src/hub75.rs, now reachable as settings
  assert.equal(at(w), 77, "20 MHz on the bench panel");
  assert.equal(at(wire(block())), 115, "the default");
  assert.equal(at(wire(block({ planes: 6 }))), 233);
  assert.equal(at(wire(block({ planes: 8 }))), 57);
  assert.equal(at(wire(undefined)), 115, "legacy firmware estimates at its constants");
});

test("a device that reports its driver is estimated from THAT, not from est_hz", () => {
  const input = { pw: 64, ph: 64, panels: 1, scan: 32 };
  // the host's own number, which was computed at ITS constants, is stale the
  // moment the form changes the bit depth — so a reported driver wins
  const w = wire(block({ planes: 6 }));
  w.matrix.est_hz = 115;
  assert.equal(Math.round(panelRefreshHz(w, input)), 233);
  // …and without a block the device's number is the better one (#475)
  const legacy = wire(undefined);
  legacy.matrix.est_hz = 115;
  assert.equal(panelRefreshHz(legacy, input), 115);
  // nothing reported at all: the browser model at the fallback driver
  const bare = wire(undefined);
  assert.equal(Math.round(panelRefreshHz(bare, input)), 115);
});

// ---- the chip vocabulary -------------------------------------------------

test("every chip the contract names has a human label and a short form", () => {
  for (const c of CHIPS) {
    assert.notEqual(chipLabel(c), c, `${c} needs a label a human can pick`);
    assert.ok(chipShort(c).length > 0);
  }
  assert.match(chipLabel("shiftreg"), /FM6124/, "the plain case names the panels it covers");
  assert.match(chipLabel("dp3246"), /3-clock latch/);
  // a chip a newer firmware invents is shown verbatim, never hidden: the
  // select is built from `driver.chips`, which is the device's own list
  assert.equal(chipLabel("fm6353"), "fm6353");
  assert.equal(chipShort("fm6353"), "fm6353");
});

test("panelGeometryOf reads the chain, and only for a matrix", () => {
  assert.deepEqual(panelGeometryOf(wire(block(), { ...MATRIX, cols: 3, rows: 2 })), {
    pw: 64,
    ph: 64,
    chain: 6,
    scan: 32,
  });
  assert.equal(panelGeometryOf(null), null);
  assert.equal(panelGeometryOf({ kind: "strip", matrix: undefined }), null);
});
