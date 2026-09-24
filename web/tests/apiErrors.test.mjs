// Unit tests for the error-translation table (src/lib/apiErrors.ts) — the ONE
// place a device's terse `{"ok":false,"error":…}` becomes a sentence
// (Gitea #538 round 2, #600).
//
// Driving it here rather than through a browser is the point: the sentences
// depend on numbers the FORM knows (the board's ceiling, the chain the user
// typed), and a fixture says what they are in one line.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { explainApiError, fitTiles, group } from "../src/lib/apiErrors.ts";
import { psramLine } from "../src/lib/settingsCaps.ts";

test("group() is locale-independent thousands", () => {
  assert.equal(group(4096), "4,096");
  assert.equal(group(8192), "8,192");
  assert.equal(group(300), "300");
  assert.equal(group(1000000), "1,000,000");
});

test("the HUB75 ceiling states both numbers, the reason and a chain that fits", () => {
  const ex = explainApiError("pw*ph*cols*rows out of range for this board", {
    scope: "layout",
    maxPixels: 4096,
    chain: { pw: 64, ph: 64, cols: 2, rows: 1 },
  });
  // the two numbers, computed from the fields and max_pixels
  assert.match(ex.text, /8,192 px — this board tops out at 4,096\./);
  // the VERIFIED reason (#600): internal SRAM, not a setting anyone can raise
  assert.match(ex.text, /two bitplane DMA frame buffers \(28 KB each\)/);
  assert.match(ex.text, /60 MB\/s/);
  assert.match(ex.text, /#599/);
  // and the arrangement that does fit, in the wire's own grammar
  assert.match(ex.text, /two 32×64 tiles \(`matrix 32 64 2 1 tr row 0 0`\)/);
  // the device's own words are kept
  assert.equal(ex.details, "pw*ph*cols*rows out of range for this board");
  assert.equal(ex.field, "layout-cols");
});

test("a chain that cannot be halved into the budget offers no arrangement", () => {
  const ex = explainApiError("pw*ph*cols*rows out of range for this board", {
    scope: "layout",
    maxPixels: 4096,
    chain: { pw: 65, ph: 65, cols: 3, rows: 3 },
  });
  assert.match(ex.text, /38,025 px — this board tops out at 4,096\./);
  assert.ok(!/you can arrange/.test(ex.text), "no suggestion when none exists");
});

test("fitTiles halves the longer side until the whole chain fits", () => {
  assert.deepEqual(fitTiles({ pw: 64, ph: 64, cols: 2, rows: 1 }, 4096), { pw: 32, ph: 64 });
  assert.deepEqual(fitTiles({ pw: 64, ph: 64, cols: 1, rows: 1 }, 4096), { pw: 64, ph: 64 });
  assert.deepEqual(fitTiles({ pw: 64, ph: 64, cols: 2, rows: 2 }, 4096), { pw: 32, ph: 32 });
  assert.equal(fitTiles({ pw: 3, ph: 3, cols: 1000, rows: 1 }, 16), null);
});

test("a strip over the ceiling gets the same ceiling sentence from `pixels`", () => {
  const ex = explainApiError("pixels out of range for this board", {
    scope: "layout",
    maxPixels: 2048,
    pixels: 5000,
  });
  assert.match(ex.text, /5,000 px — this board tops out at 2,048\./);
  assert.equal(ex.field, "layout-pixels");
});

test("the line number rides in details, never in the sentence", () => {
  const ex = explainApiError("unknown line (want strip|matrix|map|out|proj1d|proj2d|proj3d)", {
    scope: "layout",
    line: 3,
  });
  assert.match(ex.text, /bug in the app/);
  assert.match(ex.details, /\(line 3\)$/);
});

test("every layout grammar error the engine can raise has a sentence", () => {
  // The list is `crates/luxel-core/src/layout.rs`'s `err(...)` literals.
  const raws = [
    "duplicate output index",
    "expected one of index|x|y|z|xy|xz|yz",
    "expected: strip [pixels]",
    "grid is larger than this board's pixel ceiling",
    "grid is wider or taller than 65535",
    "only one strip/matrix/map line per body",
    "output count must be at least 1",
    "output index is past this board's output count",
    "pin is reserved or not an output on this board",
    "pixels out of range for this board",
    "pw*ph*cols*rows out of range for this board",
    "this board has no configurable strip output",
    "this board is a matrix: strip is not a Layout it can take",
    "two outputs cannot share a data pin",
    "unknown line (want strip|matrix|map|out|proj1d|proj2d|proj3d)",
  ];
  for (const raw of raws) {
    const ex = explainApiError(raw, { scope: "layout", maxPixels: 4096 });
    assert.notEqual(
      ex.text,
      "The device refused this LED layout and kept the one it had.",
      `no table entry for: ${raw}`,
    );
    assert.equal(ex.details, raw);
    assert.ok(ex.text.length > 20, `too terse for: ${raw}`);
  }
});

test("an unknown message still gets the scope's sentence and the raw text", () => {
  const ex = explainApiError("kaboom", { scope: "mqtt", field: "mqtt-host" });
  assert.equal(ex.text, "The device refused these MQTT settings.");
  assert.equal(ex.details, "kaboom");
  assert.equal(ex.field, "mqtt-host");
});

test("an empty message never yields a blank banner", () => {
  const ex = explainApiError("", { scope: "wifi" });
  assert.match(ex.text, /WiFi/);
  assert.equal(ex.details, "no message");
});

test("a named subject is quoted back", () => {
  const ex = explainApiError("no such pattern", { scope: "pattern", subject: "Aurora 2D" });
  assert.match(ex.text, /“Aurora 2D”/);
});

test("a flash write that did not persist says what survives a reboot", () => {
  const ex = explainApiError("applied live, but the store refused to persist it", {
    scope: "layout",
  });
  assert.match(ex.text, /running now/);
  assert.match(ex.text, /gone after a reboot/);
});

// ---- the PSRAM readout (#538 round 2) ----

test("psramLine shows the number, not the word `present`", () => {
  assert.equal(psramLine(8 * 1024 * 1024, 8 * 1024 * 1024), "8.0 MB free of 8 MB");
  assert.equal(psramLine(7.4 * 1024 * 1024, 8 * 1024 * 1024), "7.4 MB free of 8 MB");
  assert.equal(psramLine(512 * 1024, 2 * 1024 * 1024), "512 KB free of 2 MB");
  assert.equal(psramLine(1.5 * 1024 * 1024, 1.5 * 1024 * 1024), "1.5 MB free of 1.5 MB");
});

test("psramLine is null when the device advertises an arena but reports no size", () => {
  assert.equal(psramLine(0, 0), null);
});

test("a scene save the device had no heap for says to stop the running scene", () => {
  const ex = explainApiError("scenes: not enough memory to save (6616 B free)", {
    scope: "scene",
    subject: "BC sprite",
  });
  // the number the device measured, grouped
  assert.match(ex.text, /6,616 bytes left/);
  // what the user can actually do about it, and that nothing was lost
  assert.match(ex.text, /stop it \(or play a lighter one\)/);
  assert.match(ex.text, /Nothing was changed\./);
  assert.equal(ex.field, "scene-save");
  assert.equal(ex.details, "scenes: not enough memory to save (6616 B free)");
});
