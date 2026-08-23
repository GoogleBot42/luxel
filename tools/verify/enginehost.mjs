// Node host for the Luxel engine wasm — the headless twin of
// web/src/lib/luxel.ts, minus every browser dependency. Drives the same C ABI
// (`crates/luxel-wasm/src/lib.rs`) so a render produced here is bit-identical
// to one produced in the playground.
//
// Everything numeric crosses the ABI as raw 16.16 fixed point (raw = v·65536).
//
// Usage: cargo build --release --target wasm32-unknown-unknown -p luxel-wasm
//        import { load } from "./enginehost.mjs";
//        const lx = await load();
//        const eng = lx.compile(source, 60, 1);   // → Engine | {compileError}
//        eng.setWallClock(1756000000);
//        const rgb = eng.frame(50);               // Uint8Array(pixelCount*3)
//        eng.free();

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const RAW = 65536;
const I32_MIN = -2147483648;

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
export const WASM_PATH = path.join(
  REPO_ROOT,
  "target/wasm32-unknown-unknown/release/luxel_wasm.wasm",
);

/** Instantiate the engine wasm. No imports — the module is self-contained. */
export async function load(wasmPath = WASM_PATH) {
  if (!fs.existsSync(wasmPath)) {
    throw new Error(
      `engine wasm missing at ${wasmPath}\n` +
        `build it: nix develop -c cargo build --release --target wasm32-unknown-unknown -p luxel-wasm`,
    );
  }
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
  return new Host(instance.exports);
}

class Host {
  constructor(exports) {
    this.e = exports;
  }

  /** Live view — refetched every time, the buffer moves when wasm grows. */
  get mem() {
    return new Uint8Array(this.e.memory.buffer);
  }

  putStr(str) {
    const bytes = Buffer.from(str, "utf8");
    const ptr = this.e.lx_alloc(bytes.length);
    this.mem.set(bytes, ptr);
    return { ptr, len: bytes.length, free: () => this.e.lx_dealloc(ptr, bytes.length) };
  }

  response() {
    const ptr = this.e.lx_response_ptr();
    const len = this.e.lx_response_len();
    return Buffer.from(this.e.memory.buffer, ptr, len).toString("utf8");
  }

  /** Compile a pattern. Returns an Engine, or `{ compileError, diagnostic }`. */
  compile(source, pixelCount, seed = 1) {
    const s = this.putStr(source);
    let h;
    try {
      h = this.e.lx_new(s.ptr, s.len, pixelCount, seed);
    } finally {
      s.free();
    }
    if (h < 0) {
      let diagnostic = null;
      try {
        diagnostic = JSON.parse(this.response());
      } catch {
        diagnostic = { message: this.response() };
      }
      return {
        compileError: String(diagnostic?.message ?? "compile failed"),
        diagnostic,
      };
    }
    return new Engine(this, h, pixelCount);
  }
}

export class Engine {
  constructor(host, h, pixelCount) {
    this.host = host;
    this.e = host.e;
    this.h = h;
    this.pixelCount = pixelCount;
    this.freed = false;
  }

  /** Render one frame; returns a fresh copy of pixelCount*3 RGB bytes. */
  frame(deltaMs) {
    const ptr = this.e.lx_frame(this.h, Math.round(deltaMs * RAW));
    return new Uint8Array(this.e.memory.buffer.slice(ptr, ptr + this.pixelCount * 3));
  }

  /** Pop the pending runtime error, if any: {message, fn, pc, line, col}. */
  takeError() {
    if (this.e.lx_take_error(this.h) === 0) return null;
    try {
      return JSON.parse(this.host.response());
    } catch {
      return { message: this.host.response() };
    }
  }

  /** Declared UI controls: [{kind, label, name}]. */
  controls() {
    this.e.lx_controls(this.h);
    try {
      return JSON.parse(this.host.response());
    } catch {
      return [];
    }
  }

  /** Invoke a control by export name. Returns the shown value, or null if
   *  the pattern has no such control. */
  setControl(name, values) {
    const vals = Array.isArray(values) ? values : [values];
    const s = this.host.putStr(name);
    let r;
    try {
      const raw = (i) => Math.round((vals[i] ?? 0) * RAW);
      r = this.e.lx_set_control(this.h, s.ptr, s.len, raw(0), raw(1), raw(2), vals.length);
    } finally {
      s.free();
    }
    return r === I32_MIN ? null : r / RAW;
  }

  /** Exported vars, scaled out of 16.16. */
  vars() {
    this.e.lx_vars(this.h);
    const raw = JSON.parse(this.host.response());
    const out = {};
    for (const [k, v] of Object.entries(raw)) {
      out[k] = Array.isArray(v) ? v.map((x) => x / RAW) : v === null ? null : v / RAW;
    }
    return out;
  }

  /** Row-major W×H grid map (rows implied by pixelCount/w). */
  setMapGrid(w, h) {
    this.e.lx_set_map_grid(this.h, w, h);
  }

  /** Arbitrary map: array of [x,y] or [x,y,z], any units (engine normalizes). */
  setMap(coords) {
    const dims = (coords[0]?.length ?? 2) >= 3 ? 3 : 2;
    const n = coords.length;
    const bytes = n * dims * 4;
    const ptr = this.e.lx_alloc(bytes);
    try {
      const view = new DataView(this.e.memory.buffer);
      for (let i = 0; i < n; i++) {
        for (let d = 0; d < dims; d++) {
          view.setInt32(ptr + (i * dims + d) * 4, Math.round((coords[i]?.[d] ?? 0) * RAW), true);
        }
      }
      this.e.lx_set_map(this.h, dims, ptr, n);
    } finally {
      this.e.lx_dealloc(ptr, bytes);
    }
  }

  setMap3D(points) {
    this.setMap(points.map((p) => [p[0], p[1], p[2] ?? 0]));
  }

  /** Pin the wall clock so time-of-day patterns render deterministically. */
  setWallClock(unixSeconds) {
    this.e.lx_set_wall_clock(this.h, unixSeconds);
  }

  free() {
    if (this.freed) return;
    this.e.lx_free(this.h);
    this.freed = true;
  }
}

/** n×n×n integer lattice — the default geometry for render3D patterns
 *  (mirrors cubeLattice() in web/src/App.svelte). */
export function cubeLattice(n) {
  const coords = [];
  for (let z = 0; z < n; z++)
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) coords.push([x, y, z]);
  return coords;
}
