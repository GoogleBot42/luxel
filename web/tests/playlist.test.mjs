// Unit tests for the playlist transport reconciler (src/lib/playlist.ts) — the
// rule that stops one early `GET /api/playlist` from latching the transport on
// "not playing" (Gitea #431).
//
// Run: `npm test` from web/ (node's built-in runner + type stripping, so the
// .ts module is imported directly — no build step, no test dependency).
//
// Why this is worth pinning: `POST /api/playlist/play` returns before the
// device has applied it (the mirror flips `playing` in its render loop; both
// the mirror and the firmware advance `index` there), so the UI's follow-up
// read can legitimately say `playing: false` right after a successful play.
// The transport must show what was asked for until the device agrees — but it
// must NOT do so forever, or a refused request (empty playlist) latches the
// other way.
import test from "node:test";
import assert from "node:assert/strict";
import {
  TRANSPORT_SETTLE_MS,
  normalizePlaylist,
  playlistWire,
  reconcileTransport,
  transportIntent,
} from "../src/lib/playlist.ts";

/** A `GET /api/playlist` read. */
const read = (playing, index = 0) => ({
  defaultSec: 5,
  crossfadeMs: 0,
  playing,
  index,
  items: [{ id: "a", name: "a", sec: null, controls: {} }],
});

test("no pending request: the device's state is taken as-is", () => {
  for (const playing of [true, false]) {
    const r = reconcileTransport(read(playing), undefined, 1000);
    assert.equal(r.playlist.playing, playing);
    assert.equal(r.intent, undefined);
  }
});

test("a stale read that beats the device does not clear the transport", () => {
  const intent = transportIntent(true, 1000);
  const r = reconcileTransport(read(false, 0), intent, 1050);
  assert.equal(r.playlist.playing, true, "still shows the play the user asked for");
  assert.equal(r.intent, intent, "and keeps waiting for the device to agree");
});

test("everything else in a stale read is still adopted (index, items)", () => {
  const r = reconcileTransport(read(false, 3), transportIntent(true, 1000), 1050);
  assert.equal(r.playlist.index, 3);
  assert.equal(r.playlist.items.length, 1);
});

test("the device agreeing retires the request", () => {
  const r = reconcileTransport(read(true), transportIntent(true, 1000), 1050);
  assert.equal(r.playlist.playing, true);
  assert.equal(r.intent, undefined, "no overlay left to go stale");
});

test("stop is protected the same way, in the other direction", () => {
  const pending = reconcileTransport(read(true), transportIntent(false, 1000), 1100);
  assert.equal(pending.playlist.playing, false);
  assert.notEqual(pending.intent, undefined);
  const settled = reconcileTransport(read(false), transportIntent(false, 1000), 1100);
  assert.equal(settled.playlist.playing, false);
  assert.equal(settled.intent, undefined);
});

test("a request the device never applies expires — the device wins", () => {
  const intent = transportIntent(true, 1000);
  const at = (now) => reconcileTransport(read(false), intent, now);
  assert.equal(at(1000 + TRANSPORT_SETTLE_MS - 1).playlist.playing, true, "inside the window");
  const expired = at(1000 + TRANSPORT_SETTLE_MS);
  assert.equal(expired.playlist.playing, false, "past it, believe the device");
  assert.equal(expired.intent, undefined, "and stop overlaying");
});

test("the settle window is long enough for a slow loop, short enough to notice", () => {
  assert.ok(TRANSPORT_SETTLE_MS >= 1000 && TRANSPORT_SETTLE_MS <= 5000, TRANSPORT_SETTLE_MS);
});

// ---- the wire (Gitea #478: scene items) ----
//
// `I S<id>` is how a playlist names a SCENE. A scene carries no values of its
// own — its layers do — so the serializer must never put a `C` or a `P` line
// under one, and the read-back must normalize the `controls` a scene item
// does not have.

test("a pattern item serializes to I/C/P, a scene item to I S<id> alone", () => {
  const wire = playlistWire({
    defaultSec: 8,
    crossfadeMs: 500,
    playing: true,
    index: 0,
    items: [
      { id: "5eed1c92", name: "Aurora 2D", kind: "pattern", sec: null, controls: { speed: [0.5] }, proj: "x" },
      { id: "5ce4e5ff", name: "Clock overlay", kind: "scene", sec: 8, controls: {}, layers: 2 },
    ],
  });
  assert.equal(
    wire,
    [
      "D 8",
      "X 500",
      "I 5eed1c92 -1",
      "C speed 32768",
      "P x",
      "I S5ce4e5ff 8",
    ].join("\n"),
  );
});

test("a scene item never emits C or P, even carrying them", () => {
  const wire = playlistWire({
    defaultSec: 0,
    crossfadeMs: 0,
    playing: false,
    index: 0,
    items: [
      { id: "5ce4e5ff", name: "S", kind: "scene", sec: null, controls: { speed: [1] }, proj: "y" },
    ],
  });
  assert.equal(wire, ["D 0", "X 0", "I S5ce4e5ff -1"].join("\n"));
});

test("an item with no kind is a pattern (a pre-#478 device)", () => {
  const pl = normalizePlaylist({
    defaultSec: 5,
    crossfadeMs: 0,
    playing: false,
    index: 0,
    items: [{ id: "a", name: "a", sec: null, controls: { speed: [1] } }],
  });
  assert.equal(pl.items[0].kind, "pattern");
  assert.deepEqual(pl.items[0].controls, { speed: [1] });
});

test("a scene item comes back without controls and reads as an empty set", () => {
  const pl = normalizePlaylist({
    defaultSec: 5,
    crossfadeMs: 0,
    playing: false,
    index: 0,
    items: [{ kind: "scene", id: "5ce4e5ff", name: "Clock overlay", layers: 2, sec: 8 }],
  });
  assert.equal(pl.items[0].kind, "scene");
  assert.deepEqual(pl.items[0].controls, {});
  assert.equal(pl.items[0].layers, 2);
  // and it round-trips back to the wire it came from
  assert.match(playlistWire(pl), /^I S5ce4e5ff 8$/m);
});
