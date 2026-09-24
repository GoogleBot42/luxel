// The scene compositor's wasm C ABI (`lx_comp_*`, crates/luxel-wasm) — the
// exports `src/lib/luxel.ts`'s `Compositor` wrapper is built on.
//
// These run against the REAL wasm rather than the TS, because the whole point
// of Gitea #477 is that the playground's blend is `luxel_core::compose` and
// not a JS reimplementation: what is worth pinning here is that the ABI is
// reachable from JS and that the numbers coming back are the kernel's.
//
// Run: `npm test` from web/ (needs `npm run wasm` first; skipped without it).
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const WASM = fileURLToPath(new URL("../public/luxel.wasm", import.meta.url));
const RAW = 65536;

async function load() {
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
  const compile = (src, px) => {
    const s = putStr(src);
    const h = e.lx_new(s.ptr, s.len, px, 1);
    s.free();
    assert.ok(h >= 0, `compile failed: ${h < 0 ? response() : ""}`);
    return h;
  };
  const setScene = (ch, wire) => {
    const s = putStr(wire);
    const rc = e.lx_comp_set(ch, s.ptr, s.len);
    s.free();
    return rc === 0 ? null : response();
  };
  const frame = (ch, dt, n) => {
    const ptr = e.lx_comp_frame(ch, Math.round(dt * RAW));
    return new Uint8Array(e.memory.buffer.slice(ptr, ptr + n * 3));
  };
  return { e, putStr, response, compile, setScene, frame };
}

const SOLID = (hue) =>
  `export function render2D(index, x, y) { hsv(${hue}, 1, 1) }`;

test("a colour layer composites through the wasm", async () => {
  const w = await load();
  if (!w) return void console.log("skipped: web/public/luxel.wasm not built (or predates lx_comp_*)");
  const ch = w.e.lx_comp_new(2, 2);
  assert.ok(ch >= 0);
  assert.equal(
    w.setScene(ch, "S 0000000a t\nL color 0 0 0 0 normal 100 none fill 1\nK ff8800\n"),
    null,
  );
  assert.equal(w.e.lx_comp_layer_count(ch), 1);
  const px = w.frame(ch, 16, 4);
  assert.deepEqual([...px.slice(0, 3)], [0xff, 0x88, 0x00]);
  assert.equal(px.length, 12);
  w.e.lx_comp_free(ch);
});

test("a bad scene block returns the parse error, not a handle", async () => {
  const w = await load();
  if (!w) return;
  const ch = w.e.lx_comp_new(2, 2);
  assert.equal(
    w.setScene(ch, "S 0000000a t\nL color 0 0 0 0 nope 100 none fill 1\n"),
    'scene: line 2: unknown blend "nope"',
  );
  w.e.lx_comp_free(ch);
});

test("two bound pattern layers blend like the device crossfade", async () => {
  const w = await load();
  if (!w) return;
  const ch = w.e.lx_comp_new(2, 2);
  // red under, green over at 50 % — `blend_px(a, b, 32768)` per channel
  assert.equal(
    w.setScene(
      ch,
      "S 0000000a t\n" +
        "L pat 0 0 0 0 normal 100 none fill 1\nI 0123abcd\n" +
        "L pat 0 0 0 0 normal 50 none fill 1\nI 0123abce\n",
    ),
    null,
  );
  const under = w.compile(SOLID(0), 4);
  const over = w.compile(SOLID(0.3333), 4);
  for (const h of [under, over]) w.e.lx_set_map_grid(h, 2, 2);
  w.e.lx_comp_bind(ch, 0, under);
  w.e.lx_comp_bind(ch, 1, over);
  const px = w.frame(ch, 16, 4);
  // the base is pure red, the top is green: every channel is a lerp, so the
  // red channel must have fallen and the green risen from a pure-red frame
  assert.ok(px[0] > 100 && px[0] < 200, `red ${px[0]}`);
  assert.ok(px[1] > 100 && px[1] < 200, `green ${px[1]}`);
  // unbinding a layer leaves the one beneath it alone
  w.e.lx_comp_bind(ch, 1, -1);
  const solo = w.frame(ch, 16, 4);
  assert.equal(solo[0], 255);
  assert.equal(solo[1], 0);
  for (const h of [under, over]) w.e.lx_free(h);
  w.e.lx_comp_free(ch);
});

test("handles are reused after a free", async () => {
  const w = await load();
  if (!w) return;
  const a = w.e.lx_comp_new(1, 1);
  w.e.lx_comp_free(a);
  const b = w.e.lx_comp_new(1, 1);
  assert.equal(b, a);
  w.e.lx_comp_free(b);
  // a stale handle is inert, not a crash
  assert.equal(w.e.lx_comp_layer_count(a), 0);
  assert.equal(w.e.lx_comp_frame(a, 0), 0);
});
