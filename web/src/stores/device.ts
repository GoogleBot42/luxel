// The device session: everything the console knows about the hardware it is
// bound to, plus the ONE poll scheduler that keeps it fresh.
//
// Layering rule (Gitea #462): this module may not import `geometry.ts` or
// `pattern.ts`. The connect handshake therefore *returns* the pattern source
// it pulled instead of writing it — the Editor page owns the document. That
// keeps the dependency chain one-way: device ← geometry ← pattern.

import { derived, get, writable, type Readable, type Writable } from "svelte/store";
import {
  DeviceSession,
  type DeviceCaps,
  type DeviceStatus,
  type LayoutResult,
  type LayoutWire,
  type MqttStatus,
  type Playlist,
  type PlaylistItem,
  type SyncStatus,
} from "../lib/device";
import {
  DEFAULT_PROJECTION,
  normDims,
  PROJECTION_CODES,
  type DeviceGeom,
  type Projection,
  type ProjectionMode,
} from "../lib/geometry";
import { gatedFetch } from "../lib/fetchgate";
import { browserBlocked } from "../lib/lna";
import { reconcileTransport, transportIntent, type TransportIntent } from "../lib/playlist";

// ---- session ----

/** The live session, or null when nothing is connected. */
export const device: Writable<DeviceSession | null> = writable(null);

/** The device this app is bound to, if any. `""` = served from the device
 *  itself (same origin); a URL = a `?device=` dev/e2e override; `null` = a
 *  plain playground (no hardware — no connection UI at all). Set once we
 *  know we're on a device; survives disconnect so reconnect needs no URL. */
export const deviceBase: Writable<string | null> = writable(null);

/** No device involved — a hosted/standalone playground. Devices are only
 *  reached by loading the UI *from* a device (or a `?device=` override), so
 *  the playground never shows connect/share-to-device affordances. */
export const isPlayground: Readable<boolean> = derived(deviceBase, (b) => b === null);

/** "device" whenever we're bound to hardware (even while disconnected),
 *  "playground" otherwise — drives which affordances the header shows. */
export const mode: Readable<string> = derived(isPlayground, (p) => (p ? "playground" : "device"));

export const deviceError = writable("");
/** The connect failed because the BROWSER refused to make the request, not
 *  because the device is missing: an https-hosted copy of this app (the
 *  Pages build, opened from the firmware's fallback page as
 *  `?device=http://…`) reaching a plain-http LAN device is mixed content
 *  gated behind Chromium's Local Network Access permission. Nothing the
 *  app can retry its way out of, so it gets its own message and the manual
 *  routes around it rather than a generic "cannot reach device" (#162). */
export const deviceBlocked = writable(false);

// ---- hardware facts ----

/** The device's fixed hardware pixel count — the source of truth for the
 *  pixel total in device mode (layout changes only rearrange the preview,
 *  they never change how many pixels the device drives). */
export const devicePixels = writable(0);
/** Max pixel count the device firmware accepts. Per BOARD, not a constant —
 *  a 64x64 HUB75 panel board caps at 4096, strip boards at 2048 (Gitea #74). */
export const pixelMax = writable(2048);
/** Free heap the device last reported, in bytes. 0 = it can't tell us. */
export const deviceHeapFree = writable(0);
/** Heap the device's CURRENTLY loaded pattern occupies (0 pre-#287). */
export const deviceEngineHeap = writable(0);
/** The device's own post-push verdict (`/api/status` vmerr), if any. */
export const deviceVmerr = writable<string | null>(null);
/** The connected device's own frame rate, from `/api/status` (Gitea #381).
 *  `deviceFps` is what the pattern RENDERED; `deviceOutFps` is what a
 *  pipelined HUB75 board actually DISPLAYED (0 on a strip); `deviceRescanHz`
 *  is the panel's refresh ceiling. All three are 0 until the first poll. */
export const deviceFps = writable(0);
export const deviceOutFps = writable(0);
export const deviceRescanHz = writable(0);

/** Whether a pixel map is installed on the device (render2D geometry), and
 *  — for one stored in the procedural `grid W H` form — its shape, which is
 *  the rig a 2D pattern is previewed on while connected (#372). */
export interface DeviceMap {
  installed: boolean;
  dims: number;
  count: number;
  kind?: "grid" | "coords";
  w?: number;
  h?: number;
}
export const deviceMap = writable<DeviceMap>({ installed: false, dims: 0, count: 0 });

/** `/api/status`'s `geom` (Gitea #464): the engine's EFFECTIVE geometry, as
 *  reported. Raw wire state — read it through `deviceLayout`, never directly. */
export const deviceGeomStatus = writable<DeviceStatus["geom"] | null>(null);

/** `/api/status`'s `caps` (Gitea #464): what this device can do. Null until a
 *  device answers, and on firmware older than the field — a consumer that
 *  can't tell falls back to the conservative reading, never to a board name
 *  (proposal §5.3). A8 (#469) gates the whole Settings page on it; the editor
 *  reads only `panel`, to know which per-pixel current model the console
 *  preview's output chain should use. */
export const deviceCaps = writable<DeviceCaps | null>(null);

/** `GET /api/layout` (Gitea #465): the whole geometry object as the device
 *  reports it. Raw wire state — the Settings page's LED layout section is its
 *  only direct reader; everything that renders pixels goes through
 *  `deviceLayout` below. Null until a device answers (and on firmware older
 *  than the endpoint). */
export const deviceLayoutWire = writable<LayoutWire | null>(null);

/** Firmware version / OTA slot / pattern-store occupancy, from `/api/status`
 *  — the Advanced section's Storage and Firmware rows. */
export const deviceVersion = writable("");
export const deviceSlot = writable("");
export const deviceStore = writable<DeviceStatus["store"] | null>(null);

/** The coordinates of an irregular device map, when they are known: a map
 *  THIS session installed. `GET /api/map` reports only a count, so a map
 *  installed before this page loaded has none until `/api/layout` (#465)
 *  starts embedding it. */
export const deviceMapCoords = writable<number[][] | null>(null);

/** The device's projection defaults (docs/spec/projection.md §2). The mirror
 *  carries them on `/api/map` today; firmware gets them with `/api/layout`. */
export const deviceProjection = writable<Projection>(DEFAULT_PROJECTION);

function asMode(v: string | undefined, fallback: ProjectionMode): ProjectionMode {
  return v !== undefined && v in PROJECTION_CODES ? (v as ProjectionMode) : fallback;
}

/**
 * THE DEVICE-LAYOUT ADAPTER (Gitea #463, #469).
 *
 * Everything the UI knows about the device's geometry, in the ONE shape
 * `stores/geometry.ts` reconciles against. Since A8 it reads `/api/layout`
 * (#465) **wholesale** — kind, dims, w/h, the chain's wiring and the
 * embedded map all arrive in one fetch — and not a single consumer changed
 * when it did.
 *
 * Two fallbacks stay, in order:
 *
 *  1. `/api/status`'s `geom` (#464) — what answers during the connect
 *     handshake, before `/api/layout` has been read, and the ONLY reporter
 *     of the engine's fabricated ceil(√n) grid (`source:"default"`): a
 *     `render2D`-only pattern on a strip board renders through a grid the
 *     Layout does not describe, and the console preview must show it. So
 *     `geom` WINS over the Layout in exactly that case.
 *  2. `deviceMap` — firmware older than either field, where a procedural
 *     `grid W H` is still a matrix and a coords map still a cloud.
 */
export const deviceLayout: Readable<DeviceGeom | null> = derived(
  [deviceLayoutWire, deviceGeomStatus, devicePixels, deviceMap, deviceMapCoords],
  ([wire, g, pixels, dm, coords]) => {
    // the engine's own fabricated grid is not in the Layout; `geom` owns it
    const fabricated = g?.source === "default";
    if (wire && !fabricated) {
      const px = wire.pixels || pixels;
      const m = wire.matrix;
      return {
        dims: normDims(wire.dims),
        regular: wire.regular,
        w: wire.w,
        h: wire.h,
        source: wire.source === "map" ? "user" : "board",
        pixels: px || wire.w * wire.h,
        coords: wire.regular ? undefined : (coords ?? undefined),
        // the device's REAL wiring, at last: a snaked matrix walks alternate
        // rows backwards, so a by-index 1D pattern previews as the fixture
        // shows it rather than row-major (#463's open item).
        serpentine: m ? m.snake === 1 : undefined,
      } satisfies DeviceGeom;
    }
    if (pixels <= 0 && g === null) return null;
    if (g) {
      return {
        dims: normDims(g.dims),
        regular: g.regular,
        w: g.w,
        h: g.h,
        source: g.source,
        pixels: pixels || g.w * g.h,
        coords: g.regular ? undefined : (coords ?? undefined),
        // serpentine is #465's to report; row-major until then.
      } satisfies DeviceGeom;
    }
    if (dm.installed && dm.kind === "grid" && dm.w && dm.h) {
      return {
        dims: 2,
        regular: true,
        w: dm.w,
        h: dm.h,
        source: "user",
        pixels,
      } satisfies DeviceGeom;
    }
    if (dm.installed && coords) {
      return {
        dims: normDims(dm.dims),
        regular: false,
        w: 0,
        h: 0,
        source: "user",
        pixels,
        coords,
      } satisfies DeviceGeom;
    }
    return { dims: 1, regular: true, w: pixels, h: 1, source: "board", pixels } satisfies DeviceGeom;
  },
);

/** The device's stored pattern library (empty on firmware without CRUD).
 *  `source` is filled lazily in the background so each row can show a live
 *  preview thumbnail. */
export const devicePatterns = writable<{ id: string; name: string; source?: string }[]>([]);

// ---- settings-page state ----

/** Device output brightness (0–brightnessMax), from GET /api/brightness. */
export const brightness = writable(4);
export const brightnessMax = writable(31);
/** LED protocol the device is driving + the selectable options. */
export const deviceProtocol = writable("sk9822");
export const protocolOptions = writable<string[]>(["sk9822", "ws2812"]);
/** Strip DATA pin (Gitea #154): the GPIO the driver is bound to, the board
 *  default, the pins the picker accepts (empty = panel board or older
 *  firmware → no picker), a stored pin waiting for a reboot, and the one
 *  selected in the form but not yet applied. */
export const dataPin = writable(0);
export const dataPinDefault = writable(0);
export const dataPins = writable<number[]>([]);
export const dataPinNext = writable<number | null>(null);
export const dataPinChoice = writable<number | null>(null);
/** WiFi: the network the device will join next boot (never the password). */
export const wifiSsid = writable<string | null>(null);
export const wifiSource = writable("none");
export const wifiForm = writable({ ssid: "", password: "" });

export const mqttStatus = writable<MqttStatus | null>(null);
export const mqttForm = writable({ host: "", port: 1883, user: "", pass: "" });

export interface OutputStatus {
  order: string;
  gamma: number;
  capMa: number;
  brightCurve: number;
  blur: number;
  glow: number;
}
export const outputStatus = writable<OutputStatus | null>(null);
/** True once the device answered with palette fields (newer firmware). */
export const paletteSupported = writable(false);
export const paletteFlat = writable<readonly number[]>([]);
export const paletteAmount = writable(100);

export const clockStatus = writable<{ synced: boolean; local: number; tzMinutes: number } | null>(
  null,
);
export const syncStatus = writable<SyncStatus | null>(null);
/** Network input (DDP/E1.31) liveness, shown on the Settings tab. */
export const netLive = writable<"ddp" | "e131" | null>(null);

// ---- playlist ----

export const playlist = writable<Playlist>({
  defaultSec: 0,
  crossfadeMs: 0,
  playing: false,
  index: 0,
  items: [],
});
/** A play/stop the device has not confirmed yet (Gitea #431). */
let playlistIntent: TransportIntent | undefined;
/** True from a local edit until it has landed on the device — the poll must
 *  not overwrite the list with a read that predates it. */
let playlistSaving = false;
let playlistDebounce: ReturnType<typeof setTimeout> | undefined;

// ---- the one poll scheduler ----
//
// Contract:
//   * ONE `setInterval` for the whole app, ticking at TICK_MS.
//   * Subscribers register a cadence and a callback; the scheduler invokes a
//     callback when at least its cadence has elapsed since its last run.
//   * It runs only while a device is connected, and stops the moment the last
//     subscriber leaves or the session drops — so a playground tab has no
//     timers at all.
//   * Cadences in use: `status` 1 Hz for the whole session (the fps readout and
//     the capacity headroom), `playlist` 1 Hz while the Playlist tab is open,
//     `settings` 0.5 Hz while the Settings tab is open.
//
// This replaces three ad-hoc `$:` blocks that each cleared and recreated their
// own interval, with their gating conditions duplicated inline.

const TICK_MS = 500;

interface Subscriber {
  everyMs: number;
  fn: () => void | Promise<void>;
  last: number;
  /** A run that has not resolved yet. The next tick skips this subscriber
   *  entirely rather than starting a second one. */
  running: boolean;
}

const subscribers = new Map<string, Subscriber>();
let ticker: ReturnType<typeof setInterval> | undefined;

// A cadence is a ceiling, and a subscriber never runs twice at once (#540).
// The old tick fired every subscriber on schedule whether or not its previous
// run had finished — and when the device is congested a refresh does NOT
// finish quickly: every fetch retries with backoff behind the gate, so one
// run can span ten seconds while each tick queues three or four more requests
// behind it. That is the latch Jeremy hit on the Athom: one slow moment and
// the page piles work onto a device that is already refusing connections, and
// it never climbs back out. Skipping instead of stacking costs nothing when
// the device is healthy — `last` is still stamped at the START of a run, so
// the cadences in the table above are unchanged.
function tick(): void {
  const now = Date.now();
  for (const s of subscribers.values()) {
    if (s.running || now - s.last < s.everyMs) continue;
    s.last = now;
    const r = s.fn();
    if (r === undefined) continue;
    s.running = true;
    void r.finally(() => (s.running = false));
  }
}

function reconcileTicker(): void {
  const want = subscribers.size > 0 && get(device) !== null;
  if (want && ticker === undefined) ticker = setInterval(tick, TICK_MS);
  if (!want && ticker !== undefined) {
    clearInterval(ticker);
    ticker = undefined;
  }
}

device.subscribe(() => reconcileTicker());

/**
 * Register a polled refresh. Returns the unsubscribe function; calling
 * `pollSubscribe` again with the same `id` replaces the previous entry (so a
 * reactive block can re-subscribe freely without stacking timers).
 */
export function pollSubscribe(
  id: string,
  everyMs: number,
  fn: () => void | Promise<void>,
): () => void {
  subscribers.set(id, { everyMs, fn, last: Date.now(), running: false });
  reconcileTicker();
  return () => {
    subscribers.delete(id);
    reconcileTicker();
  };
}

/** Stop every poll and any pending write (app teardown). */
export function pollStopAll(): void {
  subscribers.clear();
  reconcileTicker();
  clearTimeout(playlistDebounce);
}

// ---- refreshers ----

/** Pull `/api/status` for the fps readout, the free-heap headroom and the
 *  device's own vmerr. Best-effort: a failed read leaves the last known
 *  values alone. */
export async function refreshStatus(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    const st = await d.status();
    deviceHeapFree.set(st.heap_free ?? 0);
    deviceEngineHeap.set(st.engine_heap ?? 0);
    deviceGeomStatus.set(st.geom ?? null); // the device's Layout (#464)
    deviceCaps.set(st.caps ?? null); // what it can do (#464)
    if (st.pixels) devicePixels.set(st.pixels);
    deviceFps.set(st.fps);
    deviceOutFps.set(st.out_fps ?? 0);
    deviceRescanHz.set(st.rescan_hz ?? 0);
    if (st.max_pixels) pixelMax.set(st.max_pixels); // per-board cap (#74)
    if (st.version) deviceVersion.set(st.version);
    if (st.slot) deviceSlot.set(st.slot);
    deviceStore.set(st.store ?? null);
    deviceVmerr.set(st.vmerr);
    // DDP/E1.31 liveness rides along here (#540): it is a field of this same
    // body, and the Settings tab used to re-GET the WHOLE of `/api/status` at
    // 0.5 Hz just to read it — the heaviest endpoint on the device, fetched
    // twice over, on a board with three sockets.
    netLive.set(st.live ?? null);
  } catch {
    /* transient; the next poll retries */
  }
}

export async function refreshMqtt(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    mqttStatus.set(await d.mqtt());
  } catch {
    /* older firmware without /api/mqtt */
  }
}

export async function refreshSync(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    syncStatus.set(await d.sync());
  } catch {
    /* older firmware without /api/sync */
  }
}

export async function refreshClock(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    clockStatus.set(await d.clock());
  } catch {
    /* older firmware without /api/clock */
  }
}

/** Fill in the post-process fields firmware older than the chain omits. */
function normalizeOutput(o: {
  order: string;
  gamma: number;
  capMa: number;
  brightCurve?: number;
  blur?: number;
  glow?: number;
}): OutputStatus {
  return {
    order: o.order,
    gamma: o.gamma,
    capMa: o.capMa,
    brightCurve: o.brightCurve ?? 0,
    blur: o.blur ?? 0,
    glow: o.glow ?? 0,
  };
}

export async function refreshOutput(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    const o = await d.output();
    outputStatus.set(normalizeOutput(o));
    paletteSupported.set(o.palette !== undefined);
    paletteFlat.set(o.palette ?? []);
    paletteAmount.set(o.paletteAmount ?? 100);
  } catch {
    /* older firmware without /api/output */
  }
}

/** Adopt a `/api/layout` body as the new state — the ONE place the Layout,
 *  the pixel count, the embedded map and the projection defaults are written
 *  together, so a GET and a POST reply land identically. */
function adoptLayout(l: LayoutWire): void {
  deviceLayoutWire.set(l);
  if (l.pixels) devicePixels.set(l.pixels);
  if (l.max) pixelMax.set(l.max);
  if (l.map) {
    deviceMap.set(l.map);
    if (!l.map.installed) deviceMapCoords.set(null);
  }
  deviceProjection.set({
    proj1d: asMode(l.proj?.proj1d, DEFAULT_PROJECTION.proj1d),
    proj2d: asMode(l.proj?.proj2d, DEFAULT_PROJECTION.proj2d),
    proj3d: asMode(l.proj?.proj3d, DEFAULT_PROJECTION.proj3d),
  });
}

export async function refreshLayout(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    adoptLayout(await d.layout());
  } catch {
    /* older firmware without /api/layout — the geom/map fallbacks carry it */
  }
}

/**
 * Change the Layout: ONE `POST /api/layout` per user action, and the reply IS
 * the new state (docs/api.md) — no re-GET, so the page never shows a value
 * the device has not confirmed. A rejected body changes nothing on either
 * side; the caller reports `error`/`line`.
 */
export async function applyLayout(lines: string): Promise<LayoutResult> {
  const d = get(device);
  if (!d) return { ok: false, error: "no device" };
  let r: LayoutResult;
  try {
    r = await d.setLayout(lines);
  } catch (e) {
    return { ok: false, error: String(e) };
  }
  if (r.ok) adoptLayout(r);
  return r;
}

export async function refreshDeviceMap(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    const m = await d.map();
    deviceMap.set(m);
    if (!m.installed) deviceMapCoords.set(null);
    deviceProjection.set({
      proj1d: asMode(m.proj1d, DEFAULT_PROJECTION.proj1d),
      proj2d: asMode(m.proj2d, DEFAULT_PROJECTION.proj2d),
      proj3d: asMode(m.proj3d, DEFAULT_PROJECTION.proj3d),
    });
  } catch {
    /* older firmware without /api/map */
  }
}

export async function refreshDevicePatterns(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    devicePatterns.set(await d.patterns());
  } catch {
    devicePatterns.set([]); // older firmware — no /api/patterns yet
    return;
  }
  void loadDevicePreviewSources();
}

/** Fetch each stored pattern's source one at a time (the device serves only
 *  ~2 connections, so never in parallel) to feed the row thumbnails. */
async function loadDevicePreviewSources(): Promise<void> {
  const session = get(device);
  if (!session) return;
  for (const p of get(devicePatterns)) {
    if (get(device) !== session) return; // disconnected/reconnected mid-fetch
    if (p.source !== undefined) continue;
    try {
      const full = await session.patternSource(p.id);
      p.source = full.source;
      devicePatterns.update((list) => [...list]); // reflect the filled-in thumbnail
    } catch {
      /* skip a pattern that won't load; its row just stays a spinner */
    }
  }
}

export async function refreshPlaylist(): Promise<void> {
  const d = get(device);
  if (!d || playlistSaving) return;
  try {
    const read = await d.playlist();
    const r = reconcileTransport(read, playlistIntent, Date.now());
    playlist.set(r.playlist);
    playlistIntent = r.intent;
  } catch {
    /* older firmware without /api/playlist — leave empty */
  }
}

/** Persist the playlist to the device (debounced — edits stream in). */
export function queuePlaylistSave(): void {
  clearTimeout(playlistDebounce);
  playlistSaving = true;
  const snapshot = get(playlist);
  playlistDebounce = setTimeout(() => {
    void (async () => {
      try {
        await get(device)?.setPlaylist(snapshot);
      } finally {
        playlistSaving = false;
      }
    })();
  }, 400);
}

/**
 * Append a device pattern to the playlist with a snapshot of its values —
 * the ONE path behind every "Add to playlist" affordance (the editor's, the
 * Patterns tile ⋯ menu's, the Playlist page's `+ Add` picker). Values live
 * on the item and nowhere else (D6: no named presets), so the caller passes
 * whatever it has tuned; `proj` is the per-item projection override (§5.4d),
 * normally left off so the item follows the device default.
 *
 * Optimistic like every other playlist edit: the row appears at once and
 * `queuePlaylistSave` debounces the write. The name is resolved from the
 * device library so the row is not a bare id until the next poll.
 */
export function addToPlaylist(
  patternId: string,
  values: Record<string, number[]> = {},
  proj?: ProjectionMode,
): void {
  const name = get(devicePatterns).find((p) => p.id === patternId)?.name ?? patternId;
  const item: PlaylistItem = {
    id: patternId,
    name,
    kind: "pattern",
    sec: null,
    controls: { ...values },
  };
  if (proj !== undefined) item.proj = proj;
  playlist.update((pl) => ({ ...pl, items: [...pl.items, item] }));
  queuePlaylistSave();
}

/** Show a transport request straight away and let the poll confirm it: the
 *  device applies play/stop in its render loop, so an immediate read-back
 *  can still report the state we just left (Gitea #431). */
export function markTransport(playing: boolean): void {
  playlistIntent = transportIntent(playing, Date.now());
  playlist.update((pl) => ({ ...pl, playing }));
}

// ---- device map writes ----

export async function installDeviceMapCoords(
  dims: number,
  coords: number[][],
): Promise<boolean> {
  const r = await get(device)?.setMap(dims, coords);
  if (!r?.ok) return false;
  deviceMap.set({ installed: true, dims, count: r.count ?? coords.length, kind: "coords" });
  // Remember the coordinates: `GET /api/map` gives a count back, not
  // positions, and the console's preview draws the real scatter from these.
  deviceMapCoords.set(coords);
  // The Layout's KIND follows the map (docs/api.md: a user map makes it
  // `map`), and `deviceLayout` reads the Layout first — so re-read it, or the
  // console keeps previewing the shape it had before the map landed.
  await refreshLayout();
  return true;
}

export async function installDeviceGridMap(w: number, h: number): Promise<boolean> {
  const r = await get(device)?.setGridMap(w, h);
  if (!r?.ok) return false;
  deviceMap.set({ installed: true, dims: 2, count: r.count ?? w * h, kind: "grid", w, h });
  deviceMapCoords.set(null);
  await refreshLayout();
  return true;
}

export async function clearDeviceMap(): Promise<void> {
  await get(device)?.clearMap();
  deviceMap.set({ installed: false, dims: 0, count: 0 });
  deviceMapCoords.set(null);
  await refreshLayout(); // back to the board's own kind
}

// ---- connect ----

export interface ConnectResult {
  ok: boolean;
  /** The device's running pattern, when `pullPattern` asked for it. */
  source: string | null;
}

/**
 * Bind to the device and read its whole state. The base is always known
 * (served-from-device same-origin, or a `?device=` override). No pixel
 * streaming: the preview runs locally; the device is a sink we push code +
 * controls to. `pullPattern` fetches the device's running pattern — the
 * CALLER installs it in the editor (see the layering rule at the top).
 */
export async function connectDevice(base: string, pullPattern = true): Promise<ConnectResult> {
  deviceError.set("");
  deviceBlocked.set(false);
  base = base.trim().replace(/\/+$/, "");
  const session = new DeviceSession(base);
  try {
    const st = await session.status();
    device.set(session);
    deviceBase.set(base);
    devicePixels.set(st.pixels); // hardware pixel count (fixed; layout only rearranges)
    deviceGeomStatus.set(st.geom ?? null); // the device's Layout (#464)
    deviceCaps.set(st.caps ?? null); // what it can do (#464)
    // Per-board cap (#74). Status is authoritative and keeps being refreshed
    // by every later poll; /api/config's `max` is only the fallback for
    // firmware that predates the field — so remember which one we got and
    // don't let the older source overwrite the newer.
    const capFromStatus = st.max_pixels ?? 0;
    if (capFromStatus) pixelMax.set(capFromStatus);
    deviceHeapFree.set(st.heap_free ?? 0); // 0 on a mirror / older firmware
    deviceEngineHeap.set(st.engine_heap ?? 0); // 0 on pre-#287 firmware
    deviceFps.set(st.fps); // seed the status-bar readout from the handshake
    deviceOutFps.set(st.out_fps ?? 0);
    deviceRescanHz.set(st.rescan_hz ?? 0);
    if (st.version) deviceVersion.set(st.version);
    if (st.slot) deviceSlot.set(st.slot);
    deviceStore.set(st.store ?? null);
    deviceVmerr.set(st.vmerr);
    let source: string | null = null;
    if (pullPattern) source = await session.pattern(); // show what's running
    try {
      const b = await session.brightness();
      brightness.set(b.brightness);
      brightnessMax.set(b.max || 31);
    } catch {
      /* older firmware without /api/brightness — leave the default */
    }
    try {
      const c = await session.config();
      pixelMax.set(capFromStatus || c.max || 2048);
      if (c.protocol) deviceProtocol.set(c.protocol);
      if (typeof c.data_pin === "number" && c.data_pins?.length) {
        dataPin.set(c.data_pin);
        dataPinDefault.set(c.data_pin_default ?? c.data_pin);
        dataPins.set(c.data_pins);
        dataPinNext.set(c.data_pin_next ?? null);
        dataPinChoice.set(null);
      }
    } catch {
      /* older firmware without /api/config — pixel count stays read-only */
    }
    try {
      const p = await session.protocol();
      deviceProtocol.set(p.protocol);
      if (p.options?.length) protocolOptions.set(p.options);
    } catch {
      /* older firmware without /api/protocol — leave the default */
    }
    try {
      const w = await session.wifi();
      wifiSsid.set(w.ssid);
      wifiSource.set(w.source);
      wifiForm.set({ ssid: w.ssid ?? "", password: "" });
    } catch {
      /* older firmware — leave defaults */
    }
    try {
      const m = await session.mqtt();
      mqttStatus.set(m);
      mqttForm.set({ host: m.host, port: m.port || 1883, user: m.user, pass: "" });
    } catch {
      /* older firmware without /api/mqtt — leave defaults */
    }
    try {
      const o = await session.output();
      outputStatus.set(normalizeOutput(o));
      paletteSupported.set(o.palette !== undefined);
      paletteFlat.set(o.palette ?? []);
      paletteAmount.set(o.paletteAmount ?? 100);
    } catch {
      /* older firmware without /api/output — card shows unavailable */
    }
    await refreshDevicePatterns();
    await refreshPlaylist();
    await refreshDeviceMap();
    // last, so it wins: `/api/layout` is the source of truth for geometry and
    // overwrites the pixel count / map / projection the aliases above seeded
    await refreshLayout();
    return { ok: true, source };
  } catch (e) {
    device.set(null);
    // fetch() reports a browser refusal as the same opaque TypeError a dead
    // device produces, so the two are told apart by shape, not by error text:
    // only an https page asking for an http target can be refused this way.
    // Keep deviceError a plain sentence — the inline "device unreachable — …"
    // hints reuse it; the full explanation lives in the banner.
    const blocked = browserBlocked(base);
    deviceBlocked.set(blocked);
    deviceError.set(
      blocked
        ? "the browser blocked this page from reaching the device"
        : `cannot reach device: ${String(e)}`,
    );
    return { ok: false, source: null };
  }
}

/** How we bind to a device (a plain playground binds to none):
 *   1. `?device=<base>` — a dev/e2e override pointing the built UI at a
 *      device or the native mirror (known synchronously).
 *   2. served-from-device — the UI loaded from the device's own flash, so the
 *      device is this same origin (probe `/api/status`; a dev server's SPA
 *      fallback returns 200 HTML, so require a genuine device JSON shape). */
export async function detectDeviceBase(): Promise<string | null> {
  const override = new URLSearchParams(location.search).get("device");
  if (override !== null) return override.trim().replace(/\/+$/, "");
  try {
    // Generous abort: on a 2-socket device the boot burst can leave this
    // probe connection-refused for seconds (the gate retries with backoff),
    // and a premature abort strands a real device in playground mode.
    // Genuine playgrounds aren't delayed by it: their origins ANSWER (dev
    // server 200-HTML, static host 404) rather than refuse, so the fetch
    // resolves on the first try either way.
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), 8000);
    const r = await gatedFetch("/api/status", { signal: ctl.signal });
    clearTimeout(t);
    const isJson = r.headers.get("content-type")?.includes("application/json");
    if (r.ok && isJson) {
      const st = (await r.json()) as { pixels?: unknown };
      if (typeof st.pixels === "number") return "";
    }
  } catch {
    /* not a device — stays a playground */
  }
  return null;
}

/** The 1 Hz session poll (Gitea #381): the status-bar counter shows the
 *  device's own rate rather than this browser's preview loop, and the
 *  capacity model gets fresh headroom. Deliberately no faster: a tab at 1 Hz
 *  is the load the panel's compose window is measured against
 *  (docs/tools.md, panel-load-bench), and a tighter poll starves slow
 *  patterns (#259). */
export function startSessionPoll(): void {
  pollSubscribe("status", 1000, refreshStatus);
}
