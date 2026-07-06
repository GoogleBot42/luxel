// Device-mode backend: the playground talking to real hardware (or the
// native mirror, `luxel serve`) over the device HTTP API instead of the
// local wasm engine. Raw 16.16 values cross the wire; this wrapper converts
// at the boundary, mirroring the wasm wrapper's conventions.

import type { Control } from "./luxel";

export interface DeviceStatus {
  fps: number;
  pixels: number;
  vmerr: string | null;
}

export type RunResult = { ok: true } | { ok: false; line: number; col: number; error: string };

const RAW = 65536;

export class DeviceSession {
  /** `base` is "" when served from the device itself, else "http://host[:port]". */
  constructor(readonly base: string) {}

  private url(path: string): string {
    return this.base + path;
  }

  async status(): Promise<DeviceStatus> {
    const res = await fetch(this.url("/api/status"));
    return (await res.json()) as DeviceStatus;
  }

  async pattern(): Promise<string> {
    return (await fetch(this.url("/api/pattern"))).text();
  }

  async run(source: string): Promise<RunResult> {
    const res = await fetch(this.url("/api/code"), { method: "POST", body: source });
    return (await res.json()) as RunResult;
  }

  async pixels(): Promise<Uint8Array> {
    return new Uint8Array(await (await fetch(this.url("/api/pixels"))).arrayBuffer());
  }

  async controls(): Promise<Control[]> {
    return (await (await fetch(this.url("/api/controls"))).json()) as Control[];
  }

  async setControl(name: string, values: number[]): Promise<void> {
    const raws = values.map((v) => Math.round(v * RAW)).join(" ");
    await fetch(this.url("/api/control"), { method: "POST", body: `${name} ${raws}`.trim() });
  }

  async setVar(name: string, value: number): Promise<void> {
    await fetch(this.url("/api/var"), {
      method: "POST",
      body: `${name} ${Math.round(value * RAW)}`,
    });
  }

  async vars(): Promise<Record<string, number | number[]>> {
    const raw = (await (await fetch(this.url("/api/vars"))).json()) as Record<
      string,
      number | number[] | null
    >;
    const out: Record<string, number | number[]> = {};
    for (const [k, v] of Object.entries(raw)) {
      if (v === null) continue;
      out[k] = Array.isArray(v) ? v.map((x) => x / RAW) : v / RAW;
    }
    return out;
  }

  /** Current showNumber/gauge display values. */
  async readouts(): Promise<Map<string, number>> {
    const raw = (await (await fetch(this.url("/api/readouts"))).json()) as Record<
      string,
      number | null
    >;
    const m = new Map<string, number>();
    for (const [k, v] of Object.entries(raw)) {
      if (v !== null) m.set(k, v / RAW);
    }
    return m;
  }
}
