// Smoke-test the wasm build in Node: instantiate the raw module, drive the
// C ABI directly, and assert the same golden pixels the native test suite
// locks — cross-host determinism, proven through the FFI.
//
// Usage: cargo build --release --target wasm32-unknown-unknown -p luxel-wasm
//        node tools/wasm-smoke.mjs

import assert from "node:assert";
import fs from "node:fs";

const wasmBytes = fs.readFileSync("target/wasm32-unknown-unknown/release/luxel_wasm.wasm");
const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const e = instance.exports;
const mem = () => new Uint8Array(e.memory.buffer);

function putStr(s) {
  const bytes = new TextEncoder().encode(s);
  const ptr = e.lx_alloc(bytes.length);
  mem().set(bytes, ptr);
  return { ptr, len: bytes.length, free: () => e.lx_dealloc(ptr, bytes.length) };
}

function response() {
  const ptr = e.lx_response_ptr();
  const len = e.lx_response_len();
  return new TextDecoder().decode(mem().slice(ptr, ptr + len));
}

const RAINBOW = "export function render(index) {\n  hsv(time(.1) + index / pixelCount, 1, 1)\n}";
const src = putStr(RAINBOW);
const h = e.lx_new(src.ptr, src.len, 4, 1);
src.free();
assert.ok(h >= 0, `compile failed: ${response()}`);

const px = e.lx_frame(h, 0);
const rgb = [...mem().slice(px, px + 12)];
// must match the native golden test bytes exactly
// (crates/luxel-core/tests/engine.rs::rainbow_golden_frame — the 127s are
// the floor-quantized, PB-exact values, not a rounding slip)
assert.deepStrictEqual(rgb, [255, 0, 0, 127, 255, 0, 0, 255, 255, 127, 0, 255]);
assert.strictEqual(e.lx_take_error(h), 0);

// controls + vars round-trip
const src2 = putStr(
  "export var speed = 0.5\nexport function sliderSpeed(v) { speed = v }\nexport function render(i) { hsv(0, 0, speed) }",
);
const h2 = e.lx_new(src2.ptr, src2.len, 2, 1);
src2.free();
assert.ok(h2 >= 0, response());
e.lx_controls(h2);
assert.deepStrictEqual(JSON.parse(response()), [
  { kind: "slider", label: "Speed", name: "sliderSpeed" },
]);
const name = putStr("sliderSpeed");
e.lx_set_control(h2, name.ptr, name.len, 0.75 * 65536, 0, 0, 1);
name.free();
e.lx_vars(h2);
assert.strictEqual(JSON.parse(response()).speed, 0.75 * 65536);

// compile errors report line/col
const bad = putStr("out = nonsense(");
const hBad = e.lx_new(bad.ptr, bad.len, 4, 1);
bad.free();
assert.strictEqual(hBad, -1);
const diag = JSON.parse(response());
assert.ok(diag.message.length > 0 && diag.line === 1, JSON.stringify(diag));

// 2D map + transforms work through the FFI
const src3 = putStr(
  "export function beforeRender(delta) { resetTransform()\n translate(-0.5, -0.5) }\n" +
    "export function render2D(index, x, y) { rgb(clamp(x + 0.5, 0, 1), clamp(y + 0.5, 0, 1), 0) }",
);
const h3 = e.lx_new(src3.ptr, src3.len, 4, 1);
src3.free();
assert.ok(h3 >= 0, response());
e.lx_set_map_grid(h3, 2, 2);
const px3 = e.lx_frame(h3, 0);
const rgb3 = [...mem().slice(px3, px3 + 12)];
// grid world coords max out at ≈0.99998, so the floored quantization gives
// 254, not 255 — same golden as crates/luxel-core/tests/semantics.rs::map_and_introspection
assert.deepStrictEqual(rgb3, [0, 0, 0, 254, 0, 0, 0, 254, 0, 254, 254, 0]);

// pin injection through the FFI (Gitea #177): the pattern polls a pulled-up
// input every frame, so the strip is dark at idle and lit while the host
// drives the pin LOW. Mirrors
// crates/luxel-core/tests/semantics.rs::set_pin_drives_digital_read.
const src4 = putStr(
  "pinMode(4, INPUT_PULLUP)\nexport function render(index) { hsv(0, 0, digitalRead(4) == LOW) }",
);
const h4 = e.lx_new(src4.ptr, src4.len, 1, 1);
src4.free();
assert.ok(h4 >= 0, response());
const pin = () => {
  const at = e.lx_frame(h4, 0);
  return [...mem().slice(at, at + 3)];
};
assert.deepStrictEqual(pin(), [0, 0, 0], "pulled-up pin idles HIGH: not pressed");
assert.strictEqual(e.lx_pin_read(h4, 4), 1, "idle level readable");
assert.strictEqual(e.lx_set_pin(h4, 4, 0), 1); // drive LOW
assert.strictEqual(e.lx_pin_read(h4, 4), 0);
assert.deepStrictEqual(pin(), [255, 255, 255], "driven LOW: pressed, and HELD");
assert.deepStrictEqual(pin(), [255, 255, 255], "still held on the next frame");
assert.strictEqual(e.lx_set_pin(h4, 4, -1), 1); // release
assert.deepStrictEqual(pin(), [0, 0, 0], "released: back to the idle level");
// out-of-window pins are rejected, not silently aliased onto a tracked one
assert.strictEqual(e.lx_set_pin(h4, 64, 0), 0);
assert.strictEqual(e.lx_set_pin(h4, -1, 0), 0);

// analog pin injection through the FFI (Gitea #206): a pot on pin 33 sets the
// strip's brightness, dark undriven. Mirrors
// crates/luxel-core/tests/semantics.rs::set_analog_pin_drives_analog_read.
const src5 = putStr(
  "export function render(index) { hsv(0, 0, analogRead(33)) }",
);
const h5 = e.lx_new(src5.ptr, src5.len, 1, 1);
src5.free();
assert.ok(h5 >= 0, response());
const pot = () => {
  const at = e.lx_frame(h5, 0);
  return [...mem().slice(at, at + 3)];
};
assert.deepStrictEqual(pot(), [0, 0, 0], "undriven analog pin reads 0");
assert.strictEqual(e.lx_analog_pins_used(h5, 1), 1 << 1, "pin 33 is in the used mask");
assert.strictEqual(e.lx_set_analog_pin(h5, 33, 65536), 1); // 1.0
assert.strictEqual(e.lx_analog_read(h5, 33), 65536, "1.0 survives the u16 table");
assert.deepStrictEqual(pot(), [255, 255, 255], "driven full scale, and HELD");
assert.deepStrictEqual(pot(), [255, 255, 255], "still held on the next frame");
assert.strictEqual(e.lx_set_analog_pin(h5, 33, 32768), 1); // 0.5
assert.strictEqual(e.lx_analog_read(h5, 33), 32768);
assert.deepStrictEqual(pot(), [127, 127, 127], "half scale");
assert.strictEqual(e.lx_set_analog_pin(h5, 33, 0), 1); // release
assert.deepStrictEqual(pot(), [0, 0, 0], "released: back to 0");
// clamped to 0..1, and out-of-window pins rejected like the digital surface
assert.strictEqual(e.lx_set_analog_pin(h5, 33, 9 * 65536), 1);
assert.strictEqual(e.lx_analog_read(h5, 33), 65536, "values above 1 clamp");
assert.strictEqual(e.lx_set_analog_pin(h5, 64, 65536), 0);
assert.strictEqual(e.lx_set_analog_pin(h5, -1, 65536), 0);

// projection through the FFI (Gitea #473): the engine's §5.4d table, the
// along-axis strip render, and the effective geometry a UI captions from.
// Mirrors crates/luxel-core/tests/projection.rs.
assert.deepStrictEqual(
  JSON.parse((e.lx_projection_options(1, 2), response())).map((o) => o.mode),
  ["index", "x", "y"],
  "1D pattern on a 2D Layout",
);
assert.strictEqual(e.lx_projection_options(2, 2), 0, "a native pair offers nothing");
assert.deepStrictEqual(
  JSON.parse((e.lx_projection_options(3, 2), response())).map((o) => o.label),
  ["Slice xy", "Slice xz", "Slice yz"],
  "3D pattern on a 2D Layout, labelled by the engine",
);

const src6 = putStr("export function render(index) { rgb(index / 4, pixelCount / 8, 0) }");
const h6 = e.lx_new(src6.ptr, src6.len, 8, 1);
src6.free();
assert.ok(h6 >= 0, response());
e.lx_set_map_grid(h6, 4, 2);
assert.strictEqual(e.lx_layout_dims(h6), 2);
// default triple: by index, every Layout pixel rendered
assert.strictEqual(e.lx_effective_geometry(h6), 1);
assert.deepStrictEqual(JSON.parse(response()), {
  pixelCount: 8,
  patternDims: 1,
  layoutDims: 2,
  w: 4,
  h: 2,
  mode: "index",
  label: "By index",
});
// along x: ONE strip of 4 renders, replicated over both rows
assert.strictEqual(e.lx_set_projection(h6, 1, 3, 4), 1); // x, z, xy
assert.strictEqual(e.lx_projection(h6), 1 | (3 << 8) | (4 << 16));
assert.strictEqual(e.lx_effective_geometry(h6), 1);
const geom = JSON.parse(response());
assert.strictEqual(geom.pixelCount, 4, "pixelCount reads as the strip length");
assert.strictEqual(geom.label, "Along x");
const projected = e.lx_frame(h6, 0);
const rows = [...mem().slice(projected, projected + 24)];
assert.deepStrictEqual(rows.slice(0, 12), rows.slice(12), "the row is replicated");
assert.deepStrictEqual(
  rows.slice(0, 12),
  [0, 127, 0, 63, 127, 0, 127, 127, 0, 191, 127, 0],
  "index/4 across the row, pixelCount = 4",
);

// ---- the device output chain (lx_outpipe, Gitea #466) ----
// The playground now runs the SAME chain the firmware does, so a console
// preview can show what the wire carries instead of the raw engine frame.
// Settings are packed exactly as `GET /api/output` + `GET /api/brightness`
// report them; see crates/luxel-wasm/src/lib.rs::lx_outpipe_set.
const src7 = putStr("export function render(index) { rgb(1, 0.5, 0) }");
const h7 = e.lx_new(src7.ptr, src7.len, 8, 1);
src7.free();
assert.ok(h7 >= 0, response());

function setOutpipe(h, head, palette = []) {
  const stops = Math.floor(palette.length / 4);
  const vals = head.concat([stops], palette.slice(0, stops * 4));
  const bytes = vals.length * 4;
  const ptr = e.lx_alloc(bytes);
  const view = new DataView(e.memory.buffer);
  vals.forEach((v, i) => view.setInt32(ptr + i * 4, v, true));
  const ok = e.lx_outpipe_set(h, ptr, vals.length);
  e.lx_dealloc(ptr, bytes);
  return ok;
}
// [order, gamma_t, cap_ma, blur%, glow%, palette%, brightness, curve_t, model, scan]
const OFF = [0, 0, 0, 0, 0, 0, 31, 0, 0, 32];
const wire = (h) => {
  const at = e.lx_outpipe(h);
  return [...mem().slice(at, at + 24)];
};

const frame6 = [...mem().slice(e.lx_frame(h7, 0), e.lx_frame(h7, 0) + 24)];
assert.deepStrictEqual(frame6.slice(0, 3), [255, 127, 0], "rgb(1, .5, 0)");

assert.strictEqual(setOutpipe(h7, OFF), 1);
assert.deepStrictEqual(wire(h7), frame6, "every stage off: the engine frame verbatim");
assert.strictEqual(e.lx_outpipe_bytes(h7), 0, "an all-off chain holds no memory");

// colour order: grb (code 2) puts G,R,B on the wire
assert.strictEqual(setOutpipe(h7, [2, 0, 0, 0, 0, 0, 31, 0, 0, 32]), 1);
assert.deepStrictEqual(wire(h7).slice(0, 3), [127, 255, 0], "grb permutes the channels");
assert.strictEqual(e.lx_outpipe_bytes(h7), 8 * 3, "3 B/px of scratch while a stage is on");

// gamma 2.2 darkens the midtone and leaves the endpoints alone
assert.strictEqual(setOutpipe(h7, [0, 22, 0, 0, 0, 0, 31, 0, 0, 32]), 1);
const g = wire(h7).slice(0, 3);
assert.strictEqual(g[0], 255, "gamma leaves full scale alone");
assert.ok(g[1] > 0 && g[1] < 127, `gamma darkens the midtone: ${g[1]}`);
assert.strictEqual(g[2], 0);

// a power cap below the estimate scales the whole frame down uniformly.
// 8 px of rgb(1,.5,0) at brightness 31 estimate ~239 mA on the strip model.
assert.strictEqual(setOutpipe(h7, [0, 0, 120, 0, 0, 0, 31, 0, 0, 32]), 1);
const capped = wire(h7);
assert.ok(capped[0] < 255 && capped[0] > 0, `power cap scales: ${capped[0]}`);
assert.ok(capped.every((v, i) => v <= frame6[i]), "the cap only ever darkens");

// back to all-off: the chain returns its scratch (Gitea #446/#476) and the
// frame is the engine's again
assert.strictEqual(setOutpipe(h7, OFF), 1);
assert.deepStrictEqual(wire(h7), frame6, "all stages off again");
assert.strictEqual(e.lx_outpipe_bytes(h7), 0, "scratch released");

// a short buffer is rejected rather than read past
const tiny = e.lx_alloc(8);
assert.strictEqual(e.lx_outpipe_set(h7, tiny, 2), 0, "a short settings buffer is refused");
e.lx_dealloc(tiny, 8);
assert.strictEqual(e.lx_outpipe(-1), 0, "a bad handle returns null");

e.lx_free(h);
e.lx_free(h2);
e.lx_free(h3);
e.lx_free(h4);
e.lx_free(h5);
e.lx_free(h6);
e.lx_free(h7);
console.log("wasm smoke: all golden assertions pass (native ↔ wasm bit-identical)");
