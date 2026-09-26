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
  type DevicePatternRow,
  type DeviceStatus,
  type LayoutResult,
  type LayoutWire,
  type MqttStatus,
  PatternSourceError,
  type Playlist,
  type PlaylistItem,
  type RunResult,
  type SyncStatus,
} from "../lib/device.ts";
import {
  DEFAULT_PROJECTION,
  deviceGeometry,
  latticeCoords,
  latticeMapFits,
  latticeMapLine,
  PROJECTION_CODES,
  type DeviceGeom,
  type Projection,
  type ProjectionMode,
} from "../lib/geometry.ts";
import { gatedFetch, subscribeGate } from "../lib/fetchgate.ts";
import { browserBlocked } from "../lib/lna.ts";
import { reconcileTransport, transportIntent, type TransportIntent } from "../lib/playlist.ts";
import { probeDeviceOrigin } from "../lib/probe.ts";
import { isTaggedSpriteSource } from "../lib/sprite.ts";
import { note, reportApiError } from "./notify.ts";

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
/** This console is BOUND to a device (`deviceBase !== null`) and has no live
 *  session: the handshake failed and `retryConnect()` is working on it.
 *
 *  It exists because the liveness banner could not see this state. `deviceDown`
 *  counts failures the fetch GATE saw, and a console with no session makes no
 *  requests at all (`refreshStatus` returns early) — so a handshake refused
 *  once left a half-dead page (no Playlist/Settings tab, no status, no
 *  pattern) with nothing on screen saying anything was wrong (the 2026-09-26
 *  panel). `components/ErrorBar.svelte` renders it in the same strip as
 *  `deviceDown`; to the user it is the same fact. */
export const deviceConnectFailed = writable(false);

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
/** The external pattern-array arena (Gitea #253), in bytes: what is free and
 *  how big it is. Both 0 on a board without one (`caps.psram` false) and on
 *  firmware older than the fields, where the Storage row falls back to the
 *  bare `present`. */
export const devicePsramFree = writable(0);
export const devicePsramTotal = writable(0);
/** The device's own post-push verdict (`/api/status` vmerr), if any. */
export const deviceVmerr = writable<string | null>(null);
/** The connected device's own frame rate, from `/api/status` (Gitea #381).
 *  `deviceFps` is what the pattern RENDERED; `deviceOutFps` is what a
 *  pipelined HUB75 board actually DISPLAYED (0 on a strip); `deviceRescanHz`
 *  is the panel's refresh ceiling. All three are 0 until the first poll. */
export const deviceFps = writable(0);
export const deviceOutFps = writable(0);
export const deviceRescanHz = writable(0);

/** What the live pattern is running as on the device: `/api/status`'s `jit`
 *  object (Gitea #658). `null` until the first poll, and on firmware that
 *  predates it. `state: "off"` is a board with no JIT backend, which is
 *  most of the fleet — the console says nothing at all in that case. */
export const deviceJit = writable<DeviceStatus["jit"] | null>(null);

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

/** `/api/status`'s `board` — this image's `board::NAME`, the string a
 *  `.luxr` release package names (Gitea #643). `""` on firmware older than
 *  the field, which means "cannot be checked", never "matches". */
export const deviceBoard = writable("");

/** `/api/status`'s `bc_format` — the LXBC format version the DEVICE reads.
 *  0 on firmware older than the field. Compare with `Luxel.bcFormat()` (what
 *  this bundle COMPILES) through `lib/bcskew.ts`; never assume they agree
 *  just because both are present (Gitea #643). */
export const deviceBcFormat = writable(0);

/** The device's own name, as `/api/status` reports it (`luxel-f6b0a8`). `""`
 *  on firmware older than the field, and on a mirror started without
 *  `--name` — read `deviceLabel`, never this, when you need something to
 *  PRINT. */
export const deviceName = writable("");

/** What to call this device on screen: its name when it has one, else the
 *  host it answers on. Never the word "device" — a title bar that says
 *  "device" tells nobody which board they are looking at (Jeremy,
 *  2026-09-19). Same-origin (`base === ""`) means the browser's own host,
 *  which IS the device. */
export const deviceLabel: Readable<string> = derived(
  [deviceName, deviceBase],
  ([n, b]) => n || hostOf(b) || "",
);

/** Where this device answers — `192.168.0.238`. The ADDRESS, never the name:
 *  the WiFi row states both (mockup S3). */
export const deviceHost: Readable<string> = derived(deviceBase, (b) => hostOf(b));

/** The host part of a device base URL (`http://192.168.0.183` →
 *  `192.168.0.183`), or the page's own host when the UI is served from the
 *  device itself. */
function hostOf(base: string | null): string {
  if (base === null) return "";
  if (base === "") return typeof location === "undefined" ? "" : location.host;
  try {
    return new URL(base).host;
  } catch {
    return base.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  }
}

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
 * when it did; `/api/status`'s `geom` and `/api/map` are the fallbacks for
 * firmware older than that endpoint.
 *
 * WHICH of them wins is not decided here: this is wire parsing, and the
 * policy is `lib/geometry.ts`'s `deviceGeometry`, which is pure and unit
 * tested (`web/tests/geometry.test.mjs`) precisely because it is the one
 * place a console's Layout is decided. It takes DEVICE readings only — which
 * is what keeps the browser's own state (a persisted "Preview as" #539, the
 * working copy's dimensionality, the local engine's effective geometry #573)
 * structurally out of a console's geometry.
 */
export const deviceLayout: Readable<DeviceGeom | null> = derived(
  [deviceLayoutWire, deviceGeomStatus, devicePixels, deviceMap, deviceMapCoords],
  ([wire, g, pixels, dm, coords]) =>
    deviceGeometry({
      layout: wire
        ? {
            dims: wire.dims,
            regular: wire.regular,
            w: wire.w,
            h: wire.h,
            pixels: wire.pixels,
            source: wire.source,
            serpentine: wire.matrix ? wire.matrix.snake === 1 : undefined,
          }
        : null,
      status: g
        ? {
            dims: g.dims,
            regular: g.regular,
            w: g.w,
            h: g.h,
            source: g.source,
            patternDims: g.pattern_dims,
          }
        : null,
      pixels,
      map: dm,
      coords,
    }),
);

/** The device's stored pattern library (empty on firmware without CRUD).
 *  `source` is filled lazily in the background so each row can show a live
 *  preview thumbnail. `stale` is the device saying it can no longer decode
 *  that pattern's blob (Gitea #643) — `lib/heal.ts` recompiles those. */
export const devicePatterns = writable<
  { id: string; name: string; source?: string; stale?: boolean }[]
>([]);

/**
 * Which stored pattern the DEVICE is running — the id the Patterns page rings
 * and the pill names. `""` = the device is on something with no row in its
 * store (a live push from the editor, or a pattern we have not matched yet).
 *
 * Distinct from `devicePatternId` (stores/pattern.ts), which is the id of the
 * document the EDITOR holds. The two were one value until Gitea #563, which is
 * exactly why merely opening a pattern used to run it: there was no way to
 * hold one without claiming the device was playing it.
 *
 * Written by `activateDevicePattern()` below, by the connect handshake's
 * source match (pages/Editor.svelte) and by the editor when a live push makes
 * its own document the running program.
 */
export const deviceRunningId = writable("");

/**
 * Run a stored pattern on the device — THE activation verb (`Play` on a tile,
 * `▶ Play on device` in the editor). A pattern the device already holds is
 * activated by id, which the firmware does NOT treat as a playlist takeover.
 *
 * A `bc-version` rejection means the stored bytecode predates a firmware
 * format bump; the device has no compiler, so the CALLER re-saves from source
 * and retries (compiling lives in stores/pattern.ts, which this module may not
 * import — see the layering rule at the top).
 *
 * **It parks a playing playlist first** (Gitea #538 round 2). A direct play is
 * a takeover of the fixture, and a playlist that keeps auto-advancing on top
 * of it replaces the pattern the user just chose a few seconds later, with
 * nothing on screen saying why. So this stops the auto-advance the way
 * `‖ Pause` does — `POST /api/playlist/stop` with the index remembered (#549)
 * — and records WHICH pattern took over, so the transport can say
 * `stopped — playing <name> directly` and `▶ Play` resumes the parked item.
 *
 * The UI is the only thing this covers: an activation over `/api/patterns/
 * <id>/activate` from Home Assistant, MQTT or curl still leaves the firmware's
 * playlist running. That is a firmware fix — Gitea #602.
 */
export async function activateDevicePattern(id: string): Promise<RunResult> {
  const d = get(device);
  if (!d) return { ok: false, error: "not connected" };
  const name = get(devicePatterns).find((p) => p.id === id)?.name || id;
  if (get(playlist).playing) await playlistPause();
  let r: RunResult;
  try {
    r = await d.activatePattern(id);
  } catch {
    // A write into a device the gate has already watched fail does not wait
    // out the retry ladder — it lands here at once, and says so (#538 r2).
    r = { ok: false, error: "the device did not answer" };
  }
  if (r.ok) {
    deviceRunningId.set(id);
    playlistPreemptedBy.set(name); // the transport prints this verbatim
  } else if (r.code !== "bc-version") {
    // `bc-version` is the caller's to heal (it re-saves from source and
    // retries) — everything else is a refusal the user has to hear about.
    reportApiError(r.error, { scope: "pattern", subject: name });
  }
  return r;
}

/**
 * The pattern a DIRECT play handed the fixture to while the playlist was
 * parked, or `""`. It is the transport's explanation of a `stopped` it did not
 * ask for, and every transport verb clears it — pressing Play (or Pause, or
 * Stop) is the user taking the playlist back.
 */
export const playlistPreemptedBy = writable("");

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
/**
 * Settings the device has STORED but not yet built — what the reboot bar
 * names (Gitea #538). One entry per field the user changed, in the order
 * they changed them, so the bar reads "Changes to the output table, the data
 * pin apply after a reboot" rather than a bare "some settings".
 *
 * A field lands here when the device itself answered `reboot_required` (or,
 * for `/api/datapin`, when its own reply said it is rebooting) — never on
 * the UI's guess about what is live. Protocol and colour order on output 0
 * are LIVE on both hosts, and so is every output's run (`count`, `rev`), so
 * none of those appear here; the data pin, the chain arrangement, an output
 * gained or lost, a FURTHER output's wire format and the device name do
 * (docs/api.md "Live vs reboot", Gitea #550).
 *
 * Cleared by a reboot, and only by a reboot: it is deliberately NOT cleared
 * by navigating away, because the whole point is that the device is running
 * something other than what the page shows until it restarts.
 */
export const rebootPending = writable<readonly string[]>([]);

/** Record that `field` is stored but waits for a reboot. Idempotent. */
export function noteRebootPending(field: string): void {
  rebootPending.update((list) => (list.includes(field) ? list : [...list, field]));
}

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

// The ticker runs for a bound CONSOLE, not only for a live session: the one
// state that most needs a scheduler is "device mode, no session" — nothing
// else re-runs the handshake (`retryConnect`), and that is the state a single
// refused connection used to strand the page in for ever (the 2026-09-26
// panel). A playground has `deviceBase === null` and still schedules nothing.
// Every subscriber already returns early without a session, so the extra ticks
// cost nothing and make no requests.
function reconcileTicker(): void {
  const want = subscribers.size > 0 && (get(device) !== null || get(deviceBase) !== null);
  if (want && ticker === undefined) ticker = setInterval(tick, TICK_MS);
  if (!want && ticker !== undefined) {
    clearInterval(ticker);
    ticker = undefined;
  }
}

device.subscribe(() => reconcileTicker());
deviceBase.subscribe(() => reconcileTicker());

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
  stopLiveness?.();
  stopLiveness = undefined;
  deviceDown.set(false);
}

// ---- refreshers ----

// ---- liveness (Gitea #538 round 2) ----
//
// The 1 Hz status poll is also the app's heartbeat. `lib/fetchgate.ts` counts
// consecutive transport failures across EVERY request; this turns that into
// the one thing surfaces read, plus the timestamp the banner counts up from.
//
// It is a CONDITION, not an event — `components/ErrorBar.svelte` renders it
// in the same pinned strip as a rejected POST, and it clears itself the
// moment something answers.
/** The device has stopped answering (`DOWN_AFTER` consecutive failures). */
export const deviceDown = writable(false);
/** `Date.now()` of the last answered request; 0 before the first. */
export const deviceLastSeen = writable(0);

/** Wire the gate's verdict into the two stores. Idempotent; called by
 *  `startSessionPoll()` and torn down with it. */
function watchLiveness(): () => void {
  return subscribeGate((s) => {
    if (s.lastOkAt) deviceLastSeen.set(s.lastOkAt);
    const was = get(deviceDown);
    if (s.down === was) return;
    deviceDown.set(s.down);
    // Coming back is news too, briefly — otherwise the banner just vanishes
    // and nobody knows whether what they pressed went through.
    if (was && !s.down) note("device", "reconnected", 3000);
  });
}

/** Pull `/api/status` for the fps readout, the free-heap headroom and the
 *  device's own vmerr. Best-effort: a failed read leaves the last known
 *  values alone — but it is also the liveness probe, so the failure itself
 *  is what raises the "device unreachable" banner (through the gate). */
export async function refreshStatus(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    const st = await d.status();
    deviceHeapFree.set(st.heap_free ?? 0);
    deviceEngineHeap.set(st.engine_heap ?? 0);
    devicePsramFree.set(st.psram_free ?? 0); // the second heap, where there is one
    devicePsramTotal.set(st.psram_total ?? 0);
    // Reaching here means a 2xx status body (Gitea #753 deleted the degraded
    // short body, and `DeviceSession.status()` now throws on an error status
    // rather than letting a 503 parse as a status with every field absent).
    // So these two are a real reading and may be written.
    deviceGeomStatus.set(st.geom ?? null); // the device's Layout (#464)
    deviceCaps.set(st.caps ?? null); // what it can do (#464)
    if (st.pixels) devicePixels.set(st.pixels);
    deviceFps.set(st.fps);
    deviceOutFps.set(st.out_fps ?? 0);
    deviceRescanHz.set(st.rescan_hz ?? 0);
    if (st.max_pixels) pixelMax.set(st.max_pixels); // per-board cap (#74)
    if (st.name) deviceName.set(st.name);
    if (st.version) deviceVersion.set(st.version);
    if (st.slot) deviceSlot.set(st.slot);
    deviceStore.set(st.store ?? null);
    deviceBoard.set(st.board ?? "");
    deviceBcFormat.set(st.bc_format ?? 0);
    deviceVmerr.set(st.vmerr);
    deviceJit.set(st.jit ?? null); // native / interpreted, and why (#658)
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

/** Where the `w×h×d` of the lattice this browser installed is remembered.
 *  `GET /api/map` reports a COUNT, so without it a reload turns
 *  `8×8×8 lattice` back into `512 px custom map` (Gitea #538). It is only
 *  ever believed when the device's own pixel count still matches. */
const LATTICE_KEY = "luxel.layout.lattice";

function rememberLattice(w: number, h: number, d: number): void {
  try {
    localStorage.setItem(LATTICE_KEY, `${w}x${h}x${d}`);
  } catch {
    /* private mode: the lattice still installs, only its name is forgotten */
  }
}

/** The remembered lattice, if it accounts for exactly `pixels` points. */
function rememberedLattice(pixels: number): [number, number, number] | null {
  let raw = "";
  try {
    raw = localStorage.getItem(LATTICE_KEY) ?? "";
  } catch {
    return null;
  }
  const m = /^(\d+)x(\d+)x(\d+)$/.exec(raw);
  if (!m) return null;
  const dims: [number, number, number] = [Number(m[1]), Number(m[2]), Number(m[3])];
  return dims[0] * dims[1] * dims[2] === pixels ? dims : null;
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
    // A 3D map whose count is the lattice this browser installed IS that
    // lattice: rebuild its coordinates so the console can name it and draw
    // it after a reload, instead of calling it a positionless cloud.
    if (l.map.installed && l.map.dims === 3 && get(deviceMapCoords) === null) {
      const lat = rememberedLattice(l.map.count);
      if (lat) deviceMapCoords.set(latticeCoords(lat[0], lat[1], lat[2]));
    }
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

/**
 * Rename the device (`POST /api/name`, Gitea #538).
 *
 * The name is what the header chip, the Settings title row and Home
 * Assistant call this board, and `/api/status` serves the new one
 * immediately — so the store is updated from the REPLY and every surface
 * follows within the frame. Only the DHCP hostname waits for a boot, which
 * is why an accepted POST still answers `reboot_required`.
 *
 * An empty `name` clears it back to the board's own `luxel-<mac6>`.
 */
export async function setDeviceName(name: string): Promise<{ ok: boolean; error?: string }> {
  const d = get(device);
  if (!d) return { ok: false, error: "no device" };
  let r;
  try {
    r = await d.setName(name);
  } catch (e) {
    return { ok: false, error: String(e) };
  }
  if (!r.ok) return { ok: false, error: r.error ?? "rejected" };
  if (r.name !== undefined) deviceName.set(r.name);
  if (r.reboot_required) noteRebootPending("the device name");
  return { ok: true };
}

/** Ask the device to re-sync its clock now, then read the clock back — the
 *  sync is asynchronous on firmware, so the reply alone proves nothing. */
export async function syncDeviceClock(): Promise<boolean> {
  const d = get(device);
  if (!d) return false;
  try {
    const r = await d.syncClock();
    await refreshClock();
    return r.ok === true;
  } catch {
    return false;
  }
}

/**
 * Install a `w`×`h`×`d` lattice as the device's geometry (Gitea #538) — the
 * console half of "I cannot try 3D at all".
 *
 * TWO POSTs, deliberately, because `/api/layout` takes at most one
 * `strip`/`matrix`/`map` line per body and a COORDINATE map does not resize
 * the pixel space (only the procedural `map grid W H` form does — see
 * `crates/luxel-core/src/layout.rs`). So: size the pixel space, then install
 * the coordinates into it. A lattice installed into a smaller pixel space
 * would be silently truncated by the engine.
 *
 * The second reply's `pixels` can still be the OLD count (the host applies a
 * resize on its render loop), so this re-reads the Layout instead of
 * adopting that reply.
 */
export async function installLattice(
  w: number,
  h: number,
  d: number,
): Promise<{ ok: boolean; error?: string }> {
  const pixels = w * h * d;
  if (!latticeMapFits(w, h, d)) {
    return { ok: false, error: "that lattice is more coordinates than one request can carry" };
  }
  const sized = await applyLayout(`strip ${pixels}`);
  if (!sized.ok) return { ok: false, error: sized.error ?? "rejected" };
  const mapped = await applyLayout(latticeMapLine(w, h, d));
  if (!mapped.ok) return { ok: false, error: mapped.error ?? "rejected" };
  deviceMap.set({ installed: true, dims: 3, count: pixels, kind: "coords" });
  deviceMapCoords.set(latticeCoords(w, h, d));
  rememberLattice(w, h, d);
  await refreshLayout();
  return { ok: true };
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

/**
 * Re-read the device's stored library, KEEPING the sources already held
 * (Gitea #538 round 2).
 *
 * `GET /api/patterns` answers ids and names only — the source of each row is
 * fetched separately and streams in. Replacing the store with that bare list
 * therefore said "every pattern's source just changed", and `Gallery`'s
 * diff (`components/Gallery.svelte` `syncItems`) does the only correct thing
 * with that: frees every tile engine and recompiles. Deleting ONE of N
 * patterns re-fetched and re-compiled the other N−1 — "deleting a single
 * pattern from 'On device' seems to cause all the pattern previews to
 * regenerate" (Jeremy, 2026-09-20).
 *
 * So a row that is still here keeps the source it already had. A row whose
 * CONTENT changed under the same id — a save that overwrote a name — is not
 * something the wire can tell us (no hash, no mtime), so the writer says so:
 * `invalidate` drops those ids' cached sources and nothing else's.
 */
export function refreshDevicePatterns(invalidate: readonly string[] = []): Promise<void> {
  // COALESCED (the 2026-09-26 panel): opening the scene editor asks for the
  // library three times within a frame — the screen itself, `pickFor`, and the
  // sprite migration — and every one of those used to mean a fresh
  // `/api/patterns` plus a source sweep behind it, into a board with two
  // sockets. A caller that only wants the list to be current joins the read
  // already in flight. An `invalidate` caller must NOT: its whole point is to
  // drop cached sources, which a read that started before it would not do.
  if (invalidate.length === 0 && patternsRefresh) return patternsRefresh;
  const p = doRefreshDevicePatterns(invalidate).finally(() => {
    if (patternsRefresh === p) patternsRefresh = null;
  });
  patternsRefresh = p;
  return p;
}

let patternsRefresh: Promise<void> | null = null;

async function doRefreshDevicePatterns(invalidate: readonly string[]): Promise<void> {
  const d = get(device);
  if (!d) return;
  let rows: DevicePatternRow[];
  try {
    rows = await d.patterns();
  } catch {
    // KEEP the last-known library — the same rule `refreshStatus` follows: a
    // read that failed taught us nothing, so nothing may be written. Wiping it
    // here was the second half of "the scene editor doesn't render any
    // patterns, just text" (Jeremy, 2026-09-26): one 503 dropped every row's
    // cached SOURCE, every pattern layer's `lookup()` then answered null, the
    // renderer bound no engine, and — because this store is refreshed on
    // demand and never polled — it stayed that way until a reload. Only a
    // SUCCESSFUL read of an empty list empties the library, which is also what
    // firmware without `/api/patterns` amounts to: it 404s, the list stays
    // empty because nothing ever filled it.
    return;
  }
  const held = new Map(get(devicePatterns).map((p) => [p.id, p.source]));
  const dropped = new Set(invalidate);
  // An id that is being invalidated, or that this read no longer lists, is no
  // longer known-missing: the next sweep may ask for it again.
  const live = new Set(rows.map((r) => r.id));
  for (const id of [...missingSources]) {
    if (dropped.has(id) || !live.has(id)) missingSources.delete(id);
  }
  devicePatterns.set(
    rows.map((r) => {
      const source = dropped.has(r.id) ? undefined : held.get(r.id);
      const row: { id: string; name: string; source?: string; stale?: boolean } = {
        id: r.id,
        name: r.name,
      };
      if (source !== undefined) row.source = source;
      // Carried through so the self-heal and the Patterns page read one
      // list rather than each re-GETting /api/patterns (#643).
      if (r.stale) row.stale = true;
      return row;
    }),
  );
  void loadDevicePreviewSources();
}

/** Ids the DEVICE says it does not hold (a 404, or "no such pattern"). The
 *  one reason to stop asking; everything else is transient. Cleared for an id
 *  the caller invalidates, and dropped with the row itself. */
const missingSources = new Set<string>();

/** Passes over the rows that still have no source, and the wait before each
 *  retry pass. A source refused because the board is short of heap is served
 *  fine seconds later (the 2026-09-26 panel), so the sweep comes back for it
 *  — it used to give up on that row for the whole session, which is one
 *  permanently blank scene layer per bad body. */
const SOURCE_RETRY_WAIT_MS = [400, 1500, 4000];

/** One sweep at a time. The scene editor, the picker and the Patterns page all
 *  ask for the library at once; without this each one started its own walk of
 *  the SAME rows, they raced each other for the device's two sockets, and the
 *  loser wrote its result into a row object the other had already replaced. */
let sweeping = false;
/** A refresh landed while a sweep was running: its new rows need a pass, so
 *  the running sweep goes round again instead of a second one starting. */
let sweepAgain = false;

/**
 * Fill in every row's `source`, one request at a time (the device serves ~2
 * connections, so never in parallel) — the thumbnails, the scene editor's
 * layers and the pattern picker all read it.
 *
 * Write-back is BY ID (`putSource`), never by mutating the row object the loop
 * is holding: `refreshDevicePatterns` rebuilds the list as new objects, so a
 * sweep that started before it was writing into orphans — the store then
 * published a list that visibly did not contain the source that had just
 * arrived, and the layer stayed blank with nothing to retry it.
 */
async function loadDevicePreviewSources(): Promise<void> {
  if (sweeping) {
    sweepAgain = true;
    return;
  }
  sweeping = true;
  try {
    do {
      sweepAgain = false;
      await sweepSources();
    } while (sweepAgain);
  } finally {
    sweeping = false;
  }
}

async function sweepSources(): Promise<void> {
  const session = get(device);
  if (!session) return;
  for (let pass = 0; pass <= SOURCE_RETRY_WAIT_MS.length; pass++) {
    if (pass > 0) {
      const wait = SOURCE_RETRY_WAIT_MS[pass - 1] ?? 0;
      await new Promise((r) => setTimeout(r, wait));
    }
    if (get(device) !== session) return; // disconnected/reconnected mid-sweep
    const todo = get(devicePatterns)
      .filter((p) => p.source === undefined && !missingSources.has(p.id))
      .map((p) => p.id);
    if (todo.length === 0) return;
    let transient = 0;
    for (const id of todo) {
      if (get(device) !== session) return;
      // Re-read the row: a refresh may have dropped it, and another pass or a
      // save may already have filled it in.
      const row = get(devicePatterns).find((p) => p.id === id);
      if (!row || row.source !== undefined) continue;
      try {
        const full = await session.patternSource(id);
        if (get(device) !== session) return;
        // A pattern that still carries `// @sprite` on line 1 is a SPRITE
        // waiting for `migrateTaggedSprites()` (Gitea #740), not a playable
        // pattern — sprites have their own store and their own tab now, so it
        // leaves the library here, which is the one place device rows are
        // built. `stores/sprites.ts` reads `/api/patterns` itself to convert
        // them, so dropping the row does not hide them from the migration.
        if (isTaggedSpriteSource(full.source)) {
          devicePatterns.update((list) => list.filter((x) => x.id !== id));
          continue;
        }
        putSource(id, full.source);
      } catch (e) {
        if (e instanceof PatternSourceError && e.missing) {
          missingSources.add(id); // gone for good — asking again is a wasted socket
          continue;
        }
        transient++; // busy / empty body / reset: the next pass comes back
      }
    }
    if (transient === 0) return;
  }
}

/** Publish a fetched source into whichever row currently carries that id. */
function putSource(id: string, source: string): void {
  devicePatterns.update((list) => list.map((r) => (r.id === id ? { ...r, source } : r)));
}

export async function refreshPlaylist(): Promise<void> {
  const d = get(device);
  if (!d || playlistSaving) return;
  try {
    const read = await d.playlist();
    const r = reconcileTransport(read, playlistIntent, Date.now());
    playlist.set(r.playlist);
    playlistIntent = r.intent;
    // The park follows live playback (so Pause always has somewhere to hold),
    // and the FIRST read seeds it — a page opened on a device that is already
    // stopped mid-queue should name the item it is stopped on, not item 0.
    if (r.playlist.playing || !parkedSeeded) playlistParked.set(r.playlist.index);
    parkedSeeded = true;
    // A playing playlist is what decides the RUNNING pattern — it moves the
    // device off whatever was activated last, and the Patterns page's ring
    // (and the editor's "is this the one playing?" test) must follow it
    // rather than stay on a stale id (#563).
    if (r.playlist.playing) {
      const item = r.playlist.items[r.playlist.index];
      if (item) deviceRunningId.set(item.id);
    }
  } catch {
    /* older firmware without /api/playlist — leave empty */
  }
}

/**
 * A playlist write did not land (Gitea #538 round 2).
 *
 * Every playlist edit is OPTIMISTIC — the row moves, the slider sticks, and
 * the POST follows 400 ms later. When that POST fails the list on screen is
 * a list nothing agrees with, so it is rolled back to the device's own by
 * re-reading it, and the user is told which it was. Silence here was the
 * whole of "no error toast or anything".
 */
async function playlistWriteFailed(): Promise<void> {
  note("save", "playlist not saved — the device did not answer", 6000);
  playlistSaving = false; // let the follow-up read through
  await refreshPlaylist(); // the device's list wins; the optimistic edit is gone
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
      } catch {
        await playlistWriteFailed();
      } finally {
        playlistSaving = false;
      }
    })();
  }, 400);
}

/**
 * Persist NOW, cancelling any pending debounce.
 *
 * The 400 ms debounce exists for edits that STREAM — a slider being dragged,
 * a duration being typed. A whole-list verb does not stream: Clear is one
 * decision the user already confirmed in a modal, and the debounce turned it
 * into "the rows vanish, nothing happens, then a write lands", which is
 * exactly the lag Jeremy reported (Gitea #538 §A). One-shot verbs call this;
 * streaming ones keep `queuePlaylistSave`.
 */
export function savePlaylistNow(): Promise<void> {
  clearTimeout(playlistDebounce);
  playlistDebounce = undefined;
  playlistSaving = true;
  const snapshot = get(playlist);
  return (async () => {
    try {
      await get(device)?.setPlaylist(snapshot);
    } catch {
      await playlistWriteFailed();
    } finally {
      playlistSaving = false;
    }
  })();
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

/**
 * Save a LIBRARY pattern to the device and then queue it — the `+ Add`
 * picker's Library section (Gitea #538 §F). A gallery pattern is source the
 * device has never seen, and a playlist item is a reference to a STORED
 * pattern (`I <patternId>`), so the save has to land first: an item pointing
 * at an id the device does not hold plays nothing.
 *
 * Bytecode is compiled by the caller (`stores/pattern.ts` owns the wasm
 * host; importing it here would close an import cycle). `savePattern`
 * overwrites a same-named pattern, which is the behaviour the Patterns
 * page's Duplicate/Save already has.
 *
 * The row is appended only on success, so a device that refused the save
 * (out of store space, bad bytecode) leaves the playlist exactly as it was.
 */
export async function saveAndAddToPlaylist(
  name: string,
  source: string,
  bytecode: Uint8Array,
): Promise<{ ok: true } | { ok: false; error: string }> {
  const d = get(device);
  if (!d) return { ok: false, error: "device unreachable" };
  let id: string;
  try {
    const overwritten = get(devicePatterns).find((p) => p.name === name)?.id;
    const r = await d.savePattern(name, source, bytecode);
    if (!r.ok) return { ok: false, error: "error" in r ? r.error : "the device refused the save" };
    await refreshDevicePatterns(overwritten ? [overwritten] : []);
    // `id` is absent on firmware that does not echo it; fall back to the name
    // the device library now holds (the save just created or replaced it).
    const found = r.id || get(devicePatterns).find((p) => p.name === name)?.id;
    if (!found) return { ok: false, error: "saved, but the device did not report its id" };
    id = found;
  } catch {
    return { ok: false, error: "the device did not answer" };
  }
  addToPlaylist(id);
  return { ok: true };
}

/** Show a transport request straight away and let the poll confirm it: the
 *  device applies play/stop in its render loop, so an immediate read-back
 *  can still report the state we just left (Gitea #431). */
export function markTransport(playing: boolean): void {
  playlistIntent = transportIntent(playing, Date.now());
  playlist.update((pl) => ({ ...pl, playing }));
}

// ---- transport (mockup S4) ----
//
// The mock's primary is a PAUSE verb, and the wire has no pause: the routes
// are `play <index>` / `stop` / `next` / `prev` (docs/api.md). What `stop`
// actually does on both the firmware and the mirror is stop the AUTO-ADVANCE
// — the item that was playing stays loaded and keeps rendering, and
// `index` keeps its value. That is a pause of the playlist in every sense
// except one: `play <index>` restarts the item's own clock at 0, so resuming
// replays the current item from its beginning rather than from where it
// stopped.
//
// So Pause is implemented as stop-with-remembered-index, and Play resumes at
// that index. The remembered index is the store's, not the device's, because
// a `stop` the device has not applied yet still reports the old index for a
// poll or two (the #431 settle window).

/**
 * Where playback is PARKED while stopped — what Play will resume, and what
 * the now-playing block names.
 *
 * It cannot live in `playlist.index`: that field is the DEVICE's, refreshed
 * every second, and the device's index does not move while it is stopped. So
 * a Pause that remembered its place there, or a `next` pressed while stopped,
 * would be overwritten by the very next poll.
 */
export const playlistParked = writable(0);
/** The first read seeds the park; after that it only tracks live playback. */
let parkedSeeded = false;

/** Pause: stop the auto-advance, hold this item. */
export async function playlistPause(): Promise<void> {
  const d = get(device);
  // Park on the item the DEVICE is on, not the one the last poll happened to
  // report. Both devices apply `next`/`prev` in their render loop, so a step
  // from a moment ago can still be missing from the 1 Hz read the store holds
  // (the #431 shape, for `index` instead of `playing`) — and parking on the
  // wrong item is exactly the thing Pause exists to get right.
  let at = get(playlist).index;
  try {
    const fresh = await d?.playlist();
    if (fresh) at = fresh.index;
  } catch {
    /* unreachable mid-click — the store's idea is the best we have */
  }
  playlistParked.set(at);
  playlistPreemptedBy.set(""); // a deliberate pause is not a preemption
  markTransport(false);
  await d?.playlistStop();
  void refreshPlaylist();
}

/** Play/resume at `index`, or at the parked position when it is omitted. */
export async function playlistResume(index?: number): Promise<void> {
  const pl = get(playlist);
  const want = index ?? get(playlistParked);
  const i = want >= 0 && want < pl.items.length ? want : 0;
  playlistParked.set(i);
  playlistPreemptedBy.set(""); // the playlist is back; the takeover is history
  markTransport(true);
  playlist.update((p) => ({ ...p, index: i }));
  await get(device)?.playlistPlay(i);
  void refreshPlaylist();
}

/** Stop: leave the playlist AND give up the position, so the next Play starts
 *  the queue from the top. This is the difference between the two left-hand
 *  buttons — Pause holds your place, Stop gives it up. */
export async function playlistStop(): Promise<void> {
  playlistParked.set(0);
  playlistPreemptedBy.set("");
  markTransport(false);
  await get(device)?.playlistStop();
  void refreshPlaylist();
}

/**
 * Previous / next. While PLAYING that is the device's own step. While stopped
 * both devices ignore `next`/`prev` outright, so this moves the parked
 * position instead and sends nothing — which is what a paused music player
 * does, and what makes the two buttons honest in every state rather than dead
 * in one of them.
 */
export async function playlistStep(dir: 1 | -1): Promise<void> {
  const pl = get(playlist);
  if (pl.items.length === 0) return;
  if (!pl.playing) {
    const n = pl.items.length;
    playlistParked.set((((get(playlistParked) + dir) % n) + n) % n);
    return;
  }
  await (dir === 1 ? get(device)?.playlistNext() : get(device)?.playlistPrev());
  void refreshPlaylist();
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
  /** The device's running pattern; null only when the handshake failed. */
  source: string | null;
}

/**
 * Bind to the device and read its whole state. The base is always known
 * (served-from-device same-origin, or a `?device=` override). No pixel
 * streaming: the preview runs locally; the device is a sink we push code +
 * controls to. It always reads the device's running pattern — the CALLER
 * decides what to do with it (see the layering rule at the top). It used to be
 * optional, skipped when the browser arrived holding a dirty working copy that
 * was about to be pushed over the top; since #585 nothing is pushed at boot
 * until we know what is running, so there is nothing to skip.
 */
export async function connectDevice(base: string): Promise<ConnectResult> {
  deviceError.set("");
  deviceBlocked.set(false);
  base = base.trim().replace(/\/+$/, "");
  const session = new DeviceSession(base);
  handshaking = true;
  try {
    // PATIENT (the 2026-09-26 panel): the handshake is not a heartbeat, so it
    // takes the gate's full retry ladder rather than `status()`'s one-shot
    // fast path. One refused connection here used to leave `device` null with
    // `deviceBase` set — a console with no Playlist/Settings tab, no status,
    // and nothing anywhere retrying it (`refreshStatus` returns early without
    // a session). `retryConnect()` below is the other half of that fix.
    const st = await session.status({ patient: true });
    device.set(session);
    deviceBase.set(base);
    deviceConnectFailed.set(false);
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
    devicePsramFree.set(st.psram_free ?? 0); // 0 on a board with no arena
    devicePsramTotal.set(st.psram_total ?? 0);
    deviceFps.set(st.fps); // seed the status-bar readout from the handshake
    deviceOutFps.set(st.out_fps ?? 0);
    deviceRescanHz.set(st.rescan_hz ?? 0);
    if (st.name) deviceName.set(st.name);
    if (st.version) deviceVersion.set(st.version);
    if (st.slot) deviceSlot.set(st.slot);
    deviceStore.set(st.store ?? null);
    deviceBoard.set(st.board ?? "");
    deviceBcFormat.set(st.bc_format ?? 0);
    deviceVmerr.set(st.vmerr);
    deviceJit.set(st.jit ?? null); // native / interpreted, and why (#658)
    const source = await session.pattern(); // what the device is running
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
    // A failed handshake is a CONDITION the user must see, and — unless the
    // browser itself refused, which no amount of retrying fixes — one the
    // scheduler keeps working on (`retryConnect`). `deviceBase` stays set:
    // this is still a console, bound to a device that is not answering.
    deviceBase.set(base);
    deviceConnectFailed.set(!blocked);
    return { ok: false, source: null };
  } finally {
    handshaking = false;
  }
}

/** A handshake is in flight — the boot one or a retry. Both go through
 *  `connectDevice`, and two at once would race the whole store. */
let handshaking = false;

/**
 * Re-run the handshake while this console has a device it is not talking to.
 *
 * On the ONE scheduler (`startSessionPoll`), because a bound-but-dead console
 * is exactly the state nothing used to be watching: `refreshStatus` returns
 * early with no session, so the gate saw no traffic, the liveness banner
 * stayed quiet, and the page sat there with no tabs until someone reloaded it.
 * Cheap when it fails (one fast-failing `/api/status` every few seconds) and
 * it restores the full console when it lands.
 */
async function retryConnect(): Promise<void> {
  if (handshaking || get(device) !== null) return;
  const base = get(deviceBase);
  if (base === null) return;
  if (get(deviceBlocked)) return; // the BROWSER refused; retrying cannot help
  // BACKED OFF, because a handshake is not one request: it is a dozen, and a
  // board that is refusing connections is exactly the board not to send a
  // dozen requests to every four seconds (that pile-on is the latch #540
  // describes). Doubling to half a minute keeps a recovering device found
  // within seconds and a dead one cheap.
  const now = Date.now();
  if (now < reconnectAfter) return;
  const ok = (await connectDevice(base)).ok;
  reconnectFails = ok ? 0 : Math.min(reconnectFails + 1, 3);
  reconnectAfter = ok ? 0 : Date.now() + 4000 * 2 ** reconnectFails;
}

let reconnectFails = 0;
let reconnectAfter = 0;

/** How we bind to a device (a plain playground binds to none):
 *   1. `?device=<base>` — a dev/e2e override pointing the built UI at a
 *      device or the native mirror (known synchronously).
 *   2. served-from-device — the UI loaded from the device's own flash, so the
 *      device is this same origin (probe `/api/status`; a dev server's SPA
 *      fallback returns 200 HTML, so require a genuine device JSON shape).
 *
 * The probe RETRIES (`lib/probe.ts`) — one reading is not a verdict on a board
 * that answers an empty 200 / a 503 / a reset for ~20 s around a scene switch
 * (the 2026-09-26 panel). And when the retries run out inconclusively the
 * answer is still the DEVICE, not the playground: this bundle was served by
 * this origin, so an origin that will not answer its own API is a device
 * having a bad minute. The console then boots in device mode with the
 * "Device unreachable — retrying…" bar up and `retryConnect()` on the
 * scheduler, which is recoverable; falling back to the playground is not
 * ("sometimes the webpage reverts to being a playground; the device is still
 * working", Jeremy, 2026-09-26). `absent` — a 4xx, or HTML where the API
 * should be — is the only playground answer, and it is the one every real
 * playground gives on the FIRST attempt (hosted copy: 404; dev server: HTML),
 * so nothing is delayed by this. */
export async function detectDeviceBase(onBusy?: () => void): Promise<string | null> {
  const override = new URLSearchParams(location.search).get("device");
  if (override !== null) return override.trim().replace(/\/+$/, "");
  let said = false;
  const { verdict } = await probeDeviceOrigin({
    fetch: (url, init) => gatedFetch(url, init),
    onRetry: () => {
      // Once, on the first retry: the boot cover has to say why it is still
      // up, or a busy board looks like a hung page for the whole budget.
      if (said) return;
      said = true;
      onBusy?.();
    },
  });
  return verdict === "absent" ? null : "";
}

/** The 1 Hz session poll (Gitea #381): the status-bar counter shows the
 *  device's own rate rather than this browser's preview loop, and the
 *  capacity model gets fresh headroom. Deliberately no faster: a tab at 1 Hz
 *  is the load the panel's compose window is measured against
 *  (docs/tools.md, panel-load-bench), and a tighter poll starves slow
 *  patterns (#259). */
export function startSessionPoll(): void {
  pollSubscribe("status", 1000, refreshStatus);
  // …and, only while this console is bound to a device it cannot talk to, the
  // handshake retry. It is the same scheduler on purpose (no bare timers —
  // docs/web-architecture.md), and it costs nothing in the healthy case: with
  // a session live `retryConnect` returns on its first line.
  pollSubscribe("reconnect", 4000, retryConnect);
  stopLiveness?.();
  stopLiveness = watchLiveness();
}
let stopLiveness: (() => void) | undefined;
