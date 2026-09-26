// Unit tests for the device pattern LIBRARY half of `src/stores/device.ts` —
// `refreshDevicePatterns()` and the source sweep behind it.
//
// These are the store's half of the 2026-09-26 panel report ("sometimes the
// scene editor doesn't render any patterns, just text; reloading sort of fixes
// it"). Every layer's picture in the scene editor comes out of
// `devicePatterns`, the store is refreshed ON DEMAND and never polled, and the
// device answers `/api/patterns` with a 503 or an empty body whenever it is
// short of internal heap — so the two rules under test here are the only thing
// standing between one bad body and a session-long blank editor:
//
//   1. a FAILED read changes nothing (the `refreshStatus` rule);
//   2. a source that failed to arrive is asked for AGAIN, and lands in the
//      row that carries its id however many times the list has been rebuilt
//      since.
//
// The store is driven directly with a fake `DeviceSession` — `device` is a
// plain writable, so nothing here needs a browser.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { get } from "svelte/store";
import { PatternSourceError } from "../src/lib/device.ts";
import { device, devicePatterns, refreshDevicePatterns } from "../src/stores/device.ts";

/** A `DeviceSession` stand-in: only the two calls the library path makes. */
function fakeSession(opts) {
  const calls = { patterns: 0, source: [] };
  return {
    calls,
    base: "",
    async patterns() {
      calls.patterns++;
      return opts.patterns();
    },
    async patternSource(id) {
      calls.source.push(id);
      return opts.source(id);
    },
  };
}

/** Wait for a condition the background sweep satisfies (it is `void`ed by
 *  `refreshDevicePatterns`, so there is nothing to await directly). */
async function waitFor(pred, ms = 5000) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    if (pred()) return;
    await new Promise((r) => setTimeout(r, 10));
  }
  assert.fail("condition never became true");
}

function rows() {
  return get(devicePatterns).map((p) => ({ id: p.id, name: p.name, source: p.source }));
}

function reset() {
  device.set(null);
  devicePatterns.set([]);
}

test("a filled library survives a failed /api/patterns read", async () => {
  reset();
  let ok = true;
  const s = fakeSession({
    patterns: () => {
      if (!ok) throw new Error("patterns: HTTP 503");
      return [{ id: "a1", name: "one" }];
    },
    source: (id) => ({ id, name: "one", source: "hsv(0,1,1)" }),
  });
  device.set(s);
  await refreshDevicePatterns();
  await waitFor(() => rows()[0]?.source !== undefined);

  ok = false;
  await refreshDevicePatterns();
  // The row AND its cached source are still here: a read that failed taught us
  // nothing. Wiping the list was what left every scene layer without a source.
  assert.deepEqual(rows(), [{ id: "a1", name: "one", source: "hsv(0,1,1)" }]);
  reset();
});

test("only a successful empty read empties the library", async () => {
  reset();
  let list = [{ id: "b1", name: "one" }];
  const s = fakeSession({
    patterns: () => list,
    source: (id) => ({ id, name: "one", source: "// x" }),
  });
  device.set(s);
  await refreshDevicePatterns();
  assert.equal(rows().length, 1);
  list = [];
  await refreshDevicePatterns();
  assert.deepEqual(rows(), []);
  reset();
});

test("a source the device refuses transiently is asked for again", async () => {
  reset();
  let fails = 1;
  const s = fakeSession({
    patterns: () => [{ id: "c1", name: "one" }],
    source: (id) => {
      if (fails-- > 0) throw new PatternSourceError(`pattern ${id}: busy`, false);
      return { id, name: "one", source: "hsv(0,1,1)" };
    },
  });
  device.set(s);
  await refreshDevicePatterns();
  await waitFor(() => rows()[0]?.source === "hsv(0,1,1)");
  assert.equal(s.calls.source.length, 2, "should have retried exactly once");
  reset();
});

test("a pattern the device does not hold is not asked for twice", async () => {
  reset();
  const s = fakeSession({
    patterns: () => [{ id: "d1", name: "gone" }],
    source: (id) => {
      throw new PatternSourceError(`pattern ${id}: no such pattern`, true);
    },
  });
  device.set(s);
  await refreshDevicePatterns();
  // Nothing to wait for — assert the sweep has settled and stayed settled.
  await new Promise((r) => setTimeout(r, 900));
  assert.equal(s.calls.source.length, 1);
  assert.equal(rows()[0]?.source, undefined);
  reset();
});

test("a source that lands after the list was rebuilt is not orphaned", async () => {
  reset();
  // The sweep holds a row object; a refresh that arrives while a fetch is in
  // flight replaces every row with a NEW object. Writing the source into the
  // held object (what the code used to do) published a list without it, and
  // the layer stayed blank with nothing to retry it.
  const s = fakeSession({
    patterns: () => [{ id: "e1", name: "one" }],
    source: async (id) => {
      await new Promise((r) => setTimeout(r, 60));
      return { id, name: "one", source: "hsv(1,1,1)" };
    },
  });
  device.set(s);
  const first = refreshDevicePatterns();
  await first;
  await refreshDevicePatterns(); // rebuilds the rows mid-sweep
  await waitFor(() => rows()[0]?.source === "hsv(1,1,1)");
  reset();
});

test("two refreshes at once make ONE /api/patterns read", async () => {
  reset();
  // Opening the scene editor asks three times in a frame (the screen, the
  // picker, the sprite migration) — into a board with two sockets.
  const s = fakeSession({
    patterns: async () => {
      await new Promise((r) => setTimeout(r, 30));
      return [{ id: "f1", name: "one" }];
    },
    source: (id) => ({ id, name: "one", source: "// x" }),
  });
  device.set(s);
  await Promise.all([refreshDevicePatterns(), refreshDevicePatterns(), refreshDevicePatterns()]);
  assert.equal(s.calls.patterns, 1);
  reset();
});

test("an invalidating refresh is never coalesced away", async () => {
  reset();
  const s = fakeSession({
    patterns: () => [{ id: "g1", name: "one" }],
    source: (id) => ({ id, name: "one", source: `// ${s.calls.source.length}` }),
  });
  device.set(s);
  await refreshDevicePatterns();
  await waitFor(() => rows()[0]?.source !== undefined);
  const before = rows()[0].source;
  await refreshDevicePatterns(["g1"]); // the saver saying "this one changed"
  await waitFor(() => rows()[0]?.source !== undefined && rows()[0].source !== before);
  assert.equal(s.calls.source.length, 2);
  reset();
});
