// Node-side pattern compiler for the e2e drivers: loads the built
// web/public/luxel.wasm and turns pattern source into the LXP1 envelope
// the device API takes (devices execute LXBC bytecode; raw source is no
// longer accepted on the wire). Mirrors web/src/lib/device.ts lxpEnvelope.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const WASM = fileURLToPath(new URL("../public/luxel.wasm", import.meta.url));

let exportsCache = null;

async function wasmExports() {
  if (!exportsCache) {
    const { instance } = await WebAssembly.instantiate(readFileSync(WASM), {});
    exportsCache = instance.exports;
  }
  return exportsCache;
}

/** Compile source → LXBC bytecode (Uint8Array). Throws on compile errors. */
export async function compile(source, pixelCount = 120) {
  const e = await wasmExports();
  const bytes = new TextEncoder().encode(source);
  const ptr = e.lx_alloc(bytes.length);
  new Uint8Array(e.memory.buffer).set(bytes, ptr);
  const h = e.lx_new(ptr, bytes.length, pixelCount, 1);
  e.lx_dealloc(ptr, bytes.length);
  if (h < 0) {
    const msg = new TextDecoder().decode(
      new Uint8Array(e.memory.buffer, e.lx_response_ptr(), e.lx_response_len()),
    );
    throw new Error(`compile failed: ${msg}`);
  }
  const len = e.lx_bytecode(h);
  const bc = new Uint8Array(e.memory.buffer.slice(e.lx_bytecode_ptr(h), e.lx_bytecode_ptr(h) + len));
  e.lx_free(h);
  return bc;
}

/** LXP1 envelope: name + source + bytecode (see docs/spec/bytecode.md). */
export function envelope(name, source, bytecode) {
  const enc = new TextEncoder();
  const nameB = enc.encode(name).slice(0, 255);
  const srcB = enc.encode(source);
  const out = new Uint8Array(4 + 1 + nameB.length + 4 + srcB.length + 4 + bytecode.length);
  const view = new DataView(out.buffer);
  let at = 0;
  out.set(enc.encode("LXP1"), at);
  at += 4;
  out[at++] = nameB.length;
  out.set(nameB, at);
  at += nameB.length;
  view.setUint32(at, srcB.length, true);
  at += 4;
  out.set(srcB, at);
  at += srcB.length;
  view.setUint32(at, bytecode.length, true);
  at += 4;
  out.set(bytecode, at);
  return out;
}

/** Compile + wrap in one step: the POST body for /api/code and /api/patterns. */
export async function lxpBody(name, source, pixelCount = 120) {
  return envelope(name, source, await compile(source, pixelCount));
}
