// Typed wrapper around the luxel-wasm C ABI. Values crossing the pattern
// domain are raw 16.16 fixed point (raw = value·65536) — this wrapper
// converts at the boundary so the app deals in plain numbers.

import { gatedFetch } from "./fetchgate";
import {
  DEFAULT_PROJECTION,
  PROJECTION_CODES,
  type Projection,
  type ProjectionMode,
} from "./geometry";

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
  lx_bytecode(h: number): number;
  lx_bytecode_ptr(h: number): number;
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
  lx_set_strip_layout(h: number): void;
  lx_layout_dims(h: number): number;
  lx_set_projection(h: number, one: number, two: number, three: number): number;
  lx_projection(h: number): number;
  lx_projection_options(patternDims: number, layoutDims: number): number;
  lx_effective_geometry(h: number): number;
  lx_set_map(h: number, dims: number, ptr: number, count: number): void;
  lx_enable_map_mode(h: number): void;
  lx_run_map(h: number): number;
  lx_map_dims(h: number): number;
  lx_map_count(h: number): number;
  lx_map_coords(h: number): number;
  lx_set_wall_clock(h: number, unixSeconds: number): void;
  lx_set_default_wall_clock(unixSeconds: number): void;
  lx_wants_sensors(h: number): number;
  lx_preferred_dims(h: number): number;
  lx_set_sensors(h: number, ptr: number, len: number): void;
  lx_push_event(h: number, t: number, x: number, y: number, v: number): void;
  lx_set_pin(h: number, pin: number, level: number): number;
  lx_pin_read(h: number, pin: number): number;
  lx_pins_used(h: number, half: number): number;
  lx_pins_idle_high(h: number, half: number): number;
  lx_set_analog_pin(h: number, pin: number, value: number): number;
  lx_analog_read(h: number, pin: number): number;
  lx_analog_pins_used(h: number, half: number): number;
  lx_pixels(h: number): number;
  lx_outpipe_set(h: number, ptr: number, len: number): number;
  lx_outpipe(h: number): number;
  lx_outpipe_bytes(h: number): number;
  lx_debug_enable(h: number, on: number): void;
  lx_debug_set_breakpoints(h: number, ptr: number, len: number): void;
  lx_debug_pause(h: number): void;
  lx_debug_paused(h: number): number;
  lx_debug_step(h: number, kind: number): number;
  lx_debug_state(h: number): number;
  lx_globals(h: number): number;
  lx_device_model(
    blobPtr: number,
    blobLen: number,
    envelopeLen: number,
    pixelCount: number,
    heapFree: number,
    engineHeap: number,
  ): number;
}

/** Wire colour order, as `GET /api/output` reports it. */
export type ColorOrder = "rgb" | "rbg" | "grb" | "gbr" | "brg" | "bgr";
const COLOR_ORDERS: ColorOrder[] = ["rgb", "rbg", "grb", "gbr", "brg", "bgr"];

/** The DEVICE output chain's settings — the Settings page's chain, which runs
 *  on the finished frame just before protocol encoding, on top of whatever the
 *  pattern's own `setBlur`/`setGlow`/`setOutputPalette` already did.
 *
 *  Every field is `GET /api/output` verbatim except the last three, which the
 *  device knows and the endpoint does not report: its brightness
 *  (`GET /api/brightness`) and its per-board current model, both of which only
 *  matter when a power cap is set. Defaults are "stage off", so
 *  `setOutpipe({})` is a no-op chain. */
export interface OutpipeSettings {
  /** `/api/output` `order`. */
  order?: ColorOrder;
  /** `/api/output` `gamma`, in TENTHS (22 = γ2.2). 0 and 10 both mean off. */
  gamma?: number;
  /** `/api/output` `capMa`. 0 = no cap. */
  capMa?: number;
  /** `/api/output` `brightCurve`, in tenths. 0 and 10 mean off. */
  brightCurve?: number;
  /** `/api/output` `blur`, percent. */
  blur?: number;
  /** `/api/output` `glow`, percent. */
  glow?: number;
  /** `/api/output` `palette` — the flat `[pos, r, g, b, …]` array, 0..255
   *  each, exactly as the endpoint returns it. */
  palette?: number[];
  /** `/api/output` `paletteAmount`, percent. 0 = off. */
  paletteAmount?: number;
  /** `GET /api/brightness` `brightness`, 0..31. Only the power cap reads it
   *  (the chain does not dim the frame — the driver does). Default 31. */
  brightness?: number;
  /** The device's per-pixel current model: a strip conducts every pixel at
   *  once, a HUB75 panel time-multiplexes rows. `caps.panel` on
   *  `/api/status` says which. Default `"strip"`. */
  powerModel?: "strip" | "hub75";
  /** Panel rows / 2 (32 for a 1/32-scan 64-row panel). Read only when
   *  `powerModel` is `"hub75"`. Default 32. */
  panelScan?: number;
}

/** How a modelled pattern fits the device (`luxel_core::budget::Fit`). */
export type DeviceFit = "fits" | "tight" | "over";

/** What a pattern would cost the connected device, modelled by replaying the
 *  firmware's own load sequence under a counting allocator.
 *
 *  The unprefixed fields are the LIVE push (`POST /api/code`), which is what
 *  the editor does on every recompile; the `stored*` fields are the same
 *  pattern activated from the device's own library, where the program's code
 *  and constant pool are borrowed from the flash mapping instead of copied
 *  (Gitea #276/#300). The stored path is always the cheaper one. */
export interface DeviceModel {
  /** Bytes the pattern still occupies once the load settles — what the
   *  device's post-load floor check measures. */
  resident: number;
  /** Transient high-water of the load window (upload envelope + decoded
   *  program + the store's write staging). Never reaches the floor check,
   *  but has to fit in free heap or the decode fails to allocate. */
  peak: number;
  /** Array-arena byte budget the device would grant this load. */
  budget: number;
  storedResident: number;
  storedPeak: number;
  storedBudget: number;
  /** Free heap the load starts from: `heap_free + engine_heap`. */
  base: number;
  /** Resident bytes available before the post-load floor check rejects. */
  headroom: number;
  /** The device's runtime floor — heap the firmware keeps for itself. */
  floor: number;
  fit: DeviceFit;
  storedFit: DeviceFit;
  /** Set when the pattern blew the array budget rather than the floor. */
  vmerr: string | null;
  storedVmerr: string | null;
}

// The projection vocabulary lives in `lib/geometry.ts` with the Layout it
// belongs to (Gitea #463) and is re-exported here, which is where every call
// site already imports it from.
export { DEFAULT_PROJECTION, PROJECTION_CODES };
export type { Projection, ProjectionMode };

/** One cell of the §5.4d table, as the engine lists it (display order,
 *  first = default). Labels come from the engine so every surface captions a
 *  projection identically. */
export interface ProjectionOption {
  mode: ProjectionMode;
  code: number;
  label: string;
}

/** What the pattern sees once the projection is applied — what a tile
 *  caption, a thumbnail and a preview must all be sized from. */
export interface EffectiveGeometry {
  /** What `pixelCount` reads inside the pattern. */
  pixelCount: number;
  patternDims: 1 | 2 | 3;
  layoutDims: 1 | 2 | 3;
  /** The grid the pattern's grid-space builtins see; 0 when there is none. */
  w: number;
  h: number;
  /** null when the pattern is native to the Layout — and also when it is
   *  incompatible with it, which has no projection to be in force. */
  mode: ProjectionMode | null;
  label: string | null;
  /** False when the Layout cannot show a pattern of this dimensionality at
   *  all (#538): a strip handed a 2D/3D pattern, a plane handed a 3D one. It
   *  still renders — on the engine's fallback coordinates — so a host that
   *  activates one anyway does not go dark. */
  compatible: boolean;
}

const PROJECTION_NAMES: ProjectionMode[] = ["index", "x", "y", "z", "xy", "xz", "yz"];

function projectionName(code: number): ProjectionMode {
  return PROJECTION_NAMES[code] ?? "index";
}

const RAW = 65536;
const I32_MIN = -2147483648;

export class Luxel {
  private constructor(private e: Exports) {}

  static async load(url: string): Promise<Luxel> {
    // Gated: on a device this shares the 2-socket pool with the startup
    // API probe — an ungated parallel burst gets TCP-refused (the
    // reproducible cold-load casualty was exactly this wasm fetch).
    const res = await gatedFetch(url);
    const { instance } = await WebAssembly.instantiateStreaming(res, {});
    return new Luxel(instance.exports as unknown as Exports);
  }

  compile(source: string, pixelCount: number, seed = 1): Engine | Diagnostic {
    // Wall clock BEFORE lx_new so top-level clockHour()-family reads see
    // real time during init, matching a device with RTC set (Gitea #104).
    // Same UTC convention as the per-frame setWallClock callers.
    this.e.lx_set_default_wall_clock(Date.now() / 1000);
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

  /** Model this compiled blob's cost on a device with `heapFree` bytes free.
   *
   *  Runs the firmware's decode → budgeted-engine → frames sequence inside
   *  wasm with a counting allocator watching, which is why the answer is a
   *  measurement rather than a size heuristic. `envelopeLen` is the size of
   *  the LXP1 upload the device will be holding while it decodes (see
   *  `lxpEnvelope`) — for a source-heavy pattern that overlap, not the
   *  engine, is the peak. Returns null if the blob won't decode, or if this
   *  wasm build predates the export. */
  deviceModel(
    bytecode: Uint8Array,
    envelopeLen: number,
    pixelCount: number,
    heapFree: number,
    engineHeap: number,
  ): DeviceModel | null {
    if (typeof this.e.lx_device_model !== "function") return null;
    const ptr = this.e.lx_alloc(bytecode.length);
    try {
      new Uint8Array(this.e.memory.buffer).set(bytecode, ptr);
      const rc = this.e.lx_device_model(
        ptr,
        bytecode.length,
        envelopeLen,
        pixelCount,
        heapFree,
        engineHeap,
      );
      if (rc < 0) return null;
      return JSON.parse(this.response()) as DeviceModel;
    } finally {
      this.e.lx_dealloc(ptr, bytecode.length);
    }
  }

  /** The projection choices that mean anything for a pattern of
   *  `patternDims` on a Layout of `layoutDims` (1/2/3; `preferredDims()`'s 0
   *  reads as 1) — one row of the §5.4d table, in display order, first =
   *  default. Empty for a native pair, so a UI shows the row only when this
   *  has entries, and a one-option cell is a note rather than a control. */
  projectionOptions(patternDims: number, layoutDims: number): ProjectionOption[] {
    if (typeof this.e.lx_projection_options !== "function") return [];
    const n = this.e.lx_projection_options(patternDims, layoutDims);
    if (n <= 0) return [];
    return JSON.parse(this.response()) as ProjectionOption[];
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

  /** Install the 1D Layout: no map, so the pattern's x is the strip's own
   *  `index / pixelCount`. Without it a 2D-only pattern keeps the engine's
   *  fabricated ceil(√n) grid, and the Layout reads as 2D — which is also
   *  what makes `compatible` false instead of hiding a strip behind a grid
   *  that is not there (#538). */
  setStripLayout(): void {
    this.e.lx_set_strip_layout(this.h);
  }

  /** The Layout's dimensionality as the engine sees it (1/2/3), independent
   *  of any projection. */
  layoutDims(): 1 | 2 | 3 {
    const d = this.e.lx_layout_dims(this.h);
    return d === 3 ? 3 : d === 2 ? 2 : 1;
  }

  /** Install the projection defaults (device `/api/layout` triple, #465):
   *  how a pattern whose dimensionality differs from the Layout's is shown.
   *  Only the field matching this pattern's dims is consulted, so one triple
   *  survives pattern and Layout changes. Call it before the first frame —
   *  `pixelCount` under an along-axis projection becomes the strip length,
   *  and the pattern's top-level init has already run. */
  setProjection(p: Projection): void {
    this.e.lx_set_projection(
      this.h,
      PROJECTION_CODES[p.proj1d] ?? 0,
      PROJECTION_CODES[p.proj2d] ?? 3,
      PROJECTION_CODES[p.proj3d] ?? 4,
    );
  }

  /** The installed triple. */
  projection(): Projection {
    const packed = this.e.lx_projection(this.h);
    if (packed < 0) return { ...DEFAULT_PROJECTION };
    return {
      proj1d: projectionName(packed & 0xff),
      proj2d: projectionName((packed >> 8) & 0xff),
      proj3d: projectionName((packed >> 16) & 0xff),
    };
  }

  /** What the pattern sees this frame once the projection is applied:
   *  `pixelCount`, its own dims, the grid its grid builtins read, and the
   *  projection in force (`mode: null` = native). */
  effectiveGeometry(): EffectiveGeometry {
    if (this.e.lx_effective_geometry(this.h) !== 1) {
      return {
        pixelCount: this.pixelCount,
        patternDims: 1,
        layoutDims: 1,
        w: 0,
        h: 0,
        mode: null,
        label: null,
        compatible: true,
      };
    }
    return JSON.parse(this.lx.response()) as EffectiveGeometry;
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

  /** Configure the DEVICE output chain for this engine (Gitea #466).
   *
   *  `lx_frame` returns the engine's raw frame; the device puts that through
   *  a second chain — palette remap, blur, glow, colour order, gamma, power
   *  cap — before it reaches the wire, so a preview that skips it diverges
   *  from the device by the whole Settings page. `setOutpipe` +
   *  `outpipe()` run the SAME `luxel_core::outpipe::DeviceChain` the firmware
   *  runs, so the two agree by construction.
   *
   *  Feed it `GET /api/output` plus `GET /api/brightness` and the device's
   *  `caps.panel`. Call it when those change, not per frame. */
  setOutpipe(s: OutpipeSettings): void {
    const pal = s.palette ?? [];
    const stops = Math.floor(pal.length / 4);
    const head = [
      Math.max(0, COLOR_ORDERS.indexOf(s.order ?? "rgb")),
      s.gamma ?? 0,
      s.capMa ?? 0,
      s.blur ?? 0,
      s.glow ?? 0,
      s.paletteAmount ?? 0,
      s.brightness ?? 31,
      s.brightCurve ?? 0,
      s.powerModel === "hub75" ? 1 : 0,
      s.panelScan ?? 32,
      stops,
    ];
    const vals = head.concat(pal.slice(0, stops * 4));
    const bytes = vals.length * 4;
    const ptr = this.e.lx_alloc(bytes);
    const view = new DataView(this.e.memory.buffer);
    vals.forEach((v, i) => view.setInt32(ptr + i * 4, Math.round(v), true));
    this.e.lx_outpipe_set(this.h, ptr, vals.length);
    this.e.lx_dealloc(ptr, bytes);
  }

  /** The current frame AFTER the device output chain — what the wire would
   *  carry. Identical to `frame()`'s bytes while every stage is off. Call
   *  after `frame()`; see `setOutpipe`. */
  outpipe(): Uint8Array {
    const ptr = this.e.lx_outpipe(this.h);
    return new Uint8Array(this.e.memory.buffer.slice(ptr, ptr + this.pixelCount * 3));
  }

  /** Bytes the device chain holds right now — the 3 B/px scratch plus any
   *  cooked LUTs, 0 while every stage is off. The same number a device loses
   *  from `heap_free` when a Settings stage is switched on (Gitea #446). */
  outpipeBytes(): number {
    return this.e.lx_outpipe_bytes(this.h);
  }

  /** True if the pattern binds any sensor-board variable (frequencyData,
   *  energyAverage, …) — capture audio only when it's actually consumed. */
  wantsSensors(): boolean {
    return this.e.lx_wants_sensors(this.h) === 1;
  }

  /** The geometry the COMPILED pattern asks for — 0 = strip, 2 = a 2D grid
   *  (`render2D`, or `renderFrame` plus a coordinate/grid-space bulk op),
   *  3 = a 3D point cloud (`render3D` only). Read off the compiled program,
   *  not the source text, so a `render2D` in a comment or a string never
   *  counts. The playground picks its default preview rig from this
   *  (Gitea #372). */
  preferredDims(): 0 | 2 | 3 {
    const d = this.e.lx_preferred_dims(this.h);
    return d === 2 ? 2 : d === 3 ? 3 : 0;
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

  /** Queue one external event for the pattern to read via `readEvent(out)`
   *  as `[type, x, y, value]`. x/y are normalized 0..1; type and value are
   *  source-defined (preview clicks send type 1, value 1). */
  pushEvent(type: number, x: number, y: number, value = 1): void {
    const raw = (v: number) => Math.round(v * RAW) | 0;
    this.e.lx_push_event(this.h, raw(type), raw(x), raw(y), raw(value));
  }

  /** Drive a digital input pin so `digitalRead(pin)` reports `level` instead
   *  of the pin's idle level — the stand-in for real GPIO (Gitea #177).
   *  `level: null` releases the pin back to idle (HIGH under a pull-up).
   *  Returns false when the pin is outside the tracked window (0..63). */
  setPin(pin: number, level: boolean | null): boolean {
    return this.e.lx_set_pin(this.h, pin | 0, level === null ? -1 : level ? 1 : 0) === 1;
  }

  /** What `digitalRead(pin)` reports right now — injected level if driven,
   *  otherwise the pin's `pinMode` idle level. */
  pinRead(pin: number): boolean {
    return this.e.lx_pin_read(this.h, pin | 0) === 1;
  }

  /** Pins the running pattern has actually named in a `pinMode`/`digitalRead`,
   *  ascending (Gitea #205). Pin numbers are runtime values, so this is the
   *  only honest way to know which pins deserve a control — an empty array
   *  means the pattern doesn't touch GPIO at all. Sticky for the life of the
   *  engine, so a pin read inside a rare branch stays listed once seen. */
  pinsUsed(): number[] {
    const pins: number[] = [];
    for (let half = 0; half < 2; half++) {
      const bits = this.e.lx_pins_used(this.h, half) >>> 0;
      for (let b = 0; b < 32; b++) if (bits & (1 << b)) pins.push(half * 32 + b);
    }
    return pins;
  }

  /** True when `pin` idles HIGH — `pinMode` asked for a pull-up, so
   *  `digitalRead` reads HIGH until something drives it (and "pressing" the
   *  pin means pulling it LOW, the button-to-ground wiring). */
  pinIdleHigh(pin: number): boolean {
    const p = pin | 0;
    if (p < 0 || p > 63) return false;
    const bits = this.e.lx_pins_idle_high(this.h, p >= 32 ? 1 : 0) >>> 0;
    return (bits & (1 << p % 32)) !== 0;
  }

  /** Drive an analog input pin so `analogRead(pin)`/`touchRead(pin)` report
   *  `value` (0..1) instead of the 0 they read undriven — the analog half of
   *  the pin-injection ABI (Gitea #206). Both builtins share one value per
   *  pin, and 0 is the undriven reading, so there is no separate release.
   *  Returns false when the pin is outside the tracked window (0..63). */
  setAnalogPin(pin: number, value: number): boolean {
    const v = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
    return this.e.lx_set_analog_pin(this.h, pin | 0, Math.round(v * RAW) | 0) === 1;
  }

  /** What `analogRead(pin)`/`touchRead(pin)` report right now, 0..1. */
  analogRead(pin: number): number {
    return this.e.lx_analog_read(this.h, pin | 0) / RAW;
  }

  /** Pins the running pattern has actually sampled with `analogRead`/
   *  `touchRead`, ascending (Gitea #206) — the analog counterpart of
   *  `pinsUsed`, and what gates the panel's sliders. */
  analogPinsUsed(): number[] {
    const pins: number[] = [];
    for (let half = 0; half < 2; half++) {
      const bits = this.e.lx_analog_pins_used(this.h, half) >>> 0;
      for (let b = 0; b < 32; b++) if (bits & (1 << b)) pins.push(half * 32 + b);
    }
    return pins;
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

  /** Serialize the compiled program to LXBC bytecode — what devices execute
   *  (they carry no compiler). Uploads pair this with the source. */
  bytecode(): Uint8Array {
    const len = this.e.lx_bytecode(this.h);
    if (len < 0) throw new Error(JSON.parse(this.lx.response()).message as string);
    const ptr = this.e.lx_bytecode_ptr(this.h);
    return new Uint8Array(this.e.memory.buffer.slice(ptr, ptr + len));
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
