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
