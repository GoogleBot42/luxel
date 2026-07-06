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

  private socket: WebSocket | null = null;
  private pending = new Map<number, (r: unknown) => void>();
  private nextId = 1;

  private url(path: string): string {
    return this.base + path;
  }

  /** Multiplex a request over the push socket when it's open (the ESP32
   *  only serves two connections — one socket carries everything), falling
   *  back to `null` so callers use HTTP. Wire: `"<id> <call>\n<body>"` →
   *  `{"id":N,"r":…}`. */
  private wsCall(call: string, body: string): Promise<unknown> | null {
    const ws = this.socket;
    if (!ws || ws.readyState !== WebSocket.OPEN) return null;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("ws call timeout"));
      }, 4000);
      this.pending.set(id, (r) => {
        clearTimeout(timer);
        resolve(r);
      });
      ws.send(`${id} ${call}\n${body}`);
    });
  }

  async status(): Promise<DeviceStatus> {
    const res = await fetch(this.url("/api/status"));
    return (await res.json()) as DeviceStatus;
  }

  async pattern(): Promise<string> {
    return (await fetch(this.url("/api/pattern"))).text();
  }

  async run(source: string): Promise<RunResult> {
    const ws = this.wsCall("code", source);
    if (ws) return (await ws) as RunResult;
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
    const body = `${name} ${values.map((v) => Math.round(v * RAW)).join(" ")}`.trim();
    const ws = this.wsCall("control", body);
    if (ws) {
      await ws;
      return;
    }
    await fetch(this.url("/api/control"), { method: "POST", body });
  }

  async setVar(name: string, value: number): Promise<void> {
    const body = `${name} ${Math.round(value * RAW)}`;
    const ws = this.wsCall("var", body);
    if (ws) {
      await ws;
      return;
    }
    await fetch(this.url("/api/var"), { method: "POST", body });
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

  /** Open the push socket: binary frames = RGB pixels, text frames = typed
   *  JSON (status/vars/readouts). Returns the socket; callers own closing
   *  it. Falls back to HTTP polling if it errors. */
  openSocket(handlers: {
    onPixels: (px: Uint8Array) => void;
    onStatus: (st: DeviceStatus) => void;
    onVars: (vars: Record<string, number | number[]>) => void;
    onReadouts: (r: Map<string, number>) => void;
    onControls: (c: Control[]) => void;
    onClose: () => void;
  }): WebSocket {
    const base = this.base === "" ? window.location.origin : this.base;
    const ws = new WebSocket(base.replace(/^http/, "ws") + "/ws");
    ws.binaryType = "arraybuffer";
    ws.onmessage = (e) => {
      if (typeof e.data === "string") {
        const msg = JSON.parse(e.data) as {
          type?: string;
          id?: number;
          r?: unknown;
          status?: DeviceStatus;
          vars?: Record<string, number | number[] | null>;
          readouts?: Record<string, number | null>;
          controls?: Control[];
        };
        if (msg.id !== undefined) {
          this.pending.get(msg.id)?.(msg.r);
          this.pending.delete(msg.id);
          return;
        }
        if (msg.type === "status" && msg.status) handlers.onStatus(msg.status);
        if (msg.type === "controls" && msg.controls) handlers.onControls(msg.controls);
        if (msg.type === "vars" && msg.vars) {
          const out: Record<string, number | number[]> = {};
          for (const [k, v] of Object.entries(msg.vars)) {
            if (v === null) continue;
            out[k] = Array.isArray(v) ? v.map((x) => x / RAW) : v / RAW;
          }
          handlers.onVars(out);
        }
        if (msg.type === "readouts" && msg.readouts) {
          const m = new Map<string, number>();
          for (const [k, v] of Object.entries(msg.readouts)) {
            if (v !== null) m.set(k, v / RAW);
          }
          handlers.onReadouts(m);
        }
      } else {
        handlers.onPixels(new Uint8Array(e.data as ArrayBuffer));
      }
    };
    ws.onclose = () => {
      if (this.socket === ws) this.socket = null;
      this.pending.clear();
      handlers.onClose();
    };
    ws.onerror = () => ws.close();
    this.socket = ws;
    return ws;
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
