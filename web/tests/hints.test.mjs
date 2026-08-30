// Unit tests for the `//#` control-bounds parser.
// Run: `npm test` from web/ (node's built-in test runner + type stripping,
// so hints.ts is imported directly — no build step, no test dependency).
import test from "node:test";
import assert from "node:assert/strict";
import { parseControlHints } from "../src/lib/hints.ts";

test("trailing directive on the export line", () => {
  const h = parseControlHints(
    `export function sliderSpeed(v) { speed = v }  //# min=0 max=5 step=0.5 default=2\n`,
  );
  assert.deepEqual(h.get("sliderSpeed"), { min: 0, max: 5, step: 0.5, default: 2 });
});

test("directive on its own line above the export", () => {
  const h = parseControlHints(
    `//# min=0 max=1 step=0.01 default=0.5\nexport function sliderSpeed(v) { speedMul = 1 + v * 4 }\n`,
  );
  assert.deepEqual(h.get("sliderSpeed"), { min: 0, max: 1, step: 0.01, default: 0.5 });
});

test("own-line directive tolerates indentation on both lines", () => {
  const h = parseControlHints(`  //# min=2 max=8\n\texport function inputNumberN(v) { n = v }\n`);
  assert.deepEqual(h.get("inputNumberN"), { min: 2, max: 8 });
});

test("negative and fractional bounds", () => {
  const h = parseControlHints(`//# min=-1.5 max=1.5 default=-0.25\nexport function sliderTilt(v) {}\n`);
  assert.deepEqual(h.get("sliderTilt"), { min: -1.5, max: 1.5, default: -0.25 });
});

test("multi-line function bodies do not steal the next control's directive", () => {
  const src = [
    "//# min=0 max=5 default=1",
    "export function sliderA(v) {",
    "  a = v",
    "}",
    "//# min=10 max=20 default=15",
    "export function sliderB(v) {",
    "  b = v",
    "}",
  ].join("\n");
  const h = parseControlHints(src);
  assert.deepEqual(h.get("sliderA"), { min: 0, max: 5, default: 1 });
  assert.deepEqual(h.get("sliderB"), { min: 10, max: 20, default: 15 });
});

test("both placements on one control merge, own line wins on shared keys", () => {
  const h = parseControlHints(`//# max=9\nexport function sliderX(v) {} //# min=1 max=2 step=1\n`);
  assert.deepEqual(h.get("sliderX"), { min: 1, max: 9, step: 1 });
});

test("controls without a directive are absent", () => {
  const h = parseControlHints(`export function sliderPlain(v) { p = v }\n`);
  assert.equal(h.has("sliderPlain"), false);
});

test("a directive separated by a blank line does not bind", () => {
  const h = parseControlHints(`//# min=3 max=4\n\nexport function sliderGap(v) {}\n`);
  assert.equal(h.has("sliderGap"), false);
});

test("unknown keys are ignored, known ones still parse", () => {
  const h = parseControlHints(`//# min=0 max=3 wobble=7 label=nope\nexport function sliderY(v) {}\n`);
  assert.deepEqual(h.get("sliderY"), { min: 0, max: 3 });
});

test("non-export functions are not controls", () => {
  const h = parseControlHints(`//# min=0 max=3\nfunction helper(v) {}\n`);
  assert.equal(h.size, 0);
});

test("readouts and toggles work in both placements", () => {
  const h = parseControlHints(
    ["//# default=1", "export function toggleMirror(on) { mirror = on }", "export function gaugeLoad() { return l } //# min=0 max=100"].join("\n"),
  );
  assert.deepEqual(h.get("toggleMirror"), { default: 1 });
  assert.deepEqual(h.get("gaugeLoad"), { min: 0, max: 100 });
});
