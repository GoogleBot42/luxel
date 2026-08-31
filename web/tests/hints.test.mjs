// Unit tests for the `//#` control-bounds parser.
// Run: `npm test` from web/ (node's built-in test runner + type stripping,
// so hints.ts is imported directly — no build step, no test dependency).
//
// There are TWO implementations: the typed one the playground imports, and the
// plain-JS twin the verify harness (tools/verify/snap.mjs) and the review UI
// import. Every case below runs against both, so a fix landing in one and not
// the other fails here instead of silently changing what a `//#` directive
// means depending on who read it.
import test from "node:test";
import assert from "node:assert/strict";
import { parseControlHints } from "../src/lib/hints.ts";
import {
  parseControlHints as parseControlHintsJs,
  directiveRange,
} from "../../tools/verify/hints.mjs";

/** Assert both implementations agree, and that they say what we expect. */
function hints(source) {
  const ts = parseControlHints(source);
  const js = parseControlHintsJs(source);
  assert.deepEqual([...js], [...ts], `tools/verify/hints.mjs diverged from hints.ts on:\n${source}`);
  return ts;
}

test("trailing directive on the export line", () => {
  const h = hints(
    `export function sliderSpeed(v) { speed = v }  //# min=0 max=5 step=0.5 default=2\n`,
  );
  assert.deepEqual(h.get("sliderSpeed"), { min: 0, max: 5, step: 0.5, default: 2 });
});

test("directive on its own line above the export", () => {
  const h = hints(
    `//# min=0 max=1 step=0.01 default=0.5\nexport function sliderSpeed(v) { speedMul = 1 + v * 4 }\n`,
  );
  assert.deepEqual(h.get("sliderSpeed"), { min: 0, max: 1, step: 0.01, default: 0.5 });
});

test("own-line directive tolerates indentation on both lines", () => {
  const h = hints(`  //# min=2 max=8\n\texport function inputNumberN(v) { n = v }\n`);
  assert.deepEqual(h.get("inputNumberN"), { min: 2, max: 8 });
});

test("negative and fractional bounds", () => {
  const h = hints(`//# min=-1.5 max=1.5 default=-0.25\nexport function sliderTilt(v) {}\n`);
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
  const h = hints(src);
  assert.deepEqual(h.get("sliderA"), { min: 0, max: 5, default: 1 });
  assert.deepEqual(h.get("sliderB"), { min: 10, max: 20, default: 15 });
});

test("both placements on one control merge, own line wins on shared keys", () => {
  const h = hints(`//# max=9\nexport function sliderX(v) {} //# min=1 max=2 step=1\n`);
  assert.deepEqual(h.get("sliderX"), { min: 1, max: 9, step: 1 });
});

test("controls without a directive are absent", () => {
  const h = hints(`export function sliderPlain(v) { p = v }\n`);
  assert.equal(h.has("sliderPlain"), false);
});

test("a directive separated by a blank line does not bind", () => {
  const h = hints(`//# min=3 max=4\n\nexport function sliderGap(v) {}\n`);
  assert.equal(h.has("sliderGap"), false);
});

test("unknown keys are ignored, known ones still parse", () => {
  const h = hints(`//# min=0 max=3 wobble=7 label=nope\nexport function sliderY(v) {}\n`);
  assert.deepEqual(h.get("sliderY"), { min: 0, max: 3 });
});

test("non-export functions are not controls", () => {
  const h = hints(`//# min=0 max=3\nfunction helper(v) {}\n`);
  assert.equal(h.size, 0);
});

test("readouts and toggles work in both placements", () => {
  const h = hints(
    ["//# default=1", "export function toggleMirror(on) { mirror = on }", "export function gaugeLoad() { return l } //# min=0 max=100"].join("\n"),
  );
  assert.deepEqual(h.get("toggleMirror"), { default: 1 });
  assert.deepEqual(h.get("gaugeLoad"), { min: 0, max: 100 });
});

// directiveRange() — what snap.mjs --probe-controls sweeps a dial across
// (tools/verify/snap.mjs, Gitea #180). Harness-side only, so it has no twin.

test("range midpoint snaps to the step grid", () => {
  assert.deepEqual(directiveRange({ min: 1, max: 8, step: 1 }), { min: 1, max: 8, mid: 5 });
  assert.deepEqual(directiveRange({ min: 10, max: 600, step: 10 }), {
    min: 10,
    max: 600,
    mid: 310,
  });
});

test("range midpoint is the plain mean without a usable step", () => {
  assert.deepEqual(directiveRange({ min: 1, max: 60 }), { min: 1, max: 60, mid: 30.5 });
  assert.deepEqual(directiveRange({ min: 0, max: 4, step: 0 }), { min: 0, max: 4, mid: 2 });
});

test("a step coarser than the range leaves the midpoint alone", () => {
  // Snapping would land on an END, which would probe one end twice and never
  // sample the middle at all.
  assert.deepEqual(directiveRange({ min: 0, max: 1, step: 1 }), { min: 0, max: 1, mid: 0.5 });
});

test("float dust is trimmed from the midpoint", () => {
  assert.deepEqual(directiveRange({ min: 0.1, max: 0.5 }), { min: 0.1, max: 0.5, mid: 0.3 });
});

test("a one-sided directive takes the raw default for the other end", () => {
  assert.deepEqual(directiveRange({ min: 0.2 }), { min: 0.2, max: 1, mid: 0.6 });
  assert.deepEqual(directiveRange({ max: 10 }), { min: 0, max: 10, mid: 5 });
});

test("negative ranges work", () => {
  assert.deepEqual(directiveRange({ min: -1.5, max: 1.5 }), { min: -1.5, max: 1.5, mid: 0 });
});

test("no usable range → null, so the caller falls back", () => {
  assert.equal(directiveRange(undefined), null);
  assert.equal(directiveRange({}), null);
  assert.equal(directiveRange({ step: 1, default: 3 }), null); // neither end declared
  assert.equal(directiveRange({ min: 5, max: 5 }), null); // a point, not a range
  assert.equal(directiveRange({ min: 8, max: 1 }), null); // inverted
});
