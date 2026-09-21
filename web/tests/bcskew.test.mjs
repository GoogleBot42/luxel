// The bytecode-skew rule and the stale-store detection (src/lib/bcskew.ts).
//
// These are the decisions that keep the self-heal from making things worse,
// so they are tested as rules rather than through a browser: which direction
// of skew permits a recompile, and which patterns count as stale.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { bcSkew, parseStaleError, skewBanner, stalePatternIds } from "../src/lib/bcskew.ts";

const STALE = "bytecode format v5 (this build reads v6) — recompile the pattern";

test("agreeing versions are a match", () => {
  assert.equal(bcSkew(6, 6), "match");
});

test("a missing number on either side is unknown, never a match", () => {
  assert.equal(bcSkew(0, 6), "unknown"); // luxel.wasm predates lx_bc_format
  assert.equal(bcSkew(6, undefined), "unknown"); // firmware predates bc_format
  assert.equal(bcSkew(0, undefined), "unknown");
});

test("the two directions are told apart", () => {
  assert.equal(bcSkew(5, 6), "bundle-older"); // the #643 incident
  assert.equal(bcSkew(6, 5), "bundle-newer");
});

test("only the bundle-older banner offers the asset upload", () => {
  const older = skewBanner("bundle-older", 5, 6);
  assert.ok(older);
  assert.equal(older.offerAssets, true);
  assert.match(older.text, /compiles v5, device reads v6/);
  const newer = skewBanner("bundle-newer", 6, 5);
  assert.ok(newer);
  assert.equal(newer.offerAssets, false);
  assert.match(newer.text, /update the firmware/);
});

test("a match and an unknown say nothing", () => {
  assert.equal(skewBanner("match", 6, 6), null);
  assert.equal(skewBanner("unknown", 0, undefined), null);
});

test("the firmware's own wording parses", () => {
  assert.deepEqual(parseStaleError(STALE), { found: 5, reads: 6 });
  assert.equal(parseStaleError(null), null);
  assert.equal(parseStaleError("pixel count 300 exceeds the panel"), null);
});

const rows = [
  { id: "a1", name: "Aurora" },
  { id: "b2", name: "Fairies" },
  { id: "c3", name: "Rainbow" },
];

test("the structured flag wins outright when the firmware reports it", () => {
  const flagged = [{ ...rows[0], stale: true }, rows[1], { ...rows[2], stale: true }];
  assert.deepEqual(stalePatternIds(flagged, "b2", STALE, new Map([["b2", STALE]])), ["a1", "c3"]);
});

test("without flags it falls back to vmerr and the playlist verdicts", () => {
  const ids = stalePatternIds(rows, "a1", STALE, new Map([["c3", STALE]]));
  assert.deepEqual(ids.sort(), ["a1", "c3"]);
});

test("it never recompiles on suspicion", () => {
  // nothing flagged, no vmerr, no invalid items: nothing is stale
  assert.deepEqual(stalePatternIds(rows, "a1", null, new Map()), []);
  // a playlist item invalid for a DIFFERENT reason is not stale bytecode
  assert.deepEqual(
    stalePatternIds(rows, "a1", null, new Map([["b2", "assert failed: pixelCount >= 512"]])),
    [],
  );
  // an id the store does not hold is ignored rather than fetched
  assert.deepEqual(stalePatternIds(rows, "zz", STALE, new Map([["yy", STALE]])), []);
});
