// Typed wrapper around the luxel-wasm C ABI. Values crossing the pattern
// domain are raw 16.16 fixed point (raw = value·65536) — this wrapper
// converts at the boundary so the app deals in plain numbers.

export interface Diagnostic {
  line: number;
  col: number;
  /** Byte offsets into the UTF-8 source (squiggle range). */
  start?: number;
  end?: number;
  message: string;
}

export interface RuntimeError {
  message: string;
  fn: number;
  pc: number;
}

export type ControlKind =
  | "slider"
  | "hsvPicker"
  | "rgbPicker"
  | "toggle"
  | "trigger"
  | "inputNumber"
  | "showNumber"
  | "gauge";

export interface Control {
  kind: ControlKind;
  label: string;
  name: string;
}

export interface DebugLocal {
  name: string;
  raw?: number;
  array?: number;
  fn?: number;
}

export interface DebugStackFrame {
  name: string;
  line: number;
  col: number;
  locals: DebugLocal[];
}

export interface DebugSnapshot {
  paused: boolean;
  line?: number;
  col?: number;
  pixel?: number | null;
  stack?: DebugStackFrame[];
  globals?: DebugLocal[];
}

export type StepKind = "continue" | "over" | "into" | "out";

/** One frame of sensor-board data (the PB sensor expansion board surface).
 *  0..1 throughout, except `accelerometer` (signed) and `maxFrequency` (Hz). */
export interface SensorFrame {
  frequencyData: number[]; // 32 bins, 37 Hz – 10 kHz
  energyAverage: number;
  maxFrequencyMagnitude: number;
  maxFrequency: number;
  light?: number;
  accelerometer?: [number, number, number];
  analogInputs?: number[];
}
const STEP_CODE: Record<StepKind, number> = { continue: 0, over: 1, into: 2, out: 3 };

interface Exports {
  memory: WebAssembly.Memory;
  lx_alloc(len: number): number;
  lx_dealloc(ptr: number, len: number): void;
  lx_response_ptr(): number;
  lx_response_len(): number;
  lx_new(srcPtr: number, srcLen: number, pixelCount: number, seed: number): number;
  lx_free(h: number): void;
  lx_frame(h: number, deltaRaw: number): number;
  lx_take_error(h: number): number;
  lx_controls(h: number): number;
  lx_set_control(
    h: number,
    namePtr: number,
    nameLen: number,
    v0: number,
    v1: number,
    v2: number,
    argc: number,
  ): number;
  lx_vars(h: number): number;
  lx_set_var(h: number, namePtr: number, nameLen: number, raw: number): number;
  lx_set_map_grid(h: number, w: number, gridH: number): void;
  lx_set_map(h: number, dims: number, ptr: number, count: number): void;
  lx_enable_map_mode(h: number): void;
  lx_run_map(h: number): number;
  lx_map_dims(h: number): number;
  lx_map_count(h: number): number;
  lx_map_coords(h: number): number;
  lx_set_wall_clock(h: number, unixSeconds: number): void;
  lx_wants_sensors(h: number): number;
  lx_set_sensors(h: number, ptr: number, len: number): void;
  lx_pixels(h: number): number;
  lx_debug_enable(h: number, on: number): void;
  lx_debug_set_breakpoints(h: number, ptr: number, len: number): void;
  lx_debug_pause(h: number): void;
  lx_debug_paused(h: number): number;
  lx_debug_step(h: number, kind: number): number;
  lx_debug_state(h: number): number;
  lx_globals(h: number): number;
}

const RAW = 65536;
const I32_MIN = -2147483648;

export class Luxel {
  private constructor(private e: Exports) {}

  static async load(url: string): Promise<Luxel> {
    const res = await fetch(url);
    const { instance } = await WebAssembly.instantiateStreaming(res, {});
    return new Luxel(instance.exports as unknown as Exports);
  }

  compile(source: string, pixelCount: number, seed = 1): Engine | Diagnostic {
    const s = this.putStr(source);
    const h = this.e.lx_new(s.ptr, s.len, pixelCount, seed);
    s.free();
    if (h < 0) return JSON.parse(this.response()) as Diagnostic;
    return new Engine(this.e, this, h, pixelCount);
  }

  /** Compile a *map program*: a Luxel program whose `render(index)` calls
   *  `plot(x, y[, z])` once per pixel. Runs on the VM (so it's debuggable like
   *  a pattern); `Engine.runMap()` collects the coordinates. */
  compileMap(source: string, pixelCount: number, seed = 1): Engine | Diagnostic {
    const eng = this.compile(source, pixelCount, seed);
    if (eng instanceof Engine) eng.enableMapMode();
    return eng;
  }

  putStr(str: string): { ptr: number; len: number; free: () => void } {
    const bytes = new TextEncoder().encode(str);
    const ptr = this.e.lx_alloc(bytes.length);
    new Uint8Array(this.e.memory.buffer).set(bytes, ptr);
    return { ptr, len: bytes.length, free: () => this.e.lx_dealloc(ptr, bytes.length) };
  }

  response(): string {
    const ptr = this.e.lx_response_ptr();
    const len = this.e.lx_response_len();
    return new TextDecoder().decode(new Uint8Array(this.e.memory.buffer, ptr, len));
  }
}

export class Engine {
  private freed = false;

  constructor(
    private e: Exports,
    private lx: Luxel,
    private h: number,
    readonly pixelCount: number,
  ) {}

  /** Render a frame; returns a copy of the RGB bytes (pixelCount·3). */
  frame(deltaMs: number): Uint8Array {
    const ptr = this.e.lx_frame(this.h, Math.round(deltaMs * RAW));
    return new Uint8Array(this.e.memory.buffer.slice(ptr, ptr + this.pixelCount * 3));
  }

  takeError(): RuntimeError | null {
    if (this.e.lx_take_error(this.h) === 0) return null;
    return JSON.parse(this.lx.response()) as RuntimeError;
  }

  controls(): Control[] {
    this.e.lx_controls(this.h);
    return JSON.parse(this.lx.response()) as Control[];
  }

  /** Invoke a control; returns the shown value for showNumber/gauge. */
  setControl(name: string, values: number[]): number | null {
    const s = this.lx.putStr(name);
    const raw = (i: number) => Math.round((values[i] ?? 0) * RAW);
    const r = this.e.lx_set_control(this.h, s.ptr, s.len, raw(0), raw(1), raw(2), values.length);
    s.free();
    return r === I32_MIN ? null : r / RAW;
  }

  vars(): Record<string, number | number[]> {
    this.e.lx_vars(this.h);
    const raw = JSON.parse(this.lx.response()) as Record<string, number | number[]>;
    const out: Record<string, number | number[]> = {};
    for (const [k, v] of Object.entries(raw)) {
      out[k] = Array.isArray(v) ? v.map((x) => x / RAW) : v / RAW;
    }
    return out;
  }

  setVar(name: string, value: number): boolean {
    const s = this.lx.putStr(name);
    const ok = this.e.lx_set_var(this.h, s.ptr, s.len, Math.round(value * RAW)) === 1;
    s.free();
    return ok;
  }

  setMapGrid(w: number, h: number): void {
    this.e.lx_set_map_grid(this.h, w, h);
  }

  /** Install an arbitrary pixel map (one [x,y] or [x,y,z] per pixel, any
   *  units — the engine normalizes per axis). */
  setMap(coords: number[][]): void {
    const dims = (coords[0]?.length ?? 2) >= 3 ? 3 : 2;
    const n = coords.length;
    const bytes = n * dims * 4;
    const ptr = this.e.lx_alloc(bytes);
    const view = new DataView(this.e.memory.buffer);
    for (let i = 0; i < n; i++) {
      for (let d = 0; d < dims; d++) {
        view.setInt32(ptr + (i * dims + d) * 4, Math.round((coords[i]?.[d] ?? 0) * RAW), true);
      }
    }
    this.e.lx_set_map(this.h, dims, ptr, n);
    this.e.lx_dealloc(ptr, bytes);
  }

  /** True if the pattern binds any sensor-board variable (frequencyData,
   *  energyAverage, …) — capture audio only when it's actually consumed. */
  wantsSensors(): boolean {
    return this.e.lx_wants_sensors(this.h) === 1;
  }

  /** Inject one sensor frame (PB sensor-board surface). All values 0..1
   *  except accelerometer (signed) and maxFrequency (Hz). */
  setSensors(s: SensorFrame): void {
    const vals = [
      ...Array.from({ length: 32 }, (_, i) => s.frequencyData[i] ?? 0),
      s.energyAverage,
      s.maxFrequencyMagnitude,
      s.maxFrequency,
      s.light ?? 0,
      ...(s.accelerometer ?? [0, 0, 0]),
      ...(s.analogInputs ?? [0, 0, 0, 0, 0]),
    ];
    const bytes = vals.length * 4;
    const ptr = this.e.lx_alloc(bytes);
    const view = new DataView(this.e.memory.buffer);
    for (let i = 0; i < vals.length; i++) {
      view.setInt32(ptr + i * 4, Math.round((vals[i] ?? 0) * RAW), true);
    }
    this.e.lx_set_sensors(this.h, ptr, vals.length);
    this.e.lx_dealloc(ptr, bytes);
  }

  // ---- map mode (this engine emits coordinates, not colors) ----

  enableMapMode(): void {
    this.e.lx_enable_map_mode(this.h);
  }

  /** Run (or resume) the map program over every pixel. Returns whether it
   *  suspended at a debug stop, plus the coordinates collected so far. */
  runMap(): { paused: boolean; dims: number; coords: number[][] } {
    const paused = this.e.lx_run_map(this.h) === 1;
    return { paused, ...this.mapResult() };
  }

  /** The coordinates collected by the last map run (pattern units). */
  mapResult(): { dims: number; coords: number[][] } {
    const dims = this.e.lx_map_dims(this.h);
    const count = this.e.lx_map_count(this.h);
    const ptr = this.e.lx_map_coords(this.h);
    const raw = new Int32Array(this.e.memory.buffer, ptr, count * 3);
    const coords: number[][] = [];
    for (let i = 0; i < count; i++) {
      const o = i * 3;
      const p = [(raw[o] ?? 0) / RAW, (raw[o + 1] ?? 0) / RAW];
      if (dims === 3) p.push((raw[o + 2] ?? 0) / RAW);
      coords.push(p);
    }
    return { dims, coords };
  }

  /** Current pixel buffer without rendering (partial frames while paused). */
  pixels(): Uint8Array {
    const ptr = this.e.lx_pixels(this.h);
    return new Uint8Array(this.e.memory.buffer.slice(ptr, ptr + this.pixelCount * 3));
  }

  debugEnable(on: boolean): void {
    this.e.lx_debug_enable(this.h, on ? 1 : 0);
  }

  /** Replace breakpoints (1-based lines); returns the resolved lines. */
  setBreakpoints(lines: number[]): number[] {
    const ptr = this.e.lx_alloc(Math.max(lines.length * 4, 1));
    const view = new DataView(this.e.memory.buffer);
    lines.forEach((l, i) => view.setUint32(ptr + i * 4, l, true));
    this.e.lx_debug_set_breakpoints(this.h, ptr, lines.length);
    this.e.lx_dealloc(ptr, Math.max(lines.length * 4, 1));
    return JSON.parse(this.lx.response()) as number[];
  }

  debugPause(): void {
    this.e.lx_debug_pause(this.h);
  }

  debugPaused(): boolean {
    return this.e.lx_debug_paused(this.h) === 1;
  }

  /** Resume with a step plan; returns whether still paused. */
  debugStep(kind: StepKind): boolean {
    return this.e.lx_debug_step(this.h, STEP_CODE[kind]) === 1;
  }

  debugState(): DebugSnapshot {
    this.e.lx_debug_state(this.h);
    return JSON.parse(this.lx.response()) as DebugSnapshot;
  }

  /** All user-defined globals with current values. */
  globals(): DebugLocal[] {
    this.e.lx_globals(this.h);
    return JSON.parse(this.lx.response()) as DebugLocal[];
  }

  setWallClock(unixSeconds: number): void {
    this.e.lx_set_wall_clock(this.h, unixSeconds);
  }

  free(): void {
    if (!this.freed) {
      this.e.lx_free(this.h);
      this.freed = true;
    }
  }
}
