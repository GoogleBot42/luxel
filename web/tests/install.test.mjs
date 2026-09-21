// The release-install sequence and the store repair (src/lib/install.ts,
// src/lib/heal.ts) — Gitea #643.
//
// Both are driven here against a fake device rather than through a browser,
// because what has to be right is the ORDER and the refusals: firmware
// before assets, a wrong-board package rejected before anything is streamed,
// a pattern that no longer compiles left exactly as it was. A browser run
// proves the buttons are wired; this proves the rules.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { boardMismatch, installRelease, readUpload, rebootSettled } from "../src/lib/install.ts";
import { healStaleStore, healSummary } from "../src/lib/heal.ts";
import { packLuxr } from "../src/lib/luxr.ts";

const BOARD = "Athom music-reactive WLED controller";
const APP = Uint8Array.from({ length: 64 }, (_, i) => i);
const ASSETS = Uint8Array.from({ length: 32 }, (_, i) => 255 - i);
const noSleep = () => Promise.resolve();

/** A device that accepts everything and comes back on the other slot. */
function fakeDevice(over = {}) {
  const log = [];
  let booted = false;
  return {
    log,
    async otaUpload(buf) {
      log.push(`ota:${buf.byteLength}`);
      booted = true;
      return { ok: true, bytes: buf.byteLength };
    },
    async assetsUpload(buf) {
      log.push(`assets:${buf.byteLength}`);
      return { ok: true, bytes: buf.byteLength };
    },
    async status() {
      log.push("status");
      return booted
        ? { version: "0.2.0", slot: "ota_1", board: BOARD }
        : { version: "0.1.0", slot: "ota_0", board: BOARD };
    },
    ...over,
  };
}

const BEFORE = { version: "0.1.0", slot: "ota_0" };

test("a package installs firmware first, then the assets", async () => {
  const dev = fakeDevice();
  const up = await readUpload(
    await packLuxr({ board: BOARD, version: "0.2.0", app: APP, assets: ASSETS }),
  );
  assert.equal(up.kind, "package");
  const steps = [];
  const r = await installRelease(dev, up, BEFORE, (p) => steps.push(p.step), noSleep);
  assert.equal(r.ok, true);
  assert.equal(r.assetsInstalled, true);
  assert.equal(r.version, "0.2.0");
  // the ORDER is the contract: the assets partition is served by the
  // firmware, so assets must never land in front of an older engine
  const order = dev.log.filter((l) => !l.startsWith("status"));
  assert.deepEqual(order, [`ota:${APP.length}`, `assets:${ASSETS.length}`]);
  assert.deepEqual(steps, ["firmware", "rebooting", "assets", "done"]);
});

test("a bare app image installs firmware only and says so", async () => {
  const dev = fakeDevice();
  const up = await readUpload(APP);
  assert.equal(up.kind, "app");
  const r = await installRelease(dev, up, BEFORE, () => {}, noSleep);
  assert.equal(r.ok, true);
  assert.equal(r.assetsInstalled, false);
  assert.ok(!dev.log.some((l) => l.startsWith("assets:")));
});

test("a refused image stops before the reboot wait", async () => {
  const dev = fakeDevice({
    async otaUpload() {
      return { ok: false, error: "image too large for the OTA slot" };
    },
  });
  const r = await installRelease(dev, await readUpload(APP), BEFORE, () => {}, noSleep);
  assert.equal(r.ok, false);
  assert.match(r.error, /too large/);
  assert.ok(!dev.log.includes("status"));
});

test("a device that never comes back is reported, not waited on forever", async () => {
  let t = 0;
  const dev = fakeDevice({
    async status() {
      throw new Error("connection refused");
    },
  });
  const r = await installRelease(
    dev,
    await readUpload(APP),
    BEFORE,
    () => {},
    async (ms) => {
      t += ms;
    },
    () => t,
  );
  assert.equal(r.ok, false);
  assert.match(r.error, /did not come back within 60 seconds/);
});

test("assets failing after a good firmware install is its own message", async () => {
  const dev = fakeDevice({
    async assetsUpload() {
      return { ok: false, error: "no assets partition" };
    },
  });
  const up = await readUpload(
    await packLuxr({ board: BOARD, version: "0.2.0", app: APP, assets: ASSETS }),
  );
  const r = await installRelease(dev, up, BEFORE, () => {}, noSleep);
  assert.equal(r.ok, false);
  assert.match(r.error, /firmware installed, but/);
  assert.equal(r.version, "0.2.0"); // the firmware half really did land
});

test("the reboot is settled by a changed slot OR a changed version", () => {
  assert.equal(rebootSettled({ slot: "ota_0" }, { slot: "ota_1" }), true);
  assert.equal(rebootSettled({ version: "0.1.0" }, { version: "0.1.0+ota1" }), true);
  assert.equal(rebootSettled({ version: "0.1.0", slot: "ota_0" }, { version: "0.1.0", slot: "ota_0" }), false);
  assert.equal(rebootSettled({}, {}), false);
});

test("a wrong-board package is refused; a substring board name is not", () => {
  assert.equal(boardMismatch(BOARD, BOARD), null);
  // board-target.sh's board_name is documented as a SUBSTRING of board::NAME
  assert.equal(boardMismatch("ESP32-S3 devkit", "ESP32-S3 devkit + HUB75 panel (untested)"), null);
  const bad = boardMismatch("Pixelblaze v3 Standard", BOARD);
  assert.ok(bad);
  assert.match(bad, /built for Pixelblaze v3 Standard/);
  // a device that cannot say what it is cannot be checked
  assert.equal(boardMismatch(BOARD, undefined), null);
});

// ---- the self-heal ----

/** A device whose store is full of blobs it can no longer read. */
function fakeStore(patterns, over = {}) {
  const saved = [];
  const activated = [];
  return {
    saved,
    activated,
    async patterns() {
      return patterns.map((p) => ({ id: p.id, name: p.name, ...(p.stale ? { stale: true } : {}) }));
    },
    async patternSource(id) {
      const p = patterns.find((x) => x.id === id);
      if (!p) throw new Error("no such pattern");
      return { id, name: p.name, source: p.source };
    },
    async savePattern(name, source, bc) {
      saved.push({ name, source, bytes: bc.length });
      const p = patterns.find((x) => x.name === name);
      if (p) p.stale = false;
      return { ok: true, id: p?.id };
    },
    async activatePattern(id) {
      activated.push(id);
      return { ok: true };
    },
    async playlist() {
      return { defaultSec: 10, crossfadeMs: 0, playing: false, index: 0, items: [] };
    },
    async status() {
      return { vmerr: null };
    },
    ...over,
  };
}

const compileOk = (src) => new Uint8Array([76, 88, 66, 67, 6, 0, src.length & 0xff]);

test("only the flagged patterns are recompiled, and by NAME", async () => {
  const dev = fakeStore([
    { id: "a1", name: "Aurora", source: "// a", stale: true },
    { id: "b2", name: "Fairies", source: "// b" },
    { id: "c3", name: "Rainbow", source: "// c", stale: true },
  ]);
  const r = await healStaleStore(dev, compileOk);
  assert.equal(r.found, 2);
  assert.equal(r.repaired, 2);
  assert.deepEqual(r.failed, []);
  // saving by name is what preserves the id and every playlist reference
  assert.deepEqual(dev.saved.map((s) => s.name).sort(), ["Aurora", "Rainbow"]);
});

test("nothing stale is a no-op, and the repair is idempotent", async () => {
  const dev = fakeStore([{ id: "a1", name: "Aurora", source: "// a", stale: true }]);
  assert.equal((await healStaleStore(dev, compileOk)).repaired, 1);
  const again = await healStaleStore(dev, compileOk);
  assert.equal(again.found, 0);
  assert.equal(again.repaired, 0);
  assert.equal(dev.saved.length, 1); // the second run wrote nothing
});

test("a pattern that no longer compiles is left alone and listed", async () => {
  const dev = fakeStore([
    { id: "a1", name: "Aurora", source: "// a", stale: true },
    { id: "b2", name: "Broken", source: "syntax(", stale: true },
  ]);
  const compile = (src) => (src === "syntax(" ? null : compileOk(src));
  const r = await healStaleStore(dev, compile);
  assert.equal(r.found, 2);
  assert.equal(r.repaired, 1);
  assert.deepEqual(r.failed, [
    { id: "b2", name: "Broken", why: "its source no longer compiles" },
  ]);
  assert.deepEqual(dev.saved.map((s) => s.name), ["Aurora"]);
  assert.match(healSummary(r), /recompiled 1 stored pattern .* Broken \(its source no longer compiles\)/);
});

test("a repaired running pattern is re-entered so the fixture lights again", async () => {
  const dev = fakeStore([{ id: "a1", name: "Aurora", source: "// a", stale: true }]);
  const r = await healStaleStore(dev, compileOk, { runningId: "a1" });
  assert.equal(r.reactivated, "a1");
  assert.deepEqual(dev.activated, ["a1"]);
});

test("a running pattern that could not be repaired is not re-entered", async () => {
  const dev = fakeStore([{ id: "a1", name: "Aurora", source: "// a", stale: true }]);
  const r = await healStaleStore(dev, () => null, { runningId: "a1" });
  assert.equal(r.reactivated, null);
  assert.deepEqual(dev.activated, []);
});

test("an interrupted run is resumable — the rest stay flagged", async () => {
  const dev = fakeStore([
    { id: "a1", name: "Aurora", source: "// a", stale: true },
    { id: "b2", name: "Fairies", source: "// b", stale: true },
  ]);
  let calls = 0;
  const flaky = {
    ...dev,
    async savePattern(name, source, bc) {
      if (++calls === 2) throw new Error("device stopped answering");
      return dev.savePattern(name, source, bc);
    },
  };
  const first = await healStaleStore(flaky, compileOk);
  assert.equal(first.repaired, 1);
  assert.equal(first.failed.length, 1);
  const second = await healStaleStore(dev, compileOk);
  assert.equal(second.found, 1); // exactly the one that did not land
  assert.equal(second.repaired, 1);
});

test("healSummary is empty when there was nothing to do", () => {
  assert.equal(healSummary({ found: 0, repaired: 0, failed: [], reactivated: null }), "");
});
