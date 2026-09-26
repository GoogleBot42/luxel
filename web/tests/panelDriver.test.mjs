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
  emissions,
  latchClocks,
  litWidth,
  lsbEffective,
  lsbTrade,
  LSB_FULL,
  lsbWire,
  peakBrightnessFraction,
  truncatedPlanes,
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
  // the EFFECTIVE on-time the firmware clamped to: 64 words of row block less
  // one latch clock and two blanking clocks is W = 61, and a configured 0 means
  // full, so a healthy default panel reports 61 (Gitea #789)
  lsb: 61,
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
  lsb: LSB_FULL,
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
  assert.equal(PANEL_DRIVER_LINE_DEFAULT.lsb, LSB_FULL, "full on-time = the stock schedule");
});

test("no driver block: there is no live reading to disagree with", () => {
  const s = panelDriverState(driverWire(wire(undefined)), panelGeometryOf(wire(undefined)));
  assert.equal(s.status, "unknown");
  assert.deepEqual(s.changed, []);
  assert.equal(s.live, "");
});

// ---- the wire line the form writes ---------------------------------------

test("every edit is one `panel` line, in wire order", () => {
  assert.equal(panelLine(PANEL_DRIVER_LINE_DEFAULT), "panel 7 30 shiftreg 1 0");
  assert.equal(
    panelLine({ planes: 6, clock_mhz: 20, chip: "fm6126a", blank: 2, lsb: LSB_FULL }),
    "panel 6 20 fm6126a 2 0",
  );
  // the fifth field is OPTIONAL on the wire but always written (#789): a
  // four-field line reads as `lsb 0`, which would silently reset the trade
  // whenever any other field on the card is edited
  assert.equal(
    panelLine({ planes: 7, clock_mhz: 30, chip: "shiftreg", blank: 1, lsb: 30 }),
    "panel 7 30 shiftreg 1 30",
  );
  // the card patches the CONFIGURED record, so an untouched field is resent
  // as-is rather than as a default
  const cfg = configuredDriver(wire(block({ planes: 8, clock_mhz: 24, chip: "dp3246", blank: 3 })));
  assert.equal(panelLine({ ...cfg, planes: 6 }), "panel 6 24 dp3246 3 0");
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
  // a 1/16 scan stripes the row block two ways (128 words) and a DP3246 holds
  // the latch three clocks, so that template's W — and its effective on-time at
  // full — is 128 − 3 − 6 = 119
  const other = wire(
    block({ chip: "dp3246", blank: 3 }, { ...LIVE, chip: "dp3246", blank: 3, scan: 16, lsb: 119 }),
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
  assert.deepEqual(d, { clockHz: 20e6, planes: 7, lsb: LSB_FULL, blank: 1, latch: 1 });
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

// ---- the brighter ↔ faster trade (Gitea #460 / #789) ---------------------
//
// One slider, `lsb` — the LSB's on-time in pixel clocks, 0 = full. The bench
// case throughout is the Seengreat panel: one 64×64 module at 1/32 scan, 7
// planes, 30 MHz, one blanking clock on a plain shift register. That is 64
// words of row block less one latch clock and two blanking clocks, so W = 61.

const BENCH = { pw: 64, ph: 64, chain: 1, scan: 32 };
const benchCfg = (over = {}) => ({
  planes: 7,
  clock_mhz: 30,
  chip: "shiftreg",
  blank: 1,
  lsb: LSB_FULL,
  ...over,
});

test("the lit width of a row block is the words less the latch and the blanking", () => {
  assert.equal(litWidth(64, 1, 1), 61, "the bench panel");
  assert.equal(litWidth(64, 0, 1), 63);
  assert.equal(litWidth(64, 8, 1), 47);
  // a DP3246 holds the latch three clocks
  assert.equal(litWidth(64, 1, 3), 59);
  assert.equal(latchClocks("dp3246"), 3);
  for (const c of ["shiftreg", "fm6126a", "icn2038s", "fm6353"]) assert.equal(latchClocks(c), 1);
  // a template with no lit clock at all cannot light the panel; the floor of 1
  // is what keeps every readout finite rather than dividing by zero
  assert.equal(litWidth(16, 8, 3), 1);
  assert.equal(litWidth(0, 0, 1), 1);
});

test("a configured 0 means FULL, and anything over the width clamps to it", () => {
  assert.equal(lsbEffective(0, 61), 61, "0 = full, never zero on-time");
  assert.equal(lsbEffective(30, 61), 30);
  assert.equal(lsbEffective(61, 61), 61);
  assert.equal(lsbEffective(4000, 61), 61, "a stored value a smaller panel cannot honour");
  assert.equal(lsbEffective(1, 61), 1);
});

test("the truncated planes step as the on-time crosses W / 2^t", () => {
  // t is the largest t <= planes-1 with lsb << t <= W
  assert.equal(truncatedPlanes(61, 61, 7), 0, "full on-time IS the stock schedule");
  assert.equal(truncatedPlanes(31, 61, 7), 0, "just over half: nothing fits twice");
  assert.equal(truncatedPlanes(30, 61, 7), 1);
  assert.equal(truncatedPlanes(16, 61, 7), 1);
  assert.equal(truncatedPlanes(15, 61, 7), 2);
  assert.equal(truncatedPlanes(8, 61, 7), 2);
  assert.equal(truncatedPlanes(1, 61, 7), 5, "61 < 64, so the LSB cannot shift six times");
  // the MSB is always a repeated plane, so t never reaches `planes`
  for (const planes of [4, 5, 6, 7, 8])
    assert.ok(truncatedPlanes(1, 61, planes) <= planes - 1, `${planes} planes`);
});

test("the emissions per rescan are t + 2^(planes − t) − 1, stock at t = 0", () => {
  assert.equal(emissions(7, 0), 127, "2^7 − 1, the stock BCM schedule");
  assert.equal(emissions(7, 1), 64);
  assert.equal(emissions(7, 2), 33);
  assert.equal(emissions(8, 0), 255);
  assert.equal(emissions(4, 0), 15);
  // truncating never costs MORE shifts than the stock schedule
  for (const planes of [4, 5, 6, 7, 8])
    for (let t = 0; t < planes; t++)
      assert.ok(emissions(planes, t) <= emissions(planes, 0), `${planes}/${t}`);
});

test("peak brightness is lsb / W, continuous and independent of the bit depth", () => {
  assert.equal(peakBrightnessFraction(61, 61), 1);
  assert.equal(Math.round(peakBrightnessFraction(30, 61) * 100), 49);
  assert.equal(Math.round(peakBrightnessFraction(8, 61) * 100), 13);
  assert.ok(peakBrightnessFraction(4000, 61) <= 1, "never over full");
});

test("the trade at the bench numbers: 115 Hz full, 229 at lsb 30, 444 at lsb 8", () => {
  const at = (lsb) => lsbTrade(benchCfg(), BENCH, lsb);
  const full = at(LSB_FULL);
  assert.equal(full.width, 61);
  assert.equal(full.lsb, 61);
  assert.equal(full.trunc, 0);
  assert.equal(full.emissions, 127);
  assert.equal(full.fullEmissions, 127);
  assert.equal(Math.round(full.hz), 115, "the measured bench number (Gitea #255)");
  assert.equal(Math.round(full.brightness * 100), 100);
  assert.ok(full.full, "the slider is at its brighter end");

  const half = at(30);
  assert.equal(half.trunc, 1);
  assert.equal(half.emissions, 64);
  assert.equal(Math.round(half.hz), 229);
  assert.equal(Math.round(half.brightness * 100), 49);
  assert.equal(half.full, false);

  const fast = at(8);
  assert.equal(fast.trunc, 2);
  assert.equal(fast.emissions, 33);
  assert.equal(Math.round(fast.hz), 444);
  assert.equal(Math.round(fast.brightness * 100), 13);

  // the stored value is the default, so the card's readouts need no argument
  assert.deepEqual(lsbTrade(benchCfg({ lsb: 30 }), BENCH), half);
  // brightness is CONTINUOUS in the thumb and the Hz STEP: 31 is still the
  // stock schedule at half the brightness
  const edge = at(31);
  assert.equal(edge.trunc, 0);
  assert.equal(Math.round(edge.hz), 115);
  assert.equal(Math.round(edge.brightness * 100), 51);
  // and the trade never claims a rescan the stock schedule beats
  for (let lsb = 1; lsb <= 61; lsb++) assert.ok(at(lsb).hz >= full.hz - 1e-9, `lsb ${lsb}`);
});

test("the estimate at lsb 0 is byte-for-byte the one every caller had", () => {
  const input = { pw: 64, ph: 64, panels: 1, scan: 32 };
  // the whole point of `0 = full`: the emissions formula collapses to
  // `2^planes − 1`, so no existing reading moves (Gitea #789)
  for (const planes of [4, 5, 6, 7, 8]) {
    const was = 30e6 / (64 * 1 * 1 * 32 * (2 ** planes - 1));
    assert.equal(estimatedRefreshHz(input, { clockHz: 30e6, planes }), was, `${planes} planes`);
    assert.equal(
      estimatedRefreshHz(input, { clockHz: 30e6, planes, lsb: 0, blank: 1, latch: 1 }),
      was,
      `${planes} planes, lsb 0`,
    );
  }
  // …and the blanking and the latch only matter once the on-time truncates
  assert.equal(
    estimatedRefreshHz(input, { clockHz: 30e6, planes: 7, blank: 8, latch: 3 }),
    estimatedRefreshHz(input, { clockHz: 30e6, planes: 7 }),
  );
});

test("blanking and the chip's latch move the trade, because they set W", () => {
  // 8 blanking clocks on a DP3246 leave 64 − 3 − 16 = 45 lit clocks, so `full`
  // is 45 and an lsb of 30 no longer fits twice
  const t = lsbTrade(benchCfg({ chip: "dp3246", blank: 8 }), BENCH);
  assert.equal(t.width, 45);
  assert.equal(t.lsb, 45, "full follows the lit width rather than pinning to 61");
  assert.equal(t.trunc, 0);
  assert.equal(lsbTrade(benchCfg({ chip: "dp3246", blank: 8 }), BENCH, 30).trunc, 0);
  assert.equal(lsbTrade(benchCfg({ chip: "dp3246", blank: 8 }), BENCH, 22).trunc, 1);
  // a 1/16 scan stripes the row block two ways, so it shifts 128 words and the
  // latch and blanking are still paid once: 128 − 1 − 2
  assert.equal(lsbTrade(benchCfg(), { ...BENCH, scan: 16 }).width, 125);
  // a longer chain does the same — a 3-tile chain is 192 words
  assert.equal(lsbTrade(benchCfg(), { ...BENCH, chain: 3 }).width, 189);
});

test("the slider's top posts 0, so the setting follows a later width change", () => {
  assert.equal(lsbWire(61, 61), LSB_FULL, "full is 0 on the wire, not 61");
  assert.equal(lsbWire(62, 61), LSB_FULL, "…and so is anything past it");
  assert.equal(lsbWire(60, 61), 60);
  assert.equal(lsbWire(1, 61), 1);
  assert.equal(lsbWire(0, 61), LSB_FULL);
  assert.equal(lsbWire(Number.NaN, 61), LSB_FULL);
  // every slider position round-trips: position → wire → effective position
  for (let pos = 1; pos <= 61; pos++)
    assert.equal(lsbEffective(lsbWire(pos, 61), 61), pos, `position ${pos}`);
});

test("the on-time is a BOOT field, compared as EFFECTIVE values", () => {
  // a stored 30 against a panel still running the full 61
  const pend = wire(block({ lsb: 30 }));
  const s = panelDriverState(driverWire(pend), panelGeometryOf(pend));
  assert.equal(s.status, "pending");
  assert.deepEqual(s.changed, ["the LSB on-time"]);
  assert.match(s.live, /64×64 1\/32$/, "full on-time says nothing extra");
  // the running schedule, once it has rebooted into it
  const applied = wire(block({ lsb: 30 }, { ...LIVE, lsb: 30 }));
  const s2 = panelDriverState(driverWire(applied), panelGeometryOf(applied));
  assert.equal(s2.status, "live");
  assert.deepEqual(s2.changed, []);
  assert.match(s2.live, /· LSB 30 of 61 clocks$/, "a truncating schedule names itself");
  // a configured 0 and a live 61 are the SAME schedule — the device reports the
  // effective value, so comparing the raw numbers would say "reboot" forever
  const full = wire(block({ lsb: LSB_FULL }, { ...LIVE, lsb: 61 }));
  assert.equal(panelDriverState(driverWire(full), panelGeometryOf(full)).status, "live");
  // …and so are a configured value OVER the lit width and the width itself
  const over = wire(block({ lsb: 4000 }, { ...LIVE, lsb: 61 }));
  assert.equal(panelDriverState(driverWire(over), panelGeometryOf(over)).status, "live");
  // it does not mask the other boot fields, and reads in the card's order
  const both = wire(block({ lsb: 30, planes: 6 }));
  assert.deepEqual(panelDriverState(driverWire(both), panelGeometryOf(both)).changed, [
    "bit planes",
    "the LSB on-time",
  ]);
  // firmware between #525 and #789 reports neither half: nothing to compare,
  // and no phantom reboot on a device that simply cannot truncate
  const { lsb: _cfg, ...older } = block();
  const { lsb: _live, ...olderLive } = LIVE;
  const legacy = wire({ ...older, live: olderLive });
  assert.equal(panelDriverState(driverWire(legacy), panelGeometryOf(legacy)).status, "live");
  assert.equal(configuredDriver(legacy).lsb, LSB_FULL, "no field reads as full");
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
