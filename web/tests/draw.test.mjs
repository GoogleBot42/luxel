// The scatter paint ORDER (src/lib/draw.ts) — the rule that stops an unlit
// pixel from punching a hole in a lit neighbour (Jeremy, 2026-09-19: "in
// custom map mode, in the preview, black scatter plot dots draw over other
// dots", Gitea #538).
//
// Run: `npm test` from web/ (node's runner + type stripping, so the .ts
// module is imported directly — no build step, no canvas, no DOM).
//
// Worth pinning because the old code was not obviously wrong: it DID sort
// back-to-front, which is the right rule for opaque dots of a solid object.
// A pixel that is off is not part of the object at all, and depth alone puts
// it in front of anything it happens to sit nearer than.
import test from "node:test";
import assert from "node:assert/strict";
import { isLit, paintOrder } from "../src/lib/draw.ts";

/** RGB bytes for a list of per-pixel triples. */
const px = (...triples) => new Uint8Array(triples.flat());

const BLACK = [0, 0, 0];
const RED = [255, 0, 0];
const FAINT = [0, 0, 1];

test("isLit: any non-zero channel counts, all-zero does not", () => {
  const p = px(BLACK, RED, FAINT);
  assert.equal(isLit(p, 0), false);
  assert.equal(isLit(p, 1), true);
  assert.equal(isLit(p, 2), true, "a single unit of blue is still on");
});

test("isLit: a pixel past the end of the buffer is not lit", () => {
  assert.equal(isLit(px(RED), 5), false);
});

test("every unlit point is painted before every lit one", () => {
  // depths deliberately interleaved: sorting by depth alone would put the
  // unlit #2 (depth 0.9, nearest) last, on top of the lit points
  const items = [
    { i: 0, depth: 0.1 },
    { i: 1, depth: 0.5 },
    { i: 2, depth: 0.9 },
    { i: 3, depth: 0.3 },
  ];
  const order = paintOrder(items, px(RED, BLACK, BLACK, RED)).map((q) => q.i);
  assert.deepEqual(order, [1, 2, 0, 3], "unlit 1,2 (by depth) then lit 0,3 (by depth)");
});

test("within each group the depth sort is intact — back to front", () => {
  const items = [
    { i: 0, depth: 0.7 },
    { i: 1, depth: -0.2 },
    { i: 2, depth: 0.1 },
  ];
  const order = paintOrder(items, px(RED, RED, RED)).map((q) => q.i);
  assert.deepEqual(order, [1, 2, 0]);
});

test("all-unlit and all-lit frames are pure depth sorts", () => {
  const items = [
    { i: 0, depth: 2 },
    { i: 1, depth: 1 },
  ];
  assert.deepEqual(
    paintOrder(items, px(BLACK, BLACK)).map((q) => q.i),
    [1, 0],
  );
  assert.deepEqual(
    paintOrder(items, px(RED, RED)).map((q) => q.i),
    [1, 0],
  );
});

test("the caller's array is not reordered in place", () => {
  const items = [
    { i: 0, depth: 1 },
    { i: 1, depth: 0 },
  ];
  paintOrder(items, px(RED, RED));
  assert.deepEqual(
    items.map((q) => q.i),
    [0, 1],
    "paintPoints indexes rig.pts by q.i, so the input order must survive",
  );
});

// ── The LED-panel rig (Gitea #786) ──────────────────────────────────────────
// `panelRig` is the whole geometry decision behind the look: how many device
// pixels a cell gets, how big the dot in it is, and when a grid is simply too
// dense to draw as discrete dots at all. Pure, so it is pinned here rather
// than by eye in a screenshot.
import { panelRig, parsePreviewStyle } from "../src/lib/draw.ts";

test("parsePreviewStyle accepts the two styles and nothing else", () => {
  assert.equal(parsePreviewStyle("squares"), "squares");
  assert.equal(parsePreviewStyle("panel"), "panel");
  for (const junk of ["", "Panel", 1, null, undefined, {}]) {
    assert.equal(parsePreviewStyle(junk), null, `${JSON.stringify(junk)} is not a style`);
  }
});

test("the composite surface is a whole number of cells, so dots stay aligned", () => {
  for (const [w, h] of [
    [8, 8],
    [16, 16],
    [32, 8],
    [64, 64],
    [128, 128],
    [64, 32],
    [17, 5],
  ]) {
    const r = panelRig(w, h);
    assert.ok(r, `${w}x${h} should draw as a panel`);
    assert.equal(r.width, w * r.scale);
    assert.equal(r.height, h * r.scale);
  }
});

test("the surface never exceeds the 640px cap, and a cell is never thinner than 2px", () => {
  for (let n = 1; n <= 320; n++) {
    const r = panelRig(n, n);
    assert.ok(r, `${n}x${n} should still draw as a panel`);
    assert.ok(r.scale >= 2, `${n}: scale ${r.scale}`);
    assert.ok(Math.max(r.width, r.height) <= 640, `${n}: ${r.width}x${r.height}`);
  }
});

test("a grid too dense for two device pixels per cell falls back (null)", () => {
  assert.equal(panelRig(321, 321), null);
  assert.equal(panelRig(4096, 1), null);
});

test("a non-grid or empty layout has no rig", () => {
  assert.equal(panelRig(0, 0), null, "an irregular Layout reports w=h=0");
  assert.equal(panelRig(-1, 8), null);
  assert.equal(panelRig(NaN, 8), null);
});

test("the dot leaves substrate around it and never vanishes", () => {
  for (const n of [8, 32, 64, 128, 200, 320]) {
    const r = panelRig(n, n);
    assert.ok(r.radius * 2 <= r.scale, `${n}: a dot may not overflow its cell`);
    assert.ok(r.radius >= 0.9, `${n}: radius ${r.radius} would disappear`);
  }
  // a roomy cell keeps the ~66% fill ratio the look is specified at
  const roomy = panelRig(64, 64);
  assert.ok(Math.abs((roomy.radius * 2) / roomy.scale - 0.66) < 0.01);
});
