// Device-mode backend: the playground talking to real hardware (or the
// native mirror, `luxel serve`) over the device HTTP API instead of the
// local wasm engine. Raw 16.16 values cross the wire; this wrapper converts
// at the boundary, mirroring the wasm wrapper's conventions.

import { gatedFetch } from "./fetchgate";

export interface DeviceStatus {
  fps: number;
  pixels: number;
  /** The device's hard pixel-count cap, which is PER BOARD (a 64x64 HUB75
   *  panel board reports 4096, strip boards 2048). Absent on firmware older
   *  than the field — fall back to GET /api/config's `max`, and only then to
   *  a built-in default. Never assume a constant: the whole point of the
   *  field is that the UI clamps to the connected board, not to 2048. */
  max_pixels?: number;
  vmerr: string | null;
  /** Network input currently driving the strip (DDP/E1.31), if any. */
  live?: "ddp" | "e131" | null;
  /** Free heap in bytes, measured with the CURRENT pattern still loaded —
   *  which makes it the headroom an incoming pattern has to fit inside, since
   *  the firmware builds the new engine before releasing the old one.
   *  0 or absent means "this device can't report it" (the native mirror
   *  without `--heap-free`, or firmware older than the field): treat as
   *  unknown and don't guess at capacity. */
  heap_free?: number;
}

export type RunResult =
  | { ok: true }
  | { ok: false; line?: number; col?: number; error: string; code?: string };

/** LXP1 envelope: how a pattern crosses the wire to a device — name (empty
 *  for ad-hoc runs), source, and the LXBC bytecode the browser compiled.
 *  Devices execute bytecode only; the source is stored alongside it. */
export function lxpEnvelope(
  name: string,
  source: string,
  bytecode: Uint8Array,
): Uint8Array<ArrayBuffer> {
  const enc = new TextEncoder();
  const nameB = enc.encode(name).slice(0, 255);
  const srcB = enc.encode(source);
  const out = new Uint8Array(
    new ArrayBuffer(4 + 1 + nameB.length + 4 + srcB.length + 4 + bytecode.length),
  );
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

/** Luxel-to-Luxel sync state (GET /api/sync). */
export interface SyncStatus {
  mode: "off" | "leader" | "follower";
  timeMs: number;
  leader: { bootId: number; ageMs: number; offsetMs: number } | null;
}

/** MQTT broker settings + connection state (GET /api/mqtt). */
export interface MqttStatus {
  enabled: boolean;
  host: string;
  port: number;
  user: string;
  hasPass: boolean;
  connected: boolean;
}

const RAW = 65536;

export class DeviceSession {
  /** `base` is "" when served from the device itself, else "http://host[:port]". */
  constructor(readonly base: string) {}

  private url(path: string): string {
    return this.base + path;
  }

  /** All API traffic goes through the global fetch gate (see fetchgate.ts):
   *  in-flight cap matching the device's 2-socket pool + backoff-retry on
   *  refused connections. Retrying POSTs is safe here: a refused connection
   *  was never processed, and every mutating endpoint in this API is
   *  idempotent (set-value, overwrite-by-name, delete). */
  private fetch(path: string, init?: RequestInit): Promise<Response> {
    return gatedFetch(this.url(path), init);
  }

  async status(): Promise<DeviceStatus> {
    const res = await this.fetch("/api/status");
    return (await res.json()) as DeviceStatus;
  }

  async pattern(): Promise<string> {
    return (await this.fetch("/api/pattern")).text();
  }

  /** Current output brightness (0–31) and its max. */
  async brightness(): Promise<{ brightness: number; max: number }> {
    return (await (await this.fetch("/api/brightness")).json()) as {
      brightness: number;
      max: number;
    };
  }

  /** Set output brightness (0–31); applied live and persisted on the device. */
  async setBrightness(value: number): Promise<{ ok: boolean; brightness?: number }> {
    const res = await this.fetch("/api/brightness", {
      method: "POST",
      body: String(Math.max(0, Math.min(31, Math.round(value)))),
    });
    return (await res.json()) as { ok: boolean; brightness?: number };
  }

  /** Device config: pixel count, its max, and the LED protocol. */
  async config(): Promise<{ pixels: number; max: number; protocol: string }> {
    return (await (await this.fetch("/api/config")).json()) as {
      pixels: number;
      max: number;
      protocol: string;
    };
  }

  /** Set the pixel count; the device resizes its strip live (no reboot). */
  async setConfig(pixels: number): Promise<{ ok: boolean; pixels?: number; error?: string }> {
    const res = await this.fetch("/api/config", {
      method: "POST",
      body: String(Math.max(1, Math.round(pixels))),
    });
    return (await res.json()) as { ok: boolean; pixels?: number; error?: string };
  }

  /** Current LED protocol and the selectable options. */
  async protocol(): Promise<{ protocol: string; options: string[] }> {
    return (await (await this.fetch("/api/protocol")).json()) as {
      protocol: string;
      options: string[];
    };
  }

  /** Set the LED protocol; the device reconfigures its driver live (no reboot). */
  async setProtocol(name: string): Promise<{ ok: boolean; protocol?: string; error?: string }> {
    const res = await this.fetch("/api/protocol", { method: "POST", body: name });
    return (await res.json()) as { ok: boolean; protocol?: string; error?: string };
  }

  async run(source: string, bytecode: Uint8Array): Promise<RunResult> {
    const res = await this.fetch("/api/code", {
      method: "POST",
      body: lxpEnvelope("", source, bytecode),
    });
    return (await res.json()) as RunResult;
  }

  async setControl(name: string, values: number[]): Promise<void> {
    const body = `${name} ${values.map((v) => Math.round(v * RAW)).join(" ")}`.trim();
    await this.fetch("/api/control", { method: "POST", body });
  }

  // ---- device pattern library (see serve.rs / server.rs contract) ----

  async patterns(): Promise<{ id: string; name: string }[]> {
    const r = (await (await this.fetch("/api/patterns")).json()) as {
      patterns?: { id: string; name: string }[];
    };
    return r.patterns ?? [];
  }

  async patternSource(id: string): Promise<{ id: string; name: string; source: string }> {
    return (await (await this.fetch(`/api/patterns/${id}`)).json()) as {
      id: string;
      name: string;
      source: string;
    };
  }

  /** Save (same name overwrites). Body is an LXP1 envelope — the device
   *  stores source + bytecode and validates only that the blob decodes. */
  async savePattern(
    name: string,
    source: string,
    bytecode: Uint8Array,
  ): Promise<RunResult & { id?: string }> {
    const res = await this.fetch("/api/patterns", {
      method: "POST",
      body: lxpEnvelope(name, source, bytecode),
    });
    return (await res.json()) as RunResult & { id?: string };
  }

  async deletePattern(id: string): Promise<void> {
    await this.fetch(`/api/patterns/${id}`, { method: "DELETE" });
  }

  /** Installed pixel map status. */
  async map(): Promise<{ installed: boolean; dims: number; count: number }> {
    return (await (await this.fetch("/api/map")).json()) as {
      installed: boolean;
      dims: number;
      count: number;
    };
  }

  /** Install a computed 2D/3D map so device patterns render with real geometry
   *  (render2D). `coords` are pattern-unit floats; sent as raw 16.16. */
  async setMap(dims: number, coords: number[][]): Promise<{ ok: boolean; count?: number }> {
    const parts: string[] = [String(dims)];
    for (const c of coords) {
      for (let d = 0; d < dims; d++) parts.push(String(Math.round((c[d] ?? 0) * RAW)));
    }
    const res = await this.fetch("/api/map", { method: "POST", body: parts.join(" ") });
    return (await res.json()) as { ok: boolean; count?: number };
  }

  /** Remove the installed map (patterns render 1D again). */
  async clearMap(): Promise<void> {
    await this.fetch("/api/map", { method: "POST", body: "" });
  }

  /** Which network the device will join next boot (never the password). */
  async wifi(): Promise<{ ssid: string | null; source: string }> {
    return (await (await this.fetch("/api/wifi")).json()) as {
      ssid: string | null;
      source: string;
    };
  }

  /** Set WiFi credentials — the device stores them and REBOOTS to apply. */
  async setWifi(ssid: string, password: string): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/wifi", {
      method: "POST",
      body: `${ssid}\n${password}`,
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** MQTT broker settings (never the password) + connection state. */
  async mqtt(): Promise<MqttStatus> {
    return (await (await this.fetch("/api/mqtt")).json()) as MqttStatus;
  }

  /** Set the MQTT broker; the device reconnects live (no reboot). Empty
   *  host disables MQTT. */
  async setMqtt(
    host: string,
    port: number,
    user: string,
    pass: string,
  ): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/mqtt", {
      method: "POST",
      body: `${host}\n${port || 1883}\n${user}\n${pass}`,
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /**
   * Output pipeline: wire color order, gamma (×10), power cap (mA), master-
   * dimmer curve (×10), blur %, glow %, and the device output palette
   * (flat `[pos,r,g,b,…]`, 0..=255 each, with its blend %). Everything after
   * `capMa` is absent on firmware older than the post-process chain.
   */
  async output(): Promise<{
    order: string;
    gamma: number;
    capMa: number;
    brightCurve?: number;
    blur?: number;
    glow?: number;
    palette?: number[];
    paletteAmount?: number;
  }> {
    return (await (await this.fetch("/api/output")).json()) as {
      order: string;
      gamma: number;
      capMa: number;
      brightCurve?: number;
      blur?: number;
      glow?: number;
      palette?: number[];
      paletteAmount?: number;
    };
  }

  /** Set the output pipeline; applied live + persisted. */
  async setOutput(
    order: string,
    gammaTenths: number,
    capMa: number,
    brightCurveTenths: number,
    blurPct: number,
    glowPct: number,
  ): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/output", {
      method: "POST",
      body:
        `${order} ${Math.round(gammaTenths)} ${Math.round(capMa)}` +
        ` ${Math.round(brightCurveTenths)} ${Math.round(blurPct)} ${Math.round(glowPct)}`,
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /**
   * Install the device output palette: `stops` is the flat `[pos,r,g,b,…]`
   * form (0..=255 each, positions ascending, at most 32 stops) and `amount`
   * is the blend percentage. Applied live + persisted in its own flash
   * record; it composes with a pattern's own `setOutputPalette`.
   */
  async setPalette(
    stops: readonly number[],
    amountPct: number,
  ): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/output/palette", {
      method: "POST",
      body: [Math.round(amountPct), ...stops.map((n) => Math.round(n))].join(" "),
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Clear the device output palette (record erased, stage off). */
  async clearPalette(): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/output/palette", { method: "DELETE" });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Wall clock: NTP sync status, local unix seconds, tz offset. */
  async clock(): Promise<{ synced: boolean; local: number; tzMinutes: number }> {
    return (await (await this.fetch("/api/clock")).json()) as {
      synced: boolean;
      local: number;
      tzMinutes: number;
    };
  }

  /** Set the UTC offset in minutes; applied live + persisted. */
  async setClock(tzMinutes: number): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/clock", {
      method: "POST",
      body: String(Math.round(tzMinutes)),
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Reboot the device into its provisioning access point (one boot). */
  async startApMode(): Promise<{ ok: boolean; note?: string }> {
    const res = await this.fetch("/api/apmode", { method: "POST", body: "" });
    return (await res.json()) as { ok: boolean; note?: string };
  }

  /** Luxel-to-Luxel sync role + clock + last leader beacon heard. */
  async sync(): Promise<SyncStatus> {
    return (await (await this.fetch("/api/sync")).json()) as SyncStatus;
  }

  /** Set the sync role; applied live and persisted on the device. */
  async setSync(mode: "off" | "leader" | "follower"): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/sync", { method: "POST", body: mode });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Stream one sensor-board frame (98-byte SB1.0 wire format) to the
   *  device — the browser mic standing in for the physical sensor board. */
  async sendSensors(frame: Uint8Array): Promise<void> {
    // cast: TS 5.7 types Uint8Array over ArrayBufferLike, which BodyInit
    // rejects; ours is a plain (non-shared) buffer
    await this.fetch("/api/sensors", { method: "POST", body: frame.buffer as ArrayBuffer });
  }

  /** Forward external events to the device (EV1 wire frame: "EV1\0" +
   *  u8 count + count × 4×i32-LE raw 16.16 [type, x, y, value]). The
   *  device queue caps at 32; extras here would only push older ones out. */
  async sendEvents(events: [number, number, number, number][]): Promise<void> {
    const evs = events.slice(0, 32);
    const buf = new Uint8Array(5 + evs.length * 16);
    buf.set([0x45, 0x56, 0x31, 0]); // "EV1\0"
    buf[4] = evs.length;
    const view = new DataView(buf.buffer);
    evs.forEach((ev, i) =>
      ev.forEach((v, j) => view.setInt32(5 + i * 16 + j * 4, Math.round(v * 65536) | 0, true)),
    );
    await this.fetch("/api/events", { method: "POST", body: buf.buffer as ArrayBuffer });
  }

  /** Compile + run a stored pattern on the device. */
  async activatePattern(id: string): Promise<RunResult> {
    const res = await this.fetch(`/api/patterns/${id}/activate`, { method: "POST" });
    return (await res.json()) as RunResult;
  }

  // ---- playlist ----

  /** The stored playlist + current playback state. */
  async playlist(): Promise<Playlist> {
    return (await (await this.fetch("/api/playlist")).json()) as Playlist;
  }

  /** Replace the stored playlist. `defaultSec` 0 = manual; per-item `sec` null
   *  inherits the default. Serialized to the firmware's line format. */
  async setPlaylist(pl: Playlist): Promise<void> {
    const lines: string[] = [
      `D ${Math.max(0, Math.round(pl.defaultSec))}`,
      `X ${Math.max(0, Math.round(pl.crossfadeMs))}`,
    ];
    for (const it of pl.items) {
      lines.push(`I ${it.id} ${it.sec === null ? -1 : Math.max(0, Math.round(it.sec))}`);
      for (const [name, vals] of Object.entries(it.controls)) {
        lines.push(`C ${name} ${vals.map((v) => Math.round(v * RAW)).join(" ")}`);
      }
    }
    await this.fetch("/api/playlist", { method: "POST", body: lines.join("\n") });
  }

  async playlistPlay(index = 0): Promise<void> {
    await this.fetch("/api/playlist/play", { method: "POST", body: String(index) });
  }
  async playlistStop(): Promise<void> {
    await this.fetch("/api/playlist/stop", { method: "POST" });
  }
  async playlistNext(): Promise<void> {
    await this.fetch("/api/playlist/next", { method: "POST" });
  }
  async playlistPrev(): Promise<void> {
    await this.fetch("/api/playlist/prev", { method: "POST" });
  }
}

export interface PlaylistItem {
  id: string;
  name: string;
  /** Per-item duration override in seconds; null = inherit the default. */
  sec: number | null;
  /** name → control values (floats). */
  controls: Record<string, number[]>;
  /**
   * Device pre-flight verdict: the pattern's assert() invariants fail
   * against the device's CURRENT config, so this entry would play black.
   * Absent = fine (or still being checked).
   */
  invalid?: string;
}

export interface Playlist {
  /** Default seconds per item; 0 = manual (no auto-advance). */
  defaultSec: number;
  /** Crossfade between items in ms; 0 = hard cut. */
  crossfadeMs: number;
  playing: boolean;
  index: number;
  items: PlaylistItem[];
}
