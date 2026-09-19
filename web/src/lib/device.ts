// Device-mode backend: the playground talking to real hardware (or the
// native mirror, `luxel serve`) over the device HTTP API instead of the
// local wasm engine. Raw 16.16 values cross the wire; this wrapper converts
// at the boundary, mirroring the wasm wrapper's conventions.

import { gatedFetch } from "./fetchgate";
import type { ProjectionMode } from "./geometry";

export interface DeviceStatus {
  /** Frames the pattern RENDERED in the last second. On a pipelined board
   *  (HUB75 panels) the render loop is not the wire, so this is not what the
   *  fixture showed — read `out_fps` there instead (Gitea #378). */
  fps: number;
  /** Frames the pipelined output stage actually DISPLAYED in the last second
   *  — the honest "frames the LEDs got" number on a HUB75 panel board, and 0
   *  on every board without a second-core output stage (a strip, the native
   *  mirror without `--out-fps`, firmware older than the field), where `fps`
   *  is the displayed rate. Since #394 it counts displayed frames rather than
   *  `write_frame` calls, so it can no longer exceed `rescan_hz`. */
  out_fps?: number;
  /** The panel's own refresh (rescan) rate in Hz — the ceiling `out_fps` is
   *  bounded by. 0/absent on any board without a HUB75 panel. */
  rescan_hz?: number;
  pixels: number;
  /** The device's hard pixel-count cap, which is PER BOARD (a 64x64 HUB75
   *  panel board reports 4096, strip boards 2048). Absent on firmware older
   *  than the field — fall back to GET /api/config's `max`, and only then to
   *  a built-in default. Never assume a constant: the whole point of the
   *  field is that the UI clamps to the connected board, not to 2048. */
  max_pixels?: number;
  /** The engine's EFFECTIVE geometry (Gitea #464) — what the device is
   *  actually rendering through, which is NOT the installed map whenever the
   *  engine supplied its own (the fabricated ceil(√n) grid reads
   *  `source:"default"`). Absent on firmware older than the field, where the
   *  UI falls back to `/api/map` + `pixels` (see `deviceLayout`).
   *  docs/api.md "geom". */
  geom?: {
    dims: number;
    regular: boolean;
    w: number;
    h: number;
    source: "user" | "board" | "default";
    /** What the RUNNING pattern wants (0 = no preference). */
    pattern_dims: number;
  };
  vmerr: string | null;
  /** Network input currently driving the strip (DDP/E1.31), if any. */
  live?: "ddp" | "e131" | null;
  /** Free heap in bytes, measured with the CURRENT pattern still loaded.
   *  0 or absent means "this device can't report it" (the native mirror
   *  without `--heap-free`, or firmware older than the field): treat as
   *  unknown and don't guess at capacity.
   *
   *  NOT by itself the headroom an incoming pattern has: the render task
   *  drops the outgoing engine BEFORE it decodes the incoming program, so a
   *  swap starts from `heap_free + engine_heap` (Gitea #287). */
  heap_free?: number;
  /** Largest SINGLE allocation the device heap can satisfy right now, in
   *  bytes. `heap_free` is a sum over the free list; this is the figure an
   *  upload or an engine swap actually has to fit in, and the gap between
   *  the two is fragmentation. Absent on the native mirror and on firmware
   *  older than Gitea #390, where a save can be refused for memory with
   *  `heap_free` still looking ample (a reboot clears it). */
  heap_largest?: number;
  /** Heap the currently loaded pattern's engine occupies, in bytes — the
   *  firmware measures it across each load (`shared::ENGINE_HEAP`). Add it to
   *  `heap_free` for the free heap a swap actually starts from
   *  (`luxel_core::budget::load_base`). 0 or absent on the native mirror
   *  without `--engine-heap` and on pre-#287 firmware, where the fallback is
   *  `heap_free` alone — conservative, never optimistic. */
  engine_heap?: number;
  /** Per-stage frame timing: average microseconds per rendered pattern frame
   *  over the last second. `frame_us` is the whole engine branch, `vm_us` the
   *  pattern evaluation, `pipe_us` the preview copy + output pipeline (gamma /
   *  palette / blur), `out_us` the LED/HUB75 driver write. Absent on firmware
   *  older than the fields and on the native mirror; the window averages only
   *  pattern frames, so all four read 0 while live input drives the strip. */
  frame_us?: number;
  vm_us?: number;
  pipe_us?: number;
  out_us?: number;
  /** Firmware version string (`0.1.44`) and the OTA slot it booted from
   *  (`ota_0`/`ota_1`; `"native"` on the mirror) — the Firmware & recovery
   *  disclosure's status line. Absent on firmware older than the fields. */
  version?: string;
  slot?: string;
  /** Pattern-store occupancy in bytes, and how many patterns are in it.
   *  Firmware only — the mirror keeps its store in memory and omits it. */
  store?: { used: number; total: number; dead: number; patterns: number };
  /** What this device can do (Gitea #464), derived by the firmware from board
   *  features × the current Layout. The UI shows a setting only when its
   *  capability is advertised — absent, never disabled (proposal §5.3/§5.7).
   *  Absent on firmware older than the field; see docs/api.md "caps". */
  caps?: DeviceCaps;
}

/** `/api/status`'s `caps` block (docs/api.md). Every field is advertised by
 *  both the firmware and the mirror, so they are not optional here — the
 *  whole block is, for firmware that predates it. */
export interface DeviceCaps {
  strip_driver: boolean;
  panel: boolean;
  outputs: number;
  power_cap: boolean;
  blur_glow: boolean;
  layers: number;
  text_slots: number;
  reboot: boolean;
  ota: boolean;
  psram: boolean;
  assets: boolean;
}

/** GET /api/map. */
export interface DeviceMapStatus {
  installed: boolean;
  dims: number;
  count: number;
  kind?: "grid" | "coords";
  w?: number;
  h?: number;
  proj1d?: string;
  proj2d?: string;
  proj3d?: string;
}

/** `GET /api/layout` (Gitea #465) — the ONE geometry object, with the old
 *  `/api/map` payload embedded so a client needs one fetch. See docs/api.md,
 *  "`/api/layout` — the one geometry object". */
export interface LayoutWire {
  kind: "strip" | "matrix" | "map";
  /** `regular` = the shape comes from the strip/matrix fields; `map` = from a
   *  map program's coordinates. "Custom" is a coordinate SOURCE, not a
   *  dimensionality (proposal §5.4d). */
  source: "regular" | "map";
  dims: number;
  regular: boolean;
  pixels: number;
  max: number;
  w: number;
  h: number;
  /** Present only when `kind` is `matrix`. */
  matrix?: {
    pw: number;
    ph: number;
    cols: number;
    rows: number;
    start: "tl" | "tr" | "bl" | "br";
    dir: "row" | "col";
    /** 0/1 on the wire, not a bool. */
    snake: number;
    rot180: number;
    /** HUB75 scan divisor; 0 = the board's own. */
    scan: number;
    /** The HOST's estimated rescan rate for this chain, Hz (Gitea #475).
     *  Absent on a host with no panel driver. The browser computes the same
     *  number from the same inputs (`lib/settingsCaps.ts`); this one wins,
     *  because the firmware knows its own clock and bit depth. */
    est_hz?: number;
    /** Leading tiles of the chain this board's framebuffer can shift out.
     *  `drive < cols·rows` means the rest of the arrangement is DARK — the
     *  DMA framebuffer is compile-time sized (Gitea #401). */
    drive?: number;
  };
  /** One entry per configured output. A host with nothing stored reports ONE
   *  implicit output built from its live data pin, protocol and colour order.
   *  `count` is pixels on a strip Layout and PANELS on a matrix one. */
  outputs: { n: number; pin: number; proto: string; order: string; count: number; rev: boolean }[];
  proj: { proj1d: string; proj2d: string; proj3d: string };
  /** The `GET /api/map` body verbatim. */
  map: DeviceMapStatus;
}

/** What `POST /api/layout` answers with: the GET body plus the verdict, so a
 *  client never has to re-fetch. A rejected body changes nothing and names
 *  the offending line (1-based; `0` = the body as a whole). */
export type LayoutResult =
  | ({ ok: true; reboot_required: boolean } & LayoutWire)
  | { ok: false; error: string; line?: number };

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
/** GET /api/config. The `data_pin*` fields exist only on strip-board
 *  firmware from 2026-09 on (Gitea #154). */
export interface DeviceConfig {
  pixels: number;
  max: number;
  protocol: string;
  /** GPIO the strip driver is bound to right now. */
  data_pin?: number;
  data_pin_default?: number;
  /** A stored pin that differs from `data_pin` — takes effect on reboot. */
  data_pin_next?: number | null;
  /** Every pin the picker accepts on this board. */
  data_pins?: number[];
}

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

  /** Device config: pixel count, its max, the LED protocol, and — on strip
   *  boards (Gitea #154) — the DATA pin: the one the driver is bound to,
   *  the board default, a stored value waiting for a reboot (else null),
   *  and every pin the picker accepts. Panel boards and older firmware omit
   *  the pin fields. */
  async config(): Promise<DeviceConfig> {
    return (await (await this.fetch("/api/config")).json()) as DeviceConfig;
  }

  /** Move the strip DATA line to `pin` (or back to the board default). The
   *  device persists it and REBOOTS to apply — the SPI driver binds its pin
   *  once, at boot. A rejected pin changes nothing and does not reboot. */
  async setDataPin(pin: number | "default"): Promise<{ ok: boolean; data_pin?: number; note?: string; error?: string }> {
    const res = await this.fetch("/api/datapin", { method: "POST", body: String(pin) });
    return (await res.json()) as { ok: boolean; data_pin?: number; note?: string; error?: string };
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

  /** The installed pixel map. A map stored in the procedural `grid W H` form
   *  also reports `kind: "grid"` with its `w`/`h` — the device's real matrix
   *  geometry. `kind` is absent on firmware older than the field and on an
   *  uninstalled map. The `proj*` triple is the device's projection defaults
   *  (docs/spec/projection.md §2); the mirror reports it today, firmware does
   *  not, and `/api/layout` (#465) takes both over. */
  async map(): Promise<DeviceMapStatus> {
    return (await (await this.fetch("/api/map")).json()) as DeviceMapStatus;
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

  /** Install a procedural row-major grid (`grid W H`): no per-pixel body, and
   *  zero heap on the device — the form a matrix/panel wants (Gitea #258). */
  async setGridMap(w: number, h: number): Promise<{ ok: boolean; count?: number }> {
    const res = await this.fetch("/api/map", { method: "POST", body: `grid ${w} ${h}` });
    return (await res.json()) as { ok: boolean; count?: number };
  }

  /** Remove the installed map (patterns render 1D again; a panel board
   *  falls back to its own grid). */
  async clearMap(): Promise<void> {
    await this.fetch("/api/map", { method: "POST", body: "" });
  }

  /** The whole Layout (Gitea #465): kind, dims, the matrix arrangement, the
   *  output table, the projection defaults and the installed map — one fetch
   *  for everything the LED layout section shows. */
  async layout(): Promise<LayoutWire> {
    return (await (await this.fetch("/api/layout")).json()) as LayoutWire;
  }

  /**
   * Change the Layout. `lines` is the line-oriented body (`strip 300`,
   * `matrix …`, `map grid W H`, `out n pin proto order count [rev]`,
   * `proj1d x`) — ONE POST per user action, and the reply IS the new state,
   * so a caller never re-GETs. A rejected body changes nothing.
   */
  async setLayout(lines: string): Promise<LayoutResult> {
    const res = await this.fetch("/api/layout", { method: "POST", body: lines });
    return (await res.json()) as LayoutResult;
  }

  /** Reboot the device (`caps.reboot`). The other half of
   *  `reboot_required`: a stored chain arrangement or output table has no
   *  other way to be applied, and neither `/api/wifi` nor `/api/datapin`
   *  exists on a HUB75 board (Gitea #475). Firmware only. */
  async reboot(): Promise<{ ok: boolean; note?: string }> {
    const res = await this.fetch("/api/reboot", { method: "POST", body: "" });
    return (await res.json()) as { ok: boolean; note?: string };
  }

  /** Stream a firmware image to the inactive OTA slot; the device reboots
   *  into it on success (`caps.ota`). Firmware only. */
  async otaUpload(image: ArrayBuffer): Promise<{ ok: boolean; bytes?: number; error?: string }> {
    const res = await this.fetch("/api/ota", { method: "POST", body: image });
    return (await res.json()) as { ok: boolean; bytes?: number; error?: string };
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
   *  inherits the default. Serialized to the firmware's line format — each
   *  item's `C` (values) and `P` (projection override) lines follow its `I`,
   *  and both are omitted when there is nothing to say, so what a
   *  pre-#470 device stored round-trips byte-identically. */
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
      if (it.proj !== undefined) lines.push(`P ${it.proj}`);
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
  /** What the row plays. `pattern` is the only kind the wire carries today;
   *  scene items are Phase B (#478/#481), so an absent `kind` reads as
   *  `pattern` and the row model is already shaped for the second one. */
  kind?: "pattern" | "scene";
  /** Per-item duration override in seconds; null = inherit the default. */
  sec: number | null;
  /** name → control values (floats). */
  controls: Record<string, number[]>;
  /** Projection override for THIS item (§5.4d, Gitea #470): how its pattern
   *  is shown on the device's Layout. Absent = the device's own default.
   *  The token lands in the slot matching the pattern's own dimensionality,
   *  so one value survives whatever the pattern turns out to be. */
  proj?: ProjectionMode;
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
