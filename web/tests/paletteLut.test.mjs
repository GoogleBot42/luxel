// The colour ramp's 256-entry table: `lx_palette_lut` (crates/luxel-wasm) vs
// the TS fallback `rampLut` (src/lib/gradient.ts) — all 768 bytes, every shape
// that hurts. Gitea #748 §3.
//
// The table is WIRE-VISIBLE: it is what the device turns each brightness into,
// so the ramp editor's bar and the LEDs have to agree exactly. Since #748 the
// bar asks the engine (`Luxel.paletteLut` → `outpipe::fill_palette_lut` +
// `outpipe::palette_remap_frame`), and `rampLut` is only the synchronous
// fallback for the first paint, before the wasm module is up. A fallback that
// nothing compares against is a second implementation waiting to drift — the
// pre-#734 CSS-gradient preview was exactly that — so this file is the join:
// if the two disagree, the RUST is right and `gradient.ts` gets fixed.
//
// What is being pinned on the Rust side, beyond agreement: the #787 edge rule
// (the ramp clamps at BOTH ends — above the last stop its colour continues
// rather than cutting to black, unlike `paint()`, whose black edge is Pixel
// Blaze's own and is preserved).
//
// Run: `npm test` from web/ (needs `npm run wasm` first; skipped without it).
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { rampLut, stopBytes } from "../src/lib/gradient.ts";

const WASM = fileURLToPath(new URL("../public/luxel.wasm", import.meta.url));

async function load() {
  if (!existsSync(WASM)) return null;
  const { instance } = await WebAssembly.instantiate(readFileSync(WASM), {});
  const e = instance.exports;
  if (typeof e.lx_palette_lut !== "function") return null;
  /** The export, called the way `Luxel.paletteLut` calls it: stop bytes in,
   *  a COPY of the 768-byte table out (the module reuses its buffer). */
  const lut = (bytes, pct) => {
    let ptr = 0;
    if (bytes.length > 0) {
      ptr = e.lx_alloc(bytes.length);
      new Uint8Array(e.memory.buffer).set(bytes, ptr);
    }
    const at = e.lx_palette_lut(ptr, Math.floor(bytes.length / 4), pct);
    assert.notEqual(at, 0, "lx_palette_lut returned null");
    const out = new Uint8Array(e.memory.buffer, at, 768).slice();
    if (ptr !== 0) e.lx_dealloc(ptr, bytes.length);
    return out;
  };
  return { e, lut };
}

const hex = (r, g, b) =>
  [r, g, b].map((v) => (v & 0xff).toString(16).padStart(2, "0")).join("");

const entry = (t, i) => [t[i * 3], t[i * 3 + 1], t[i * 3 + 2]];

/** All 768 bytes, with a first-difference message that names the entry. */
function same(got, want, label) {
  assert.equal(got.length, 768, `${label}: length`);
  assert.equal(want.length, 768, `${label}: fallback length`);
  for (let i = 0; i < 768; i++) {
    if (got[i] !== want[i]) {
      const n = Math.floor(i / 3);
      assert.fail(
        `${label}: entry ${n} channel ${i % 3} — rust ${entry(got, n)} vs ts ${entry(want, n)}`,
      );
    }
  }
}

/** The comparison this whole file exists for: same stops, same amount, and
 *  the two implementations must produce the identical table. */
function agree(w, stops, pct, label) {
  const bytes = stopBytes(stops);
  same(w.lut(bytes, pct), rampLut(stops, pct), `${label} @ ${pct}%`);
}

const AMOUNTS = [100, 50, 33, 1, 0];

// ---------------------------------------------------------------- the shapes

test("four stops spanning 0..255 agree at every amount", async () => {
  const w = await load();
  if (!w) return;
  const stops = [
    { pos: 0, hex: "000000" },
    { pos: 87, hex: "2a1060" },
    { pos: 173, hex: "c23a6b" },
    { pos: 255, hex: "f7e08a" },
  ];
  for (const pct of AMOUNTS) agree(w, stops, pct, "0..255 span");
});

test("stops strictly inside 0..255 agree — the #787 clamp at both ends", async () => {
  const w = await load();
  if (!w) return;
  const stops = [
    { pos: 40, hex: "1b0e3a" },
    { pos: 96, hex: "c23a6b" },
    { pos: 176, hex: "f7e08a" },
  ];
  for (const pct of AMOUNTS) agree(w, stops, pct, "inside 0..255");
});

test("a zero-width span agrees (two stops at one position)", async () => {
  const w = await load();
  if (!w) return;
  for (const p of [0, 128, 255]) {
    const stops = [
      { pos: 0, hex: "102030" },
      { pos: p, hex: "ff0000" },
      { pos: p, hex: "00ff00" },
      { pos: 255, hex: "ffffff" },
    ];
    for (const pct of AMOUNTS) agree(w, stops, pct, `zero-width at ${p}`);
  }
  // and the degenerate list: nothing but a zero-width pair
  for (const pct of AMOUNTS) {
    agree(
      w,
      [
        { pos: 128, hex: "ff0000" },
        { pos: 128, hex: "00ff00" },
      ],
      pct,
      "zero-width alone",
    );
  }
});

test("adjacent positions agree (127 and 128)", async () => {
  const w = await load();
  if (!w) return;
  const stops = [
    { pos: 0, hex: "000000" },
    { pos: 127, hex: "00ff80" },
    { pos: 128, hex: "ff0080" },
    { pos: 255, hex: "ffffff" },
  ];
  for (const pct of AMOUNTS) agree(w, stops, pct, "adjacent 127/128");
});

test("a single stop agrees wherever it sits", async () => {
  const w = await load();
  if (!w) return;
  for (const pos of [0, 128, 255]) {
    for (const pct of AMOUNTS) agree(w, [{ pos, hex: "1234ab" }], pct, `single stop at ${pos}`);
  }
});

test("an empty stop list is the identity table on both sides", async () => {
  const w = await load();
  if (!w) return;
  for (const pct of AMOUNTS) agree(w, [], pct, "empty");
  // and it really is the identity: no stop to clamp to, so `sample_palette`
  // returns [v,v,v] and the table is grey (0 and 255 land exactly)
  const t = w.lut(stopBytes([]), 100);
  assert.deepEqual(entry(t, 0), [0, 0, 0]);
  assert.deepEqual(entry(t, 255), [255, 255, 255]);
  for (let i = 0; i < 256; i++) {
    const [r, g, b] = entry(t, i);
    assert.equal(r, g, `entry ${i} is grey`);
    assert.equal(g, b, `entry ${i} is grey`);
  }
});

test("falling channels agree — the floor on a negative delta", async () => {
  const w = await load();
  if (!w) return;
  const stops = [
    { pos: 0, hex: "ffffff" },
    { pos: 255, hex: "000000" },
  ];
  for (const pct of AMOUNTS) agree(w, stops, pct, "ffffff → 000000");
  // a falling ramp with the last stop inside the range, so the clamp and the
  // negative delta are exercised together
  const inside = [
    { pos: 17, hex: "ffffff" },
    { pos: 200, hex: "000000" },
  ];
  for (const pct of AMOUNTS) agree(w, inside, pct, "falling, inside");
});

test("32 stops — the cap — agree", async () => {
  const w = await load();
  if (!w) return;
  const stops = Array.from({ length: 32 }, (_, i) => ({
    pos: Math.round((i * 255) / 31),
    hex: hex(i * 8, 255 - i * 8, (i * 37) % 256),
  }));
  assert.equal(stopBytes(stops).length, 128);
  for (const pct of AMOUNTS) agree(w, stops, pct, "32 stops");
});

// ---------------------------------------------------------------- the fuzz

/** Count reported by the fuzz test, so the run says how much was compared. */
const FUZZ_SHAPES = 50;

test(`${FUZZ_SHAPES} pseudo-random shapes agree byte for byte`, async () => {
  const w = await load();
  if (!w) return;
  // A tiny LCG written here rather than pulled in, so a failure is
  // reproducible from the seed alone (numerical Recipes' constants).
  let s = 0x5eed_1c92 >>> 0;
  const next = () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s;
  };
  const pick = (n) => next() % n;

  // Positions come from a deliberately SMALL space on some shapes — 1 included,
  // i.e. EVERY stop on top of every other. With 0..255 to draw from, a list
  // whose FIRST and LAST stop share a position never comes up, and that is
  // exactly where the engine's position clamp and a naive "return the last
  // colour" part company (`sample_palette` checks `v <= first` before
  // `v == last`). The first version of this fuzz sailed straight past the real
  // divergence in `rampLut` that the zero-width case above caught.
  const SPACES = [256, 256, 64, 8, 3, 1];

  let compared = 0;
  for (let k = 0; k < FUZZ_SHAPES; k++) {
    const n = pick(33); // 0..32 inclusive — the empty list and the cap included
    const space = SPACES[pick(SPACES.length)];
    const stops = Array.from({ length: n }, () => ({
      pos: pick(space),
      hex: hex(pick(256), pick(256), pick(256)),
    }));
    const pct = pick(101);
    agree(w, stops, pct, `fuzz ${k} (${n} stops over 0..${space - 1})`);
    compared++;
  }
  assert.equal(compared, FUZZ_SHAPES, "every shape was compared");
});

// ------------------------------------------------- the semantic, not just the
// ------------------------------------------------- agreement

test("the ramp continues its last stop above it, and never goes black there", async () => {
  const w = await load();
  if (!w) return;
  const stops = [
    { pos: 0, hex: "1b0e3a" },
    { pos: 96, hex: "c23a6b" },
    { pos: 176, hex: "f7e08a" },
  ];
  const t = w.lut(stopBytes(stops), 100);
  const top = entry(t, 176);
  assert.notDeepEqual(top, [0, 0, 0], "the last stop's colour, not black");
  // f7e08a round-tripped through 16.16 and floor(v·255) — within a bit
  for (const [i, want] of [[0, 0xf7], [1, 0xe0], [2, 0x8a]]) {
    assert.ok(Math.abs(top[i] - want) <= 1, `channel ${i}: ${top[i]} vs ${want}`);
  }
  for (let i = 176; i <= 255; i++) {
    assert.deepEqual(entry(t, i), top, `entry ${i} continues the last stop`);
    assert.notDeepEqual(entry(t, i), [0, 0, 0], `entry ${i} is not black`);
  }
  // below the first stop clamps too, and inside the ramp is still a ramp
  assert.notDeepEqual(entry(t, 96), top, "entry 96 is the middle stop");

  // the pre-#787 behaviour is still reachable — by SAYING it, with a black
  // last stop (`RAMP_PRESETS`' "To black")
  const toBlack = w.lut(
    stopBytes([
      { pos: 0, hex: "f7e08a" },
      { pos: 255, hex: "000000" },
    ]),
    100,
  );
  assert.deepEqual(entry(toBlack, 255), [0, 0, 0]);
});
