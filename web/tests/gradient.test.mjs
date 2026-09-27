// The colour-ramp editor's MODEL (`src/lib/gradient.ts`) — every rule the #787
// redesign stands on, with no DOM and no device in any of it. `web/tests`
// cannot import a `.svelte` file, so a rule that lives only in the component
// is a rule nothing can test, and the #787 inventory is largely a list of what
// that costs.
//
// Each test names the inventory item it holds and reproduces its exact walk.
// The first three sections are the damage control PR #853 landed before the
// redesign; everything from "the redesign's model" down is the redesign's own,
// and the last section is the re-read decision that a real device round-trip
// found a hole in (see it for the story).
//
// The #853 three:
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
import {
  MAX_STOPS,
  MIN_STOPS,
  RAMP_PRESETS,
  addStopAt,
  assignIds,
  clusterStops,
  fieldNumber,
  history,
  legalStopCount,
  lumaWedge,
  lutHex,
  matchingPreset,
  moveStopTo,
  nextInCluster,
  normalizeHex,
  rampLut,
  remapFrame,
  removeStopAt,
  removeStopId,
  sameStops,
  setStopHex,
  sortStops,
  stopBytes,
  stripIds,
  syncVerdict,
  trailingCommit,
  widestGapPos,
} from "../src/lib/gradient.ts";

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

// ── the redesign's model (step 3) ─────────────────────────────────────────
//
// Everything above is the step-1 damage control that PR #853 landed. What
// follows is the REDESIGN's own rules — where a new stop goes, which stops are
// reachable when two coincide, what a legal ramp is, what a preset does, and
// undo — all of them decisions with no canvas and no pointer in them, which is
// why they live in `src/lib/gradient.ts` and not in the component.

const ids = (list) => list.map((s) => s.id);
const widestGap = (list) => widestGapPos(list);
const poss = (list) => list.map((s) => s.pos);

// ── item 21 · ONE legality rule ───────────────────────────────────────────

test("item 21 a ramp is 0 stops (off) or at least 2 — never 1", () => {
  assert.equal(MIN_STOPS, 2);
  assert.equal(legalStopCount(0), true);
  assert.equal(legalStopCount(1), false, "the rule the UI prints, and the one scene.rs enforces");
  assert.equal(legalStopCount(2), true);
  assert.equal(legalStopCount(32), true);
});

test("item 21 the floor is the SAME at both mounts now", () => {
  const two = assignIds([
    { pos: 0, hex: "000000" },
    { pos: 255, hex: "ffffff" },
  ]);
  // the device palette used to pass minStops=0 and the scene mount 2
  assert.equal(removeStopId(two, two[0].id), null);
  assert.equal(removeStopId(two, two[1].id), null);
});

// ── item 5 · + Add stop lands in the widest gap ───────────────────────────

test("item 5 + Add stop lands in the middle of the WIDEST gap, never past the last", () => {
  const list = assignIds([
    { pos: 0, hex: "000000" },
    { pos: 20, hex: "ff0000" },
    { pos: 255, hex: "ffffff" },
  ]);
  const lut = rampLut(stripIds(list), 100);
  const at = widestGap(list);
  assert.equal(at, 137, "the 20..255 gap, halved");
  const r = addStopAt(list, at, lut);
  assert.ok(r);
  assert.deepEqual(poss(r.stops), [0, 20, 137, 255]);
  assert.ok(
    r.stops.every((s) => s.pos <= 255),
    "and nothing lands past the last stop — which is how 30 invisible stops piled up at 255",
  );
});

test("item 5 the eight clicks from the inventory no longer stack at 255", () => {
  // the repro: eight `add stop` presses from a two-stop ramp.
  let list = assignIds([
    { pos: 0, hex: "000000" },
    { pos: 255, hex: "ffffff" },
  ]);
  for (let i = 0; i < 8; i++) {
    const at = widestGap(list);
    assert.notEqual(at, null);
    const r = addStopAt(list, at, rampLut(stripIds(list), 100));
    assert.ok(r);
    list = r.stops;
  }
  assert.equal(list.length, 10);
  const dupes = poss(list).length - new Set(poss(list)).size;
  assert.equal(dupes, 0, `every added stop is visible and draggable: ${JSON.stringify(poss(list))}`);
});

test("item 5 there is no gap to add into below the two-stop minimum, or at the cap", () => {
  assert.equal(widestGap(assignIds([])), null);
  assert.equal(widestGap(assignIds([{ pos: 0, hex: "000000" }])), null);
  const full = assignIds(
    Array.from({ length: MAX_STOPS }, (_, i) => ({ pos: i * 8, hex: "010203" })),
  );
  assert.equal(widestGap(full), null, "the cap, which the button states as its data-reason");
  assert.equal(addStopAt(full, 4, rampLut(stripIds(full), 100)), null);
});

// ── item 10 · adding a stop never changes the gradient ────────────────────

test("item 10 a stop added anywhere on the bar takes the colour already there", () => {
  const list = assignIds([
    { pos: 0, hex: "1b0e3a" },
    { pos: 96, hex: "c23a6b" },
    { pos: 176, hex: "f7e08a" },
  ]);
  const before = rampLut(stripIds(list), 100);
  for (const at of [10, 50, 96, 120, 176, 217, 255]) {
    const r = addStopAt(list, at, before);
    assert.ok(r);
    const after = rampLut(stripIds(r.stops), 100);
    const worst = Math.max(...Array.from(after, (v, i) => Math.abs(v - before[i])));
    // INSIDE the span the new stop's colour is the table's own reading, and
    // the table quantizes with `floor(v·255)` — so re-interpolating through it
    // can move a neighbouring entry by one unit, which is invisible. What the
    // old editor did was not that: it added a BLACK stop above the last one
    // and moved 88 of 256 entries by up to 255 (inventory item 10).
    assert.ok(worst <= 1, `adding at ${at} moved the table by ${worst}`);
  }
  // and in the CLAMPED zones it is exact, because the stop takes the end
  // stop's own hex rather than the table's rounding of it
  for (const at of [0, 176, 200, 255]) {
    const r = addStopAt(list, at, before);
    assert.ok(r);
    assert.deepEqual(
      Array.from(rampLut(stripIds(r.stops), 100)),
      Array.from(before),
      `adding at ${at} — a clamped zone — must not move the table at all`,
    );
  }
});

test("item 10 in the CLAMPED zones the new stop is the end stop's own hex", () => {
  // the LUT quantizes with floor(v·255), so `f7e08a` reads back as `f6df89`;
  // re-inserting the table's reading would tilt the plateau by one unit
  const list = assignIds([
    { pos: 0, hex: "1b0e3a" },
    { pos: 176, hex: "f7e08a" },
  ]);
  const lut = rampLut(stripIds(list), 100);
  assert.equal(lutHex(lut, 200), "f6df89", "what the table says at 200");
  const r = addStopAt(list, 200, lut);
  assert.ok(r);
  assert.equal(r.stops[2].hex, "f7e08a", "but the STOP takes the last stop's own colour");
});

// ── item 15 · stops that sit on top of each other ─────────────────────────

test("item 15 stops within the tolerance collapse into ONE handle with a count", () => {
  const list = assignIds([
    { pos: 0, hex: "000000" },
    { pos: 176, hex: "ff0000" },
    { pos: 176, hex: "00ff00" },
    { pos: 177, hex: "0000ff" },
    { pos: 255, hex: "ffffff" },
  ]);
  const c = clusterStops(list, 2);
  assert.deepEqual(
    c.map((x) => [x.pos, x.ids.length]),
    [
      [0, 1],
      [176, 3],
      [255, 1],
    ],
  );
});

test("item 15 `next of N` cycles the selection through a cluster and wraps", () => {
  const list = assignIds([
    { pos: 100, hex: "ff0000" },
    { pos: 100, hex: "00ff00" },
    { pos: 100, hex: "0000ff" },
  ]);
  const c = clusterStops(list, 0);
  assert.equal(c.length, 1);
  const [a, b, d] = c[0].ids;
  assert.equal(nextInCluster(c, a), b);
  assert.equal(nextInCluster(c, b), d);
  assert.equal(nextInCluster(c, d), a, "…and wraps, so the one underneath is always reachable");
  assert.equal(nextInCluster(c, 999999), null);
});

test("item 15 a zero tolerance still groups stops at the SAME position", () => {
  const list = assignIds([
    { pos: 8, hex: "111111" },
    { pos: 8, hex: "222222" },
  ]);
  assert.equal(clusterStops(list, 0).length, 1);
});

// ── item 8 · selection is an identity, not an index ───────────────────────

test("item 8 a stop keeps its identity across a re-sort, so a drag never swaps hold", () => {
  const list = assignIds([
    { pos: 0, hex: "aaaaaa" },
    { pos: 100, hex: "bbbbbb" },
    { pos: 200, hex: "cccccc" },
  ]);
  const held = list[0].id;
  const moved = moveStopTo(list, held, 240);
  assert.deepEqual(poss(moved), [100, 200, 240]);
  assert.equal(moved[2].id, held, "the stop being dragged is still the one at the pointer");
  assert.equal(moved[2].hex, "aaaaaa");
});

test("item 8 an edit aimed at a stop that is GONE does nothing at all", () => {
  const list = assignIds([
    { pos: 0, hex: "aaaaaa" },
    { pos: 128, hex: "bbbbbb" },
    { pos: 255, hex: "cccccc" },
  ]);
  const doomed = list[1].id;
  const after = removeStopId(list, doomed);
  assert.ok(after);
  // the old editor held an INDEX, so slot 1 now aliased the stop at 255 and
  // every edit through the panel silently moved the wrong one
  assert.deepEqual(moveStopTo(after.stops, doomed, 77), after.stops);
  assert.deepEqual(setStopHex(after.stops, doomed, "ffffff"), after.stops);
  assert.equal(removeStopId(after.stops, doomed), null);
});

test("item 8 removing a stop hands the selection to a stop that still exists", () => {
  const list = assignIds([
    { pos: 0, hex: "aaaaaa" },
    { pos: 85, hex: "bbbbbb" },
    { pos: 170, hex: "cccccc" },
    { pos: 255, hex: "dddddd" },
  ]);
  const r = removeStopId(list, list[3].id);
  assert.ok(r);
  assert.ok(ids(r.stops).includes(r.select), "never an id nothing holds");
});

// ── item 18 · presets, reset, undo ────────────────────────────────────────

test("item 18 every preset is a LEGAL ramp and is recognised back", () => {
  assert.equal(RAMP_PRESETS.length, 6);
  for (const p of RAMP_PRESETS) {
    assert.ok(legalStopCount(p.stops.length), `${p.name} has ${p.stops.length} stops`);
    assert.ok(p.stops.length <= MAX_STOPS);
    assert.deepEqual(poss(sortStops(p.stops)), poss(p.stops), `${p.name} is sorted`);
    for (const s of p.stops) {
      assert.equal(normalizeHex(s.hex), s.hex, `${p.name}: ${s.hex} is already the wire spelling`);
      assert.ok(s.pos >= 0 && s.pos <= 255);
    }
    assert.equal(matchingPreset(p.stops), p.name);
  }
  assert.equal(matchingPreset([]), null);
});

test("item 18 `To black` is where the pre-#787 engine behaviour lives now", () => {
  // Jeremy's decision (2026-09-27): the ramp clamps at BOTH ends, and "the
  // brightest pixels go dark" is something you say, not something you discover
  const toBlack = RAMP_PRESETS.find((p) => p.name === "To black");
  assert.ok(toBlack);
  assert.equal(toBlack.stops[toBlack.stops.length - 1].hex, "000000");
  assert.equal(toBlack.stops[toBlack.stops.length - 1].pos, 255);
  const lut = rampLut(toBlack.stops, 100);
  assert.deepEqual([lut[765], lut[766], lut[767]], [0, 0, 0]);
});

test("item 18 undo is bounded, ordered, and knows when it is empty", () => {
  const h = history("a", 3);
  assert.equal(h.canUndo(), false);
  assert.equal(h.undo(), null);
  assert.equal(h.base(), "a");
  h.push("a");
  h.push("b");
  h.push("c");
  assert.equal(h.depth(), 3);
  h.push("d"); // over the limit: the oldest falls off, the newest stays
  assert.equal(h.depth(), 3);
  assert.equal(h.undo(), "d");
  assert.equal(h.undo(), "c");
  assert.equal(h.undo(), "b");
  assert.equal(h.canUndo(), false);
  h.reset("z");
  assert.equal(h.base(), "z");
  assert.equal(h.depth(), 0);
});

// ── the wire, and the table ───────────────────────────────────────────────

test("stopBytes is the four bytes per stop the device takes, ascending", () => {
  const b = stopBytes([
    { pos: 255, hex: "#F7E08A" },
    { pos: 0, hex: "000000" },
  ]);
  assert.deepEqual(Array.from(b), [0, 0, 0, 0, 255, 0xf7, 0xe0, 0x8a], "sorted, and `#`-tolerant");
  assert.deepEqual(Array.from(stopBytes([])), []);
});

test("an empty ramp is the identity table — `no ramp` means `leave the pixel alone`", () => {
  // `sample_palette` returns `[v, v, v]` for an empty list and the cook then
  // does the 16.16 round trip (`trunc(i·65536/255)` in, `floor(v·255/65536)`
  // out), which lands one unit low in the middle. That is the ENGINE's own
  // arithmetic, pinned here rather than wished away — and it never reaches a
  // pixel, because with no stops `palette_on()` is false and the stage does
  // not run at all.
  const lut = rampLut([], 100);
  for (let i = 0; i < 256; i++) {
    const v = lut[i * 3];
    assert.deepEqual([lut[i * 3 + 1], lut[i * 3 + 2]], [v, v], `entry ${i} is grey`);
    assert.ok(v === i || v === i - 1, `entry ${i} reads ${v}`);
  }
  assert.equal(lut[0], 0);
  assert.equal(lut[255 * 3], 255, "and the two ends are exact");
});

test("the ramp CLAMPS at both ends (#787, Jeremy's option A)", () => {
  const stops = [
    { pos: 32, hex: "1b0e3a" },
    { pos: 176, hex: "f7e08a" },
  ];
  const lut = rampLut(stops, 100);
  const at = (i) => [lut[i * 3], lut[i * 3 + 1], lut[i * 3 + 2]];
  assert.deepEqual(at(0), at(32), "below the first stop its colour continues (unchanged)");
  assert.deepEqual(at(255), at(176), "and above the LAST stop it continues too — this is the change");
  assert.notDeepEqual(at(255), [0, 0, 0], "it used to be black up there");
  for (let i = 176; i <= 255; i++) assert.deepEqual(at(i), at(176), `entry ${i}`);
});

test("amount 0 is a no-op and amount 100 is a full replace", () => {
  const stops = [
    { pos: 0, hex: "ff0000" },
    { pos: 255, hex: "0000ff" },
  ];
  const off = rampLut(stops, 0);
  for (const i of [0, 90, 255]) assert.deepEqual([off[i * 3], off[i * 3 + 1], off[i * 3 + 2]], [i, i, i]);
  const full = rampLut(stops, 100);
  assert.deepEqual([full[0], full[1], full[2]], [255, 0, 0]);
  assert.deepEqual([full[765], full[766], full[767]], [0, 0, 255]);
});

test("remapFrame is the LEDs cell: the pattern's pixels through the table", () => {
  const stops = [
    { pos: 0, hex: "ff0000" },
    { pos: 255, hex: "00ff00" },
  ];
  const lut = rampLut(stops, 100);
  const px = new Uint8Array([0, 0, 0, 255, 255, 255, 10, 20, 30]);
  const full = remapFrame(px, lut, 100);
  assert.deepEqual(Array.from(full.slice(0, 3)), [255, 0, 0], "a black pixel takes the left end");
  assert.deepEqual(Array.from(full.slice(3, 6)), [0, 255, 0], "a white one takes the right end");
  const none = remapFrame(px, lut, 0);
  assert.deepEqual(Array.from(none), Array.from(px), "amount 0 leaves the frame alone");
});

test("the brightness wedge stands in for a pattern that is not running", () => {
  const w = lumaWedge(256);
  assert.equal(w.length, 768);
  assert.deepEqual([w[0], w[1], w[2]], [0, 0, 0]);
  assert.deepEqual([w[765], w[766], w[767]], [255, 255, 255]);
});

test("sameStops is spelling-insensitive, which is what makes echo detection work", () => {
  assert.equal(sameStops([{ pos: 0, hex: "#FF0000" }], [{ pos: 0, hex: "ff0000" }]), true);
  assert.equal(sameStops([{ pos: 0, hex: "ff0000" }], [{ pos: 1, hex: "ff0000" }]), false);
  assert.equal(sameStops([], []), true);
});

// ── item 1 · re-reading the owner's value while the editor is open ────────
//
// The worst thing in the inventory: the old editor read the device's palette
// once at mount and never again, so an edit made twenty minutes later pushed
// the STALE stops and the STALE amount over whatever the device actually held.
// The fix has two halves — the Settings tab polls (PR #853), and the editor
// decides what to DO with what it reads. This is that decision, and it has one
// trap: an editor whose own optimistic `input` is handed straight back by its
// mount must not raise a conflict against ITSELF. It did, once, because a
// single emitter forgot to record `mine` — and once the strip was up every
// later write was muted, silently.

const A = [
  { pos: 0, hex: "000000" },
  { pos: 255, hex: "ffffff" },
];
const B = [
  { pos: 0, hex: "0000ff" },
  { pos: 128, hex: "ffff00" },
];
const base = {
  incoming: A,
  incomingAmount: 100,
  loaded: A,
  loadedAmount: 100,
  mine: A,
  workAmount: 100,
  edited: false,
};

test("item 1 an echo of what the editor last COMMITTED is not news", () => {
  assert.equal(syncVerdict(base), "ignore");
  assert.equal(syncVerdict({ ...base, edited: true }), "ignore");
});

test("item 1 an echo of the editor's own OPTIMISTIC input is never a conflict", () => {
  // mid-edit: the device still holds `A` (that is `loaded`), the editor has
  // moved on to `B` and its mount handed `B` straight back through the prop
  assert.equal(
    syncVerdict({ ...base, incoming: B, loaded: A, mine: B, edited: true }),
    "ignore",
    "the strip must not appear against the editor itself",
  );
  // …and the amount half of the same case
  assert.equal(
    syncVerdict({ ...base, incoming: B, incomingAmount: 60, loaded: A, mine: B, workAmount: 60, edited: true }),
    "ignore",
  );
});

test("item 1 with nothing edited the editor simply follows the device", () => {
  assert.equal(syncVerdict({ ...base, incoming: B, mine: A, edited: false }), "adopt");
  assert.equal(syncVerdict({ ...base, incomingAmount: 40, edited: false }), "adopt");
});

test("item 1 a real change under local edits is a CONFLICT — nothing is sent", () => {
  assert.equal(syncVerdict({ ...base, incoming: B, mine: A, edited: true }), "conflict");
  // the amount alone moving is a conflict too: the inventory's repro lost the
  // user's 40 % as well as their colours
  assert.equal(syncVerdict({ ...base, incomingAmount: 40, edited: true }), "conflict");
});

test("item 1 the first read of all adopts, whatever it says", () => {
  assert.equal(syncVerdict({ ...base, loaded: null, mine: null, edited: false }), "adopt");
  assert.equal(syncVerdict({ ...base, incoming: [], loaded: null, mine: null, edited: false }), "adopt");
});

test("item 1 `mine` alone is enough — an emitter that forgets it breaks the control", () => {
  // this is the regression: `onColor` used to dispatch `input` without
  // recording `mine`, so its own echo matched neither and became a conflict on
  // the FIRST colour change of any session
  assert.equal(
    syncVerdict({ ...base, incoming: B, loaded: A, mine: null, edited: true }),
    "conflict",
    "…which is what the bug looked like",
  );
  assert.equal(
    syncVerdict({ ...base, incoming: B, loaded: A, mine: B, edited: true }),
    "ignore",
    "…and what recording it fixes",
  );
});
