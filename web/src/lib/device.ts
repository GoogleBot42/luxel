// Device-mode backend: the playground talking to real hardware (or the
// native mirror, `luxel serve`) over the device HTTP API instead of the
// local wasm engine. Raw 16.16 values cross the wire; this wrapper converts
// at the boundary, mirroring the wasm wrapper's conventions.

export interface DeviceStatus {
  fps: number;
  pixels: number;
  vmerr: string | null;
  /** Network input currently driving the strip (DDP/E1.31), if any. */
  live?: "ddp" | "e131" | null;
}

export type RunResult = { ok: true } | { ok: false; line: number; col: number; error: string };

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

  async status(): Promise<DeviceStatus> {
    const res = await fetch(this.url("/api/status"));
    return (await res.json()) as DeviceStatus;
  }

  async pattern(): Promise<string> {
    return (await fetch(this.url("/api/pattern"))).text();
  }

  /** Current output brightness (0–31) and its max. */
  async brightness(): Promise<{ brightness: number; max: number }> {
    return (await (await fetch(this.url("/api/brightness"))).json()) as {
      brightness: number;
      max: number;
    };
  }

  /** Set output brightness (0–31); applied live and persisted on the device. */
  async setBrightness(value: number): Promise<{ ok: boolean; brightness?: number }> {
    const res = await fetch(this.url("/api/brightness"), {
      method: "POST",
      body: String(Math.max(0, Math.min(31, Math.round(value)))),
    });
    return (await res.json()) as { ok: boolean; brightness?: number };
  }

  /** Device config: pixel count, its max, and the LED protocol. */
  async config(): Promise<{ pixels: number; max: number; protocol: string }> {
    return (await (await fetch(this.url("/api/config"))).json()) as {
      pixels: number;
      max: number;
      protocol: string;
    };
  }

  /** Set the pixel count; the device resizes its strip live (no reboot). */
  async setConfig(pixels: number): Promise<{ ok: boolean; pixels?: number; error?: string }> {
    const res = await fetch(this.url("/api/config"), {
      method: "POST",
      body: String(Math.max(1, Math.round(pixels))),
    });
    return (await res.json()) as { ok: boolean; pixels?: number; error?: string };
  }

  /** Current LED protocol and the selectable options. */
  async protocol(): Promise<{ protocol: string; options: string[] }> {
    return (await (await fetch(this.url("/api/protocol"))).json()) as {
      protocol: string;
      options: string[];
    };
  }

  /** Set the LED protocol; the device reconfigures its driver live (no reboot). */
  async setProtocol(name: string): Promise<{ ok: boolean; protocol?: string; error?: string }> {
    const res = await fetch(this.url("/api/protocol"), { method: "POST", body: name });
    return (await res.json()) as { ok: boolean; protocol?: string; error?: string };
  }

  async run(source: string): Promise<RunResult> {
    const res = await fetch(this.url("/api/code"), { method: "POST", body: source });
    return (await res.json()) as RunResult;
  }

  async setControl(name: string, values: number[]): Promise<void> {
    const body = `${name} ${values.map((v) => Math.round(v * RAW)).join(" ")}`.trim();
    await fetch(this.url("/api/control"), { method: "POST", body });
  }

  // ---- device pattern library (see serve.rs / server.rs contract) ----

  async patterns(): Promise<{ id: string; name: string }[]> {
    const r = (await (await fetch(this.url("/api/patterns"))).json()) as {
      patterns?: { id: string; name: string }[];
    };
    return r.patterns ?? [];
  }

  async patternSource(id: string): Promise<{ id: string; name: string; source: string }> {
    return (await (await fetch(this.url(`/api/patterns/${id}`))).json()) as {
      id: string;
      name: string;
      source: string;
    };
  }

  /** Save (same name overwrites). Body is "name\nsource" — text, so the
   *  firmware needs no JSON parser. */
  async savePattern(name: string, source: string): Promise<RunResult & { id?: string }> {
    const res = await fetch(this.url("/api/patterns"), {
      method: "POST",
      body: `${name}\n${source}`,
    });
    return (await res.json()) as RunResult & { id?: string };
  }

  async deletePattern(id: string): Promise<void> {
    await fetch(this.url(`/api/patterns/${id}`), { method: "DELETE" });
  }

  /** Installed pixel map status. */
  async map(): Promise<{ installed: boolean; dims: number; count: number }> {
    return (await (await fetch(this.url("/api/map"))).json()) as {
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
    const res = await fetch(this.url("/api/map"), { method: "POST", body: parts.join(" ") });
    return (await res.json()) as { ok: boolean; count?: number };
  }

  /** Remove the installed map (patterns render 1D again). */
  async clearMap(): Promise<void> {
    await fetch(this.url("/api/map"), { method: "POST", body: "" });
  }

  /** Which network the device will join next boot (never the password). */
  async wifi(): Promise<{ ssid: string | null; source: string }> {
    return (await (await fetch(this.url("/api/wifi"))).json()) as {
      ssid: string | null;
      source: string;
    };
  }

  /** Set WiFi credentials — the device stores them and REBOOTS to apply. */
  async setWifi(ssid: string, password: string): Promise<{ ok: boolean; error?: string }> {
    const res = await fetch(this.url("/api/wifi"), {
      method: "POST",
      body: `${ssid}\n${password}`,
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** MQTT broker settings (never the password) + connection state. */
  async mqtt(): Promise<MqttStatus> {
    return (await (await fetch(this.url("/api/mqtt"))).json()) as MqttStatus;
  }

  /** Set the MQTT broker; the device reconnects live (no reboot). Empty
   *  host disables MQTT. */
  async setMqtt(
    host: string,
    port: number,
    user: string,
    pass: string,
  ): Promise<{ ok: boolean; error?: string }> {
    const res = await fetch(this.url("/api/mqtt"), {
      method: "POST",
      body: `${host}\n${port || 1883}\n${user}\n${pass}`,
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Output pipeline: wire color order, gamma (×10), power cap (mA). */
  async output(): Promise<{ order: string; gamma: number; capMa: number }> {
    return (await (await fetch(this.url("/api/output"))).json()) as {
      order: string;
      gamma: number;
      capMa: number;
    };
  }

  /** Set the output pipeline; applied live + persisted. */
  async setOutput(
    order: string,
    gammaTenths: number,
    capMa: number,
  ): Promise<{ ok: boolean; error?: string }> {
    const res = await fetch(this.url("/api/output"), {
      method: "POST",
      body: `${order} ${Math.round(gammaTenths)} ${Math.round(capMa)}`,
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Wall clock: NTP sync status, local unix seconds, tz offset. */
  async clock(): Promise<{ synced: boolean; local: number; tzMinutes: number }> {
    return (await (await fetch(this.url("/api/clock"))).json()) as {
      synced: boolean;
      local: number;
      tzMinutes: number;
    };
  }

  /** Set the UTC offset in minutes; applied live + persisted. */
  async setClock(tzMinutes: number): Promise<{ ok: boolean; error?: string }> {
    const res = await fetch(this.url("/api/clock"), {
      method: "POST",
      body: String(Math.round(tzMinutes)),
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Reboot the device into its provisioning access point (one boot). */
  async startApMode(): Promise<{ ok: boolean; note?: string }> {
    const res = await fetch(this.url("/api/apmode"), { method: "POST", body: "" });
    return (await res.json()) as { ok: boolean; note?: string };
  }

  /** Luxel-to-Luxel sync role + clock + last leader beacon heard. */
  async sync(): Promise<SyncStatus> {
    return (await (await fetch(this.url("/api/sync"))).json()) as SyncStatus;
  }

  /** Set the sync role; applied live and persisted on the device. */
  async setSync(mode: "off" | "leader" | "follower"): Promise<{ ok: boolean; error?: string }> {
    const res = await fetch(this.url("/api/sync"), { method: "POST", body: mode });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Stream one sensor-board frame (98-byte SB1.0 wire format) to the
   *  device — the browser mic standing in for the physical sensor board. */
  async sendSensors(frame: Uint8Array): Promise<void> {
    // cast: TS 5.7 types Uint8Array over ArrayBufferLike, which BodyInit
    // rejects; ours is a plain (non-shared) buffer
    await fetch(this.url("/api/sensors"), { method: "POST", body: frame.buffer as ArrayBuffer });
  }

  /** Compile + run a stored pattern on the device. */
  async activatePattern(id: string): Promise<RunResult> {
    const res = await fetch(this.url(`/api/patterns/${id}/activate`), { method: "POST" });
    return (await res.json()) as RunResult;
  }

  // ---- playlist ----

  /** The stored playlist + current playback state. */
  async playlist(): Promise<Playlist> {
    return (await (await fetch(this.url("/api/playlist"))).json()) as Playlist;
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
    await fetch(this.url("/api/playlist"), { method: "POST", body: lines.join("\n") });
  }

  async playlistPlay(index = 0): Promise<void> {
    await fetch(this.url("/api/playlist/play"), { method: "POST", body: String(index) });
  }
  async playlistStop(): Promise<void> {
    await fetch(this.url("/api/playlist/stop"), { method: "POST" });
  }
  async playlistNext(): Promise<void> {
    await fetch(this.url("/api/playlist/next"), { method: "POST" });
  }
  async playlistPrev(): Promise<void> {
    await fetch(this.url("/api/playlist/prev"), { method: "POST" });
  }
}

export interface PlaylistItem {
  id: string;
  name: string;
  /** Per-item duration override in seconds; null = inherit the default. */
  sec: number | null;
  /** name → control values (floats). */
  controls: Record<string, number[]>;
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
