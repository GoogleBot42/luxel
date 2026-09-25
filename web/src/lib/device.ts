// Device-mode backend: the playground talking to real hardware (or the
// native mirror, `luxel serve`) over the device HTTP API instead of the
// local wasm engine. Raw 16.16 values cross the wire; this wrapper converts
// at the boundary, mirroring the wasm wrapper's conventions.

import { gatedFetch, type GateOptions } from "./fetchgate";
import type { ProjectionMode } from "./geometry";
import { normalizePlaylist, playlistWire } from "./playlist";

export interface DeviceStatus {
  /** The device's own mDNS-style name (`luxel-f6b0a8`) — what the console
   *  calls it in the title bar. Absent on firmware older than the field and
   *  on the native mirror started without `--name`; the UI falls back to the
   *  host it answers on (`stores/device.ts` `deviceLabel`). */
  name?: string;
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
   *
   *  It is therefore NOT the fixture, and it is not a Layout: a strip handed a
   *  `render2D` program still has the geometry of a strip. Only
   *  `lib/geometry.ts`'s `deviceGeometry` may read it, and it takes only the
   *  `source` `user`/`board` readings (Gitea #573).
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
  /** What the LIVE pattern is running as, and why (Gitea #658,
   *  docs/jit-design.md §4a). `state` is `"native"` when the device
   *  compiled it, `"interp"` when it has a JIT and refused, `"none"` when
   *  it has a backend but nothing is resident (a board nobody has given a
   *  pattern — the state #744 made reachable by removing the built-in
   *  default), `"off"` when the board carries no backend. `reason` is the
   *  shared refusal vocabulary — the same spellings `jitlint` uses for the
   *  compile-time half, plus the three only a device can know (`debug`,
   *  `init-error`, `no-buffer`) and `disabled` for the `POST /api/jit`
   *  switch. Absent only from firmware older than #658.
   *
   *  The five scalar fields describe the ONE running program. They used to
   *  be the whole story and were wrong for a scene: every layer's compile
   *  overwrote them, so a board rendering natively could report `interp`
   *  because the last layer to compile happened to refuse (Gitea #718).
   *  They now describe the primary engine only, and `layers` describes the
   *  stack. */
  jit?: {
    state: "native" | "interp" | "none" | "off";
    reason: string | null;
    /** Bytes of native code, literal pool included. 0 when not native. */
    code_bytes: number;
    /** What compiling it cost. 0 when not native. */
    compile_us: number;
    /** Resident engines compiled / interpreting, counted over `layers`.
     *  Their sum is `engines` unless the stack is deeper than eight layers.
     *  Absent from firmware older than #718 and from the native mirror. */
    native?: number;
    interp?: number;
    /** One entry per RESIDENT ENGINE, bottom → top. Sparse in `layer`: a
     *  text or colour layer, and a pattern layer that did not fit, have no
     *  entry. `[]` when `state` is `"none"` or `"off"`. */
    layers?: {
      /** 0-based index into the scene's own `layers` array; a bare pattern
       *  is layer 0. (`scene: layer N does not fit` is 1-based.) */
      layer: number;
      /** `"sprite"` is an engine built and read but never stepped — its
       *  compile is real, its native code never runs (Gitea #740). */
      kind: "pattern" | "sprite";
      state: "native" | "interp" | "off";
      reason: string | null;
      code_bytes: number;
    }[];
  };
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
  /** The external pattern-array arena (Gitea #253), in bytes — a SECOND heap
   *  that is not part of `heap_free`, so a big `array()` costs the engine
   *  heap nothing. Both absent on every board without one, which is what
   *  `caps.psram` says; the Settings page shows the numbers rather than the
   *  word `present` (Jeremy, 2026-09-20). */
  psram_free?: number;
  psram_total?: number;
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
  /** This image's `board::NAME` (`"Pixelblaze v3 Standard"`) — the same
   *  string a `.luxr` release package names and `tools/ota-push.sh` greps an
   *  image for (#389), so the console can refuse a package built for another
   *  board before it reaches an OTA slot. Absent on firmware older than the
   *  field; the mirror reports `"native mirror"` unless `--board-name`
   *  impersonates one. */
  board?: string;
  /** The LXBC format version this build READS. Compare it with the bundle's
   *  own (`Luxel.bcFormat()`): a device reading a newer format cannot run
   *  anything this console compiles, and one reading an older format cannot
   *  run what its store already holds. Absent on firmware older than the
   *  field, which means "unknown" — never assume they agree (Gitea #643). */
  bc_format?: number;
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

/** One row of `GET /api/patterns`. */
export interface DevicePatternRow {
  id: string;
  name: string;
  /** The device cannot decode this pattern's stored blob — see
   *  `DeviceSession.patterns`. */
  stale?: boolean;
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
  private fetch(path: string, init?: RequestInit, opts?: GateOptions): Promise<Response> {
    return gatedFetch(this.url(path), init, opts);
  }

  /**
   * `/api/status`, on the FAST path: no retry ladder and a 4 s deadline.
   *
   * It is the app's liveness probe as well as its readout — the 1 Hz poll is
   * what tells the user the board went away (Gitea #538 round 2) — so it has
   * to report a dead device in about a second rather than spending the read
   * path's ~30 s of patience in silence. `force` because this is the ONE
   * request that has to keep going out while the gate says the device is
   * down: it is the probe that clears the latch.
   */
  async status(): Promise<DeviceStatus> {
    const res = await this.fetch("/api/status", undefined, { fastFail: true, force: true });
    // An ERROR STATUS IS NOT A STATUS. `fetchgate` throws only on transport
    // failures, so without this a 503 `{"ok":false,"error":"out of memory"}`
    // deserializes into a `DeviceStatus` whose every field is `undefined` —
    // and the poll would then write `caps: null` and `geom: null` into the
    // stores, tearing a healthy console down to a playground (no Settings
    // tab, every tile reshaped) for a board that is merely under memory
    // pressure. That is the same invariant leak as #539 and #573, arriving
    // through a third door. Throwing puts it on the poll's own catch path,
    // where "we did not learn anything this second" already means "keep the
    // last reading". `/api/status` can answer 503 whenever the heap cannot
    // spare a 256 B body segment (Gitea #753).
    if (!res.ok) throw new Error(`status: HTTP ${res.status}`);
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

  /**
   * The RUNNING pattern's projection override (Gitea #598,
   * docs/spec/projection.md §2) — the `proj` line on `/api/layout`. `null`
   * clears it back to the device's own `proj1d/2d/3d` defaults.
   *
   * It is deliberately LIVE ONLY: the device stores nothing, so the next
   * activation, playlist item or `/api/code` push starts from the defaults
   * again. That matches what the override IS on this side — a property of
   * the editor's working copy, not of the rig — which is also why it rides
   * on the endpoint that owns the projection rather than getting a route of
   * its own (the tightest board in the fleet has ~200 B of OTA slot to
   * spare; docs/boards.md). The console re-posts it after every live code
   * push (`Editor.devicePush`).
   *
   * False when the device rejected it — firmware older than #598 answers
   * `{"ok":false,…,"line":1}` for the unknown line, so a caller can stay
   * quiet rather than raising a device error.
   */
  async setProjection(mode: string | null): Promise<boolean> {
    return (await this.setLayout(`proj ${mode ?? "default"}`)).ok === true;
  }

  // ---- device pattern library (see serve.rs / server.rs contract) ----

  /** The stored library. `stale` marks a pattern whose compiled blob the
   *  device can no longer read — its source is intact, so a console with a
   *  current compiler recompiles and re-saves it by name (Gitea #643).
   *  Absent on firmware older than the field, which means "not known to be
   *  stale", never "known to be fine". */
  async patterns(): Promise<DevicePatternRow[]> {
    const r = (await (await this.fetch("/api/patterns")).json()) as {
      patterns?: DevicePatternRow[];
    };
    return r.patterns ?? [];
  }

  /** Stream the packed web app (a LUXA/LUX2 archive) into the assets
   *  partition. No reboot — the device serves the new bundle immediately,
   *  so the caller reloads the page afterwards. Firmware only; the mirror
   *  answers this under `--accept-ota`. */
  async assetsUpload(archive: ArrayBuffer): Promise<{ ok: boolean; bytes?: number; error?: string }> {
    const res = await this.fetch("/api/assets", { method: "POST", body: archive });
    return (await res.json()) as { ok: boolean; bytes?: number; error?: string };
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
   *  `reboot_required`: a stored chain arrangement, a moved data pad or an
   *  output that has no driver instance yet has no other way to be applied
   *  (#475/#550), and neither `/api/wifi` nor `/api/datapin` exists on a
   *  HUB75 board. Firmware only. */
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

  /** What this device calls itself (Gitea #538) — `luxel-<mac6>` until
   *  somebody renames it. `/api/status` carries the same string, so the
   *  console only reads this route when Settings wants `source`. */
  async name(): Promise<{ name: string; source: "stored" | "default" }> {
    return (await (await this.fetch("/api/name")).json()) as {
      name: string;
      source: "stored" | "default";
    };
  }

  /** Rename the device. 1..=32 printable bytes, no `"` or `\`; an EMPTY
   *  string clears the name back to the board default. The name is live in
   *  `/api/status` at once, but the DHCP hostname is built at boot — hence
   *  `reboot_required` on every accepted POST. */
  async setName(name: string): Promise<{
    ok: boolean;
    name?: string;
    source?: "stored" | "default";
    reboot_required?: boolean;
    error?: string;
  }> {
    const res = await this.fetch("/api/name", { method: "POST", body: name });
    return (await res.json()) as {
      ok: boolean;
      name?: string;
      source?: "stored" | "default";
      reboot_required?: boolean;
      error?: string;
    };
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

  // ---- text slots (Gitea #485, docs/api.md `/api/text`) ----

  /** The eight host-settable text values a `textSlot(n)` reads. */
  async text(): Promise<string[]> {
    const r = (await (await this.fetch("/api/text")).json()) as { slots?: string[] };
    return r.slots ?? [];
  }

  /** `POST /api/text` body `<slot> <utf8…>` — one line, the text is the rest
   *  of it, ≤ 64 B truncated on a char boundary by the device. Empty clears. */
  async setText(slot: number, text: string): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch("/api/text", {
      method: "POST",
      body: `${Math.round(slot)} ${text}`,
    });
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

  /** Ask the device to re-sync its clock NOW (Gitea #538). Asynchronous on
   *  firmware — it wakes the SNTP task, which otherwise sleeps out a 6 h
   *  period — so the reply is the clock as it stands at that instant and
   *  `synced` can still be the PREVIOUS state. Poll `clock()` for the
   *  result. */
  async syncClock(): Promise<{ ok: boolean; synced?: boolean; local?: number }> {
    const res = await this.fetch("/api/clock/sync", { method: "POST", body: "" });
    return (await res.json()) as { ok: boolean; synced?: boolean; local?: number };
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

  // ---- PLAYLIST (the block Gitea #478 extends with scene items) ----

  /** The stored playlist + current playback state. Normalized so a scene item
   *  (which carries no `controls`) and a pre-#478 device (which sends no
   *  `kind`) read the same as everything else — `lib/playlist.ts`. */
  async playlist(): Promise<Playlist> {
    return normalizePlaylist((await (await this.fetch("/api/playlist")).json()) as Playlist);
  }

  /** Replace the stored playlist. `defaultSec` 0 = manual; per-item `sec` null
   *  inherits the default. Serialized by `playlistWire` (`lib/playlist.ts`) to
   *  the firmware's line format — a pattern item's `C` (values) and `P`
   *  (projection override) lines follow its `I`, both omitted when there is
   *  nothing to say so a pre-#470 device's playlist round-trips
   *  byte-identically, and a scene item is `I S<id> <sec>` with neither. */
  async setPlaylist(pl: Playlist): Promise<void> {
    await this.fetch("/api/playlist", { method: "POST", body: playlistWire(pl) });
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

  // ---- scenes (Gitea #480; routes in docs/api.md, record in
  //      docs/spec/scenes.md) -------------------------------------------
  // WEB-A owns this block. Everything crosses as the SCENE WIRE BLOCK
  // (`serializeScene`), never JSON: the device's parser is the one in
  // `luxel_core::scene`, and its refusals come back as the
  // `scene: line N: …` / `scenes: store full (…)` strings `lib/apiErrors.ts`
  // translates. A POST with `S -` means "assign an id" and the reply names
  // the one it gave.

  /** Every stored scene plus the blob budget and which one is running. */
  async scenes(): Promise<ScenesWire> {
    // Same rule as `status()`: a 503 body is not an empty scene library, and
    // letting it parse as one would empty the Scenes tab on screen for a
    // board that is only briefly out of heap (Gitea #753).
    const res = await this.fetch("/api/scenes");
    if (!res.ok) throw new Error(`scenes: HTTP ${res.status}`);
    return (await res.json()) as ScenesWire;
  }

  /** One scene. */
  async scene(id: string): Promise<SceneWire> {
    return (await (await this.fetch(`/api/scenes/${id}`)).json()) as SceneWire;
  }

  /** Create (`id` empty → the body must start `S -`) or replace a scene.
   *  Replacing the ACTIVE scene live-applies it on the device. */
  async saveScene(id: string, block: string): Promise<SceneSaveResult> {
    const res = await this.fetch(id === "" ? "/api/scenes" : `/api/scenes/${id}`, {
      method: "POST",
      body: block,
    });
    return (await res.json()) as SceneSaveResult;
  }

  async deleteScene(id: string): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch(`/api/scenes/${id}`, { method: "DELETE" });
    return (await res.json()) as { ok: boolean; error?: string };
  }

  /** Play a scene — parks the playlist exactly as playing a pattern does. */
  async activateScene(id: string, crossfadeMs?: number): Promise<{ ok: boolean; error?: string }> {
    const res = await this.fetch(`/api/scenes/${id}/activate`, {
      method: "POST",
      body: crossfadeMs === undefined ? "" : String(Math.max(0, Math.round(crossfadeMs))),
    });
    return (await res.json()) as { ok: boolean; error?: string };
  }
  // ---- end scenes ---------------------------------------------------
}

/** `GET /api/scenes/<id>` — the JSON shape of `scene::push_json`. Typed in
 *  `lib/scene.ts` (`SceneJson`); re-declared loosely here so `lib/device.ts`
 *  does not depend on the codec. */
export type SceneWire = Record<string, unknown>;

/** `GET /api/scenes`. `used`/`max` are the shared blob's bytes. */
export interface ScenesWire {
  active: string | null;
  layers_max: number;
  used: number;
  max: number;
  scenes: SceneWire[];
}

export type SceneSaveResult = { ok: true; id: string } | { ok: false; error: string };

export interface PlaylistItem {
  id: string;
  name: string;
  /** What the row plays — a stored pattern, or a SCENE (a stack of layers,
   *  Gitea #478). The wire spells a scene item `I S<id> <sec>`; a pre-#478
   *  device sends no `kind` at all, and `playlist()` normalizes that to
   *  `pattern` so nothing downstream has to guard. */
  kind?: "pattern" | "scene";
  /** Layers in the scene this item names — the row's `Scene ▤ · 2 layers`
   *  type line (mock S4c). Scene items only. */
  layers?: number;
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
