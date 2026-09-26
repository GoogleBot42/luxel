// Unit tests for the `.luxr` release-package codec (src/lib/luxr.ts).
//
// The container carries the bytes that get written into a device's OTA slot,
// so the failure this has to catch is a truncated or mangled download — not
// a parse that "mostly worked". Every test below is therefore about the
// boundary: does a round trip come back byte-identical, and does a damaged
// package refuse to parse rather than hand out half an image.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import {
  isLuxr,
  LuxrError,
  LUXR_FORMAT,
  LUXR_MAGIC,
  packLuxr,
  parseLuxr,
  sha256Js,
} from "../src/lib/luxr.ts";

const BOARD = "Athom music-reactive WLED controller";
const APP = Uint8Array.from({ length: 1024 }, (_, i) => (i * 7) & 0xff);
const ASSETS = Uint8Array.from({ length: 333 }, (_, i) => (i * 13 + 5) & 0xff);

async function pkg(over = {}) {
  return packLuxr({ board: BOARD, version: "0.1.46", app: APP, assets: ASSETS, ...over });
}

test("a package round-trips byte for byte", async () => {
  const p = await parseLuxr(await pkg());
  assert.equal(p.board, BOARD);
  assert.equal(p.version, "0.1.46");
  assert.deepEqual(Array.from(p.app), Array.from(APP));
  assert.deepEqual(Array.from(p.assets), Array.from(ASSETS));
});

test("the header is the documented shape", async () => {
  const buf = await pkg();
  const dv = new DataView(buf.buffer);
  assert.equal(new TextDecoder().decode(buf.subarray(0, 4)), LUXR_MAGIC);
  assert.equal(dv.getUint16(4, true), LUXR_FORMAT);
  assert.equal(buf[6], new TextEncoder().encode(BOARD).length);
  assert.equal(buf[7], "0.1.46".length);
  assert.equal(dv.getUint32(8, true), APP.length);
  assert.equal(dv.getUint32(12, true), ASSETS.length);
  // fixed header + both strings + both payloads, nothing else
  assert.equal(buf.length, 80 + BOARD.length + 6 + APP.length + ASSETS.length);
});

test("isLuxr tells a package from a bare app image", async () => {
  assert.equal(isLuxr(await pkg()), true);
  assert.equal(isLuxr(APP), false);
  assert.equal(isLuxr(new Uint8Array(2)), false);
});

test("a firmware-only package parses with empty assets", async () => {
  const p = await parseLuxr(await pkg({ assets: new Uint8Array(0) }));
  assert.equal(p.assets.length, 0);
  assert.deepEqual(Array.from(p.app), Array.from(APP));
});

test("a non-ASCII board name survives the length byte (bytes, not chars)", async () => {
  const board = "Seengreat RGB Matrix HUB75 S3 — 64×64";
  const p = await parseLuxr(await pkg({ board }));
  assert.equal(p.board, board);
});

test("a bare app image is refused, not misread", async () => {
  await assert.rejects(() => parseLuxr(APP), (e) => e instanceof LuxrError && /LUXR/.test(e.message));
});

test("a truncated package is refused", async () => {
  const buf = await pkg();
  await assert.rejects(
    () => parseLuxr(buf.subarray(0, buf.length - 16)),
    (e) => e instanceof LuxrError && /truncated or corrupt/.test(e.message),
  );
});

test("a corrupted app image fails its checksum", async () => {
  const buf = await pkg();
  buf[80 + BOARD.length + 6 + 10] ^= 0xff; // one bit inside the app payload
  await assert.rejects(
    () => parseLuxr(buf),
    (e) => e instanceof LuxrError && /firmware image .* failed its checksum/.test(e.message),
  );
});

test("corrupted assets fail their checksum", async () => {
  const buf = await pkg();
  buf[buf.length - 3] ^= 0xff;
  await assert.rejects(
    () => parseLuxr(buf),
    (e) => e instanceof LuxrError && /web assets .* failed their checksum/.test(e.message),
  );
});

test("a future container format is named, not guessed at", async () => {
  const buf = await pkg();
  new DataView(buf.buffer).setUint16(4, LUXR_FORMAT + 1, true);
  await assert.rejects(
    () => parseLuxr(buf),
    (e) => e instanceof LuxrError && /package format v2/.test(e.message),
  );
});

test("packing refuses a board name that would not fit its length byte", async () => {
  await assert.rejects(() => pkg({ board: "x".repeat(256) }), LuxrError);
  await assert.rejects(() => pkg({ board: "" }), LuxrError);
  await assert.rejects(() => pkg({ app: new Uint8Array(0) }), LuxrError);
});

// ── the insecure-origin path (Gitea #794) ────────────────────────────────
// `crypto.subtle` exists only in a SECURE CONTEXT, and a device's own
// console is served from plain `http://<lan-ip>/`. Every harness here runs
// on 127.0.0.1, which IS one, so the codec's WebCrypto call passed
// everywhere and still died the first time a real Athom met it. These pin
// the fallback: same digest, and a full round trip with `crypto.subtle`
// taken away.

test("the in-house digest agrees with WebCrypto byte for byte", async () => {
  const cases = [
    new Uint8Array(0),
    Uint8Array.from([0x61]),
    new TextEncoder().encode("abc"),
    // the two lengths either side of a padding block boundary
    new Uint8Array(55).fill(0x5a),
    new Uint8Array(56).fill(0x5a),
    new Uint8Array(64).fill(0x5a),
    APP,
    ASSETS,
    Uint8Array.from({ length: 200_000 }, (_, i) => (i * 31 + 7) & 0xff),
  ];
  for (const c of cases) {
    const want = new Uint8Array(await crypto.subtle.digest("SHA-256", c.slice().buffer));
    assert.deepEqual(Array.from(sha256Js(c)), Array.from(want), `length ${c.length}`);
  }
});

test("a package round-trips on an origin with no crypto.subtle", async () => {
  const real = globalThis.crypto;
  // what a plain-http browser origin looks like: crypto is there, subtle is not
  Object.defineProperty(globalThis, "crypto", {
    value: { getRandomValues: real.getRandomValues.bind(real) },
    configurable: true,
  });
  try {
    const p = await parseLuxr(await pkg());
    assert.equal(p.board, BOARD);
    assert.deepEqual(Array.from(p.app), Array.from(APP));
    assert.deepEqual(Array.from(p.assets), Array.from(ASSETS));
    // and it still catches damage, which is the whole point of the hashes
    const bad = await pkg();
    bad[80 + BOARD.length + 6 + 10] ^= 0xff;
    await assert.rejects(
      () => parseLuxr(bad),
      (e) => e instanceof LuxrError && /failed its checksum/.test(e.message),
    );
  } finally {
    Object.defineProperty(globalThis, "crypto", { value: real, configurable: true });
  }
});
