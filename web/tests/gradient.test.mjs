// The colour-ramp editor's model (`src/lib/gradient.ts`) — the three rules the
// #787 bug inventory found losing the user's work. No DOM, no device.
//
// Each test names the inventory item it holds and reproduces its exact walk,
// because the redesign (step 2 of #787) replaces the widget and must inherit
// the rules:
//
//   §2  a colour edit's 250 ms trailing commit re-committed the list it had
//       CAPTURED, so a drag inside that window was reverted and a deleted stop
//       came back;
//   §3  removing one stop from a two-stop scene ramp fell through to "destroy
//       the whole ramp", which `Edit…` then reseeded black→white;
//   §4  `Number("")` is 0, so clearing the position field slammed the stop to
//       0 and clearing the amount field silently turned the palette off.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { fieldNumber, removeStopAt, trailingCommit } from "../src/lib/gradient.ts";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The seeded scene ramp the inventory trimmed down (`RampEditor.SEED` grown). */
const four = () => [
  { pos: 0, hex: "000000" },
  { pos: 85, hex: "ff0000" },
  { pos: 170, hex: "00ff00" },
  { pos: 255, hex: "ffffff" },
];

// ── §3 · removal at the floor ─────────────────────────────────────────────

test("§3 a scene ramp at minStops refuses the removal instead of clearing", () => {
  const two = [
    { pos: 0, hex: "112233" },
    { pos: 255, hex: "445566" },
  ];
  // the scene mount's floor (RampEditor.svelte passes minStops=2)
  assert.equal(removeStopAt(two, 0, 2), null);
  assert.equal(removeStopAt(two, 1, 2), null);
  // and the list the caller holds is untouched — no `clear`, no reseed
  assert.deepEqual(two, [
    { pos: 0, hex: "112233" },
    { pos: 255, hex: "445566" },
  ]);
});

test("§3 above the floor a removal still removes exactly one stop", () => {
  const r = removeStopAt(four(), 1, 2);
  assert.ok(r);
  assert.deepEqual(
    r.stops.map((s) => s.pos),
    [0, 170, 255],
  );
  // the selection follows the shortened list, never past its end
  assert.equal(r.picked, 1);
  const last = removeStopAt(four(), 3, 2);
  assert.ok(last);
  assert.equal(last.picked, 2);
});

test("§3 the device palette (minStops 0) keeps its empty-is-legal behaviour", () => {
  const one = [{ pos: 0, hex: "ff0000" }];
  const r = removeStopAt(one, 0, 0);
  assert.ok(r, "the last stop of a device palette must still be removable");
  assert.deepEqual(r.stops, []);
  assert.equal(r.picked, 0);
  // …and with nothing left there is nothing to remove
  assert.equal(removeStopAt([], 0, 0), null);
});

test("§3 an out-of-range index removes nothing (a stale selection, #8)", () => {
  assert.equal(removeStopAt(four(), 4, 2), null);
  assert.equal(removeStopAt(four(), -1, 2), null);
});

// ── §2 · the trailing colour commit ───────────────────────────────────────

test("§2 the trailing commit carries the CURRENT list, not the captured one", async () => {
  // recolour stop 0 (arms the latch), then drag stop 1 from 128 to 76 inside
  // the window: the commit must be the dragged list
  let live = [
    { pos: 0, hex: "0000ff" },
    { pos: 128, hex: "00ff00" },
  ];
  const committed = [];
  const latch = trailingCommit(
    30,
    () => live,
    (v) => committed.push(v),
  );
  latch.arm();
  live = [
    { pos: 0, hex: "0000ff" },
    { pos: 76, hex: "00ff00" },
  ];
  await sleep(80);
  assert.equal(committed.length, 1);
  assert.deepEqual(
    committed[0].map((s) => s.pos),
    [0, 76],
    "the drag inside the 250 ms window was reverted by the trailing commit",
  );
  assert.equal(latch.pending(), false);
});

test("§2 a later edit cancels the pending commit, so the deleted stop stays gone", async () => {
  let live = [
    { pos: 0, hex: "0000ff" },
    { pos: 128, hex: "00ff00" },
    { pos: 255, hex: "ffffff" },
  ];
  const committed = [];
  const latch = trailingCommit(
    30,
    () => live,
    (v) => committed.push(v),
  );
  latch.arm(); // the recolour
  assert.equal(latch.pending(), true);
  // the editor's own commit path (a Delete) — every emit cancels the latch
  const r = removeStopAt(live, 1, 2);
  assert.ok(r);
  live = r.stops;
  latch.cancel();
  await sleep(80);
  assert.deepEqual(committed, [], "the removal's own commit is the last word");
  assert.equal(latch.pending(), false);
});

test("§2 re-arming restarts the window rather than stacking commits", async () => {
  let live = [{ pos: 0, hex: "000000" }];
  let commits = 0;
  const latch = trailingCommit(
    40,
    () => live,
    () => commits++,
  );
  // a ColorPicker drag is a pointermove storm: many arms, one commit
  for (let i = 0; i < 6; i++) {
    latch.arm();
    await sleep(10);
  }
  live = [{ pos: 0, hex: "ff00ff" }];
  await sleep(120);
  assert.equal(commits, 1);
});

test("§2 cancel on destroy leaves nothing to fire", async () => {
  let commits = 0;
  const latch = trailingCommit(
    20,
    () => [],
    () => commits++,
  );
  latch.arm();
  latch.cancel();
  await sleep(60);
  assert.equal(commits, 0);
});

// ── §4 · an emptied number field ──────────────────────────────────────────

test("§4 an emptied or unparseable field is no change, not zero", () => {
  assert.equal(fieldNumber(""), null);
  assert.equal(fieldNumber("   "), null);
  assert.equal(fieldNumber("abc"), null);
  assert.equal(fieldNumber("-"), null);
  // `<input type=number>` hands over "" for a typed "1e" too
  assert.equal(fieldNumber("1e"), null);
});

test("§4 a real number still comes through, sign and spaces included", () => {
  assert.equal(fieldNumber("0"), 0);
  assert.equal(fieldNumber("77"), 77);
  assert.equal(fieldNumber(" 128 "), 128);
  // out of range is the CLAMP's business (moveStop / setAmount), not this one:
  // the field only decides whether the user said anything at all
  assert.equal(fieldNumber("999"), 999);
  assert.equal(fieldNumber("-40"), -40);
});
