// Typed wrapper around the luxel-wasm C ABI. Values crossing the pattern
// domain are raw 16.16 fixed point (raw = value·65536) — this wrapper
// converts at the boundary so the app deals in plain numbers.

export interface Diagnostic {
  line: number;
  col: number;
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
  lx_set_wall_clock(h: number, unixSeconds: number): void;
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
