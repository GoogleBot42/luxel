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

e.lx_free(h);
e.lx_free(h2);
e.lx_free(h3);
e.lx_free(h4);
e.lx_free(h5);
console.log("wasm smoke: all golden assertions pass (native ↔ wasm bit-identical)");
