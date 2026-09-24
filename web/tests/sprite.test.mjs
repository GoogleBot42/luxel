// `src/lib/sprite.ts` — the browser's codec for the sprite-tagged pattern
// format (Gitea #481, docs/spec/scenes.md §4).
//
// The interesting assertion is not that the codec round-trips its own
// strings: it is that what `emitSprite` writes is what
// `luxel_core::compose::sprite_view` reads. So the last test COMPILES the
// emitted source in the real wasm, binds it as a sprite layer of a real
// compositor, and checks the composite pixels are the texels that were
// painted. If the emitter and the core ever drift, that is where it shows.
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  SPRITE_MAX_COLORS,
  emitSprite,
  fillSprite,
  newSprite,
  paintSprite,
  paletteWouldOverflow,
  parseSprite,
  resizeSprite,
  spriteIndex,
  spriteLength,
  spritePalette,
} from "../src/lib/sprite.ts";
import { parseSpriteTag } from "../src/lib/scene.ts";

const WASM = fileURLToPath(new URL("../public/luxel.wasm", import.meta.url));
const RAW = 65536;

test("a new sprite is fully transparent and emits a parsable pattern", () => {
  const sp = newSprite(4, 3);
  assert.deepEqual(sp.tag, { w: 4, h: 3, frames: 1, fps: 0 });
  assert.equal(spriteLength(sp.tag), 12);
  assert.ok(sp.v.every((v) => v === 0));

  const src = emitSprite(sp);
  assert.equal(src.split("\n")[0], "// @sprite w=4 h=3 frames=1 fps=0");
  assert.deepEqual(parseSpriteTag(src), { w: 4, h: 3, frames: 1, fps: 0 });
  const back = parseSprite(src);
  assert.ok(back);
  assert.deepEqual(back.tag, sp.tag);
  assert.deepEqual(back.v, sp.v);
});

test("painting and erasing round-trip through the source", () => {
  let sp = newSprite(3, 2);
  sp = paintSprite(sp, 0, 1, 0, [0.5, 1, 1]);
  sp = paintSprite(sp, 0, 2, 1, [0, 0, 1]);
  const back = parseSprite(emitSprite(sp));
  assert.ok(back);
  assert.equal(back.h[spriteIndex(back.tag, 0, 1, 0)], 0.5);
  assert.equal(back.v[spriteIndex(back.tag, 0, 1, 0)], 1);
  assert.equal(back.v[spriteIndex(back.tag, 0, 2, 1)], 1);
  assert.equal(back.s[spriteIndex(back.tag, 0, 2, 1)], 0);
  assert.equal(back.v[spriteIndex(back.tag, 0, 0, 0)], 0);

  const erased = paintSprite(sp, 0, 1, 0, null);
  assert.equal(erased.v[spriteIndex(erased.tag, 0, 1, 0)], 0);
  // the original is untouched — records are replaced, never mutated
  assert.equal(sp.v[spriteIndex(sp.tag, 0, 1, 0)], 1);
});

test("fill floods one frame, four-connected, over the seed colour only", () => {
  let sp = newSprite(3, 3, 2);
  // a vertical wall down the middle of frame 0
  for (let row = 0; row < 3; row++) sp = paintSprite(sp, 0, 1, row, [0, 0, 1]);
  sp = fillSprite(sp, 0, 0, 0, [0.25, 1, 1]);
  // the left column filled …
  for (let row = 0; row < 3; row++) {
    assert.equal(sp.h[spriteIndex(sp.tag, 0, 0, row)], 0.25, `left row ${row}`);
    // … the wall did not …
    assert.equal(sp.h[spriteIndex(sp.tag, 0, 1, row)], 0, `wall row ${row}`);
    // … the right column is behind the wall …
    assert.equal(sp.v[spriteIndex(sp.tag, 0, 2, row)], 0, `right row ${row}`);
    // … and frame 1 never moved
    assert.equal(sp.v[spriteIndex(sp.tag, 1, 0, row)], 0, `frame 1 row ${row}`);
  }
});

test("the palette is distinct opaque colours, in first-appearance order", () => {
  let sp = newSprite(4, 1);
  sp = paintSprite(sp, 0, 0, 0, [0.5, 1, 1]);
  sp = paintSprite(sp, 0, 1, 0, [0, 1, 1]);
  sp = paintSprite(sp, 0, 2, 0, [0.5, 1, 1]); // a repeat takes no slot
  const pal = spritePalette(sp);
  assert.equal(pal.length, 2);
  assert.deepEqual(pal[0], [0.5, 1, 1]);
  assert.deepEqual(pal[1], [0, 1, 1]);
  // the transparent texel is not a colour
  assert.ok(!pal.some((p) => p[2] === 0));
});

test("the sixteenth colour fits and the seventeenth does not", () => {
  let sp = newSprite(20, 1);
  for (let i = 0; i < SPRITE_MAX_COLORS; i++) sp = paintSprite(sp, 0, i, 0, [i / 32, 1, 1]);
  assert.equal(spritePalette(sp).length, SPRITE_MAX_COLORS);
  assert.equal(paletteWouldOverflow(sp, [0 / 32, 1, 1]), false, "a colour already in the palette");
  assert.equal(paletteWouldOverflow(sp, [0.9, 1, 1]), true, "a new colour at 16");
  assert.equal(paletteWouldOverflow(sp, [0, 0, 0]), false, "the eraser is never a colour");
});

test("resizing keeps the texels that still fit", () => {
  let sp = newSprite(3, 3);
  sp = paintSprite(sp, 0, 0, 0, [0.5, 1, 1]);
  sp = paintSprite(sp, 0, 2, 2, [0.25, 1, 1]);
  const small = resizeSprite(sp, { w: 2, h: 2, frames: 1, fps: 0 });
  assert.equal(spriteLength(small.tag), 4);
  assert.equal(small.h[spriteIndex(small.tag, 0, 0, 0)], 0.5);
  const big = resizeSprite(small, { w: 4, h: 4, frames: 1, fps: 0 });
  assert.equal(big.h[spriteIndex(big.tag, 0, 0, 0)], 0.5);
  assert.equal(big.v[spriteIndex(big.tag, 0, 3, 3)], 0);
});

test("a source the editor cannot draw on reads as null", () => {
  assert.equal(parseSprite("export function renderFrame() {}"), null, "no tag");
  assert.equal(
    parseSprite("// @sprite w=2 h=2 frames=1 fps=0\nvar sprH = [0,0,0,0]\nvar sprS = [0,0,0,0]\n"),
    null,
    "a missing array",
  );
  assert.equal(
    parseSprite(
      "// @sprite w=2 h=2 frames=1 fps=0\nvar sprH = [0,0]\nvar sprS = [0,0,0,0]\nvar sprV = [0,0,0,0]\n",
    ),
    null,
    "a length the tag disagrees with",
  );
});

// ---- the one that matters: the emitter against the real core ----

async function wasm() {
  if (!existsSync(WASM)) return null;
  const { instance } = await WebAssembly.instantiate(readFileSync(WASM), {});
  const e = instance.exports;
  if (typeof e.lx_comp_new !== "function") return null;
  const putStr = (s) => {
    const b = new TextEncoder().encode(s);
    const ptr = e.lx_alloc(b.length);
    new Uint8Array(e.memory.buffer).set(b, ptr);
    return { ptr, len: b.length, free: () => e.lx_dealloc(ptr, b.length) };
  };
  const response = () =>
    new TextDecoder().decode(
      new Uint8Array(e.memory.buffer, e.lx_response_ptr(), e.lx_response_len()),
    );
  return { e, putStr, response };
}

test("what emitSprite writes is what the compositor blits", async () => {
  const w = await wasm();
  if (!w) return void console.log("skipped: web/public/luxel.wasm not built (or predates lx_comp_*)");

  // a 2x2 sprite: red top-left, blue bottom-right, the other two transparent
  let sp = newSprite(2, 2);
  sp = paintSprite(sp, 0, 0, 0, [0, 1, 1]); // hue 0, full sat/val = pure red
  sp = paintSprite(sp, 0, 1, 1, [2 / 3, 1, 1]); // hue 2/3 = pure blue
  const src = emitSprite(sp, "Test sprite");

  // the source compiles as an ordinary pattern …
  const s = w.putStr(src);
  const eh = w.e.lx_new(s.ptr, s.len, 4, 1);
  s.free();
  assert.ok(eh >= 0, `the emitted sprite must compile: ${eh < 0 ? w.response() : ""}`);

  // … and the compositor reads its texels out of the const pool without ever
  // stepping it (docs/spec/scenes.md §4)
  const ch = w.e.lx_comp_new(2, 2);
  assert.ok(ch >= 0);
  const wire = w.putStr("S 0000000b t\nL sprite 0 0 2 2 normal 100 none fill 1\nI 0000000c\n");
  const rc = w.e.lx_comp_set(ch, wire.ptr, wire.len);
  wire.free();
  assert.equal(rc, 0, w.response());
  w.e.lx_comp_bind(ch, 0, eh);

  const ptr = w.e.lx_comp_frame(ch, Math.round(16 * RAW));
  const px = new Uint8Array(w.e.memory.buffer.slice(ptr, ptr + 4 * 3));
  assert.deepEqual([...px.slice(0, 3)], [255, 0, 0], "top-left is the red texel");
  assert.deepEqual([...px.slice(3, 6)], [0, 0, 0], "top-right is transparent");
  assert.deepEqual([...px.slice(6, 9)], [0, 0, 0], "bottom-left is transparent");
  assert.deepEqual([...px.slice(9, 12)], [0, 0, 255], "bottom-right is the blue texel");

  w.e.lx_comp_free(ch);
  w.e.lx_free(eh);
});

test("a multi-frame sprite's own body compiles and plays alone", async () => {
  const w = await wasm();
  if (!w) return void console.log("skipped: web/public/luxel.wasm not built");
  let sp = newSprite(2, 2, 3, 8);
  sp = paintSprite(sp, 1, 0, 0, [0.3, 1, 1]);
  const src = emitSprite(sp);
  assert.equal(src.split("\n")[0], "// @sprite w=2 h=2 frames=3 fps=8");
  const s = w.putStr(src);
  const eh = w.e.lx_new(s.ptr, s.len, 4, 1);
  s.free();
  assert.ok(eh >= 0, `an animated sprite must compile: ${eh < 0 ? w.response() : ""}`);
  const back = parseSprite(src);
  assert.ok(back);
  assert.equal(back.h[spriteIndex(back.tag, 1, 0, 0)], 0.3);
  w.e.lx_free(eh);
});
