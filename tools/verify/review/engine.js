// Browser twin of tools/verify/enginehost.mjs — the same luxel-wasm C ABI
// (crates/luxel-wasm/src/lib.rs), fetched instead of read off disk, so a
// render here matches one from snap.mjs/report.mjs bit for bit.
//
// Everything numeric crosses the ABI as raw 16.16 fixed point (raw = v·65536);
// this wrapper converts at the boundary so the UI deals in plain numbers.
//
// SPDX-License-Identifier: Apache-2.0

const RAW = 65536;
const I32_MIN = -2147483648;

let hostPromise = null;

/** Instantiate the engine wasm once for the whole page. */
export function loadEngineHost(url = "/luxel.wasm") {
  hostPromise ??= (async () => {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`GET ${url} → ${res.status}`);
    const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {});
    return new Host(instance.exports);
  })();
  return hostPromise;
}

class Host {
  constructor(exports) {
    this.e = exports;
    this.enc = new TextEncoder();
    this.dec = new TextDecoder();
  }

  /** Live view — refetched every time, the buffer moves when wasm grows. */
  get mem() {
    return new Uint8Array(this.e.memory.buffer);
  }

  putStr(str) {
    const bytes = this.enc.encode(str);
    const ptr = this.e.lx_alloc(bytes.length);
    this.mem.set(bytes, ptr);
    return { ptr, len: bytes.length, free: () => this.e.lx_dealloc(ptr, bytes.length) };
  }

  response() {
    const ptr = this.e.lx_response_ptr();
    const len = this.e.lx_response_len();
    return this.dec.decode(new Uint8Array(this.e.memory.buffer, ptr, len));
  }

  /** Wall clock handed to engines created by FUTURE compile() calls, so
   *  top-level init sees time-of-day — a post-compile setWallClock() is too
   *  late for init-time clockHour() reads (Gitea #104). Non-finite values
   *  throw: an undefined clock silently rendered every clock pattern at
   *  epoch 0 through the whole 2026-08 sweep. Mirrors enginehost.mjs. */
  setDefaultWallClock(unixSeconds) {
    if (!Number.isFinite(unixSeconds))
      throw new Error(`setDefaultWallClock: non-finite wall clock ${unixSeconds}`);
    this.e.lx_set_default_wall_clock(unixSeconds);
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
      return { compileError: String(diagnostic?.message ?? "compile failed"), diagnostic };
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

  /** Render one frame; returns a view of pixelCount*3 RGB bytes valid until
   *  the next wasm call (the canvas paint below consumes it immediately). */
  frame(deltaMs) {
    const ptr = this.e.lx_frame(this.h, Math.round(deltaMs * RAW));
    return new Uint8Array(this.e.memory.buffer, ptr, this.pixelCount * 3);
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

  /** Invoke a control by export name. Returns the shown value, or null if the
   *  pattern has no such control. Pass [] to READ a showNumber/gauge. */
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
      for (let i = 0; i < n; i++)
        for (let d = 0; d < dims; d++)
          view.setInt32(ptr + (i * dims + d) * 4, Math.round((coords[i]?.[d] ?? 0) * RAW), true);
      this.e.lx_set_map(this.h, dims, ptr, n);
    } finally {
      this.e.lx_dealloc(ptr, bytes);
    }
  }

  setMap3D(points) {
    this.setMap(points.map((p) => [p[0], p[1], p[2] ?? 0]));
  }

  /** Pin the wall clock so time-of-day patterns render deterministically.
   *  Non-finite values throw (NaN casts to epoch 0 in the wasm, silently). */
  setWallClock(unixSeconds) {
    if (!Number.isFinite(unixSeconds))
      throw new Error(`setWallClock: non-finite wall clock ${unixSeconds}`);
    this.e.lx_set_wall_clock(this.h, unixSeconds);
  }

  /** True if the pattern EXPORTS any sensor-board global — i.e. whether
   *  feeding it sensor frames can change what it renders at all. */
  wantsSensors() {
    return this.e.lx_wants_sensors(this.h) === 1;
  }

  /** Inject one sensor frame: SENSOR_SLOT_COUNT numbers in ABI slot order. */
  setSensors(slots) {
    const len = slots.length;
    const bytes = len * 4;
    const ptr = this.e.lx_alloc(bytes);
    try {
      // lx_alloc buffers are align-1; DataView reads/writes unaligned fine.
      const view = new DataView(this.e.memory.buffer);
      for (let i = 0; i < len; i++)
        view.setInt32(ptr + i * 4, Math.round((slots[i] ?? 0) * RAW), true);
      this.e.lx_set_sensors(this.h, ptr, len);
    } finally {
      this.e.lx_dealloc(ptr, bytes);
    }
  }

  free() {
    if (this.freed) return;
    this.e.lx_free(this.h);
    this.freed = true;
  }
}

// ---- sensor frames (verbatim layout from enginehost.mjs) --------------------

export const SENSOR_SLOTS = Object.freeze({
  frequencyData: 0,
  frequencyDataLen: 32,
  energyAverage: 32,
  maxFrequencyMagnitude: 33,
  maxFrequency: 34,
  light: 35,
  accelerometer: 36,
  accelerometerLen: 3,
  analogInputs: 39,
  analogInputsLen: 5,
});
export const SENSOR_SLOT_COUNT = 44;

export function sensorSlots(frame = {}) {
  const slots = new Array(SENSOR_SLOT_COUNT).fill(0);
  const put = (at, values, max) => {
    if (!values) return;
    for (let i = 0; i < Math.min(values.length, max); i++) slots[at + i] = values[i];
  };
  put(SENSOR_SLOTS.frequencyData, frame.frequencyData, SENSOR_SLOTS.frequencyDataLen);
  slots[SENSOR_SLOTS.energyAverage] = frame.energyAverage ?? 0;
  slots[SENSOR_SLOTS.maxFrequencyMagnitude] = frame.maxFrequencyMagnitude ?? 0;
  slots[SENSOR_SLOTS.maxFrequency] = frame.maxFrequency ?? 0;
  slots[SENSOR_SLOTS.light] = frame.light ?? 0;
  put(SENSOR_SLOTS.accelerometer, frame.accelerometer, SENSOR_SLOTS.accelerometerLen);
  put(SENSOR_SLOTS.analogInputs, frame.analogInputs, SENSOR_SLOTS.analogInputsLen);
  return slots;
}

/** n×n×n integer lattice — the cloud rig's geometry (mirrors cubeLattice() in
 *  enginehost.mjs and web/src/App.svelte). */
export function cubeLattice(n) {
  const coords = [];
  for (let z = 0; z < n; z++)
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) coords.push([x, y, z]);
  return coords;
}

/** Script-specified control bounds — the `//#` directive, ported from
 *  web/src/lib/hints.ts with one addition: library/*.js files often put the
 *  directive on the line ABOVE the export, so both placements are accepted. */
export function parseControlHints(source) {
  const hints = new Map();
  const put = (name, body) => {
    const hint = hints.get(name) ?? {};
    for (const kv of body.matchAll(/(\w+)\s*=\s*(-?\d*\.?\d+)/g)) {
      const v = Number(kv[2]);
      if (Number.isNaN(v)) continue;
      if (["min", "max", "step", "default"].includes(kv[1])) hint[kv[1]] = v;
    }
    hints.set(name, hint);
  };
  // same line: `export function sliderX(v) { … }  //# min=0 max=5`
  for (const m of source.matchAll(
    /export\s+function\s+([A-Za-z_$][\w$]*)\s*\([^)]*\)[^\n]*?\/\/#([^\n]*)/g,
  ))
    put(m[1], m[2]);
  // line above: `//# min=0 max=5` \n `export function sliderX(v) { … }`
  for (const m of source.matchAll(
    /^[ \t]*\/\/#([^\n]*)\n[ \t]*export\s+function\s+([A-Za-z_$][\w$]*)\s*\(/gm,
  ))
    put(m[2], m[1]);
  return hints;
}
