<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import Controls from "./components/Controls.svelte";
  import Debugger from "./components/Debugger.svelte";
  import Editor from "./components/Editor.svelte";
  import Gallery from "./components/Gallery.svelte";
  import PatternThumb from "./components/PatternThumb.svelte";
  import PlaylistRow from "./components/PlaylistRow.svelte";
  import Preview from "./components/Preview.svelte";
  import VarWatcher from "./components/VarWatcher.svelte";
  import { DeviceSession } from "./lib/device";
  import type { MqttStatus, Playlist, SyncStatus } from "./lib/device";
  import { DEFAULT_PATTERN, type Layout } from "./lib/examples";
  import {
    deletePattern,
    listPatterns,
    loadWorkingCopy,
    savePattern,
    saveWorkingCopy,
    type SavedPattern,
  } from "./lib/store";
  import { parseControlHints, type ControlHint } from "./lib/hints";
  import {
    Engine,
    Luxel,
    type Control,
    type DebugSnapshot,
    type Diagnostic,
    type RuntimeError,
    type StepKind,
  } from "./lib/luxel";
  import { MicSource, toSensorBoardFrame } from "./lib/audio";

  let luxel: Luxel | undefined;
  let engine: Engine | undefined;
  let editor: Editor;
  let preview: Preview;

  let source = DEFAULT_PATTERN.source;
  let layout: Layout = DEFAULT_PATTERN.layout;
  let exampleName = DEFAULT_PATTERN.name;

  let compileError: Diagnostic | null = null;
  let runtimeError: RuntimeError | null = null;
  let controls: Control[] = [];
  let hints: Map<string, ControlHint> = new Map();
  let controlValues: Record<string, number[]> = {};
  let readouts = new Map<string, number>();
  let vars: Record<string, number | number[]> = {};
  let fps = 0;
  let targetFps = 60;
  let running = true;
  let loadFailure = "";
  /** The editor's source differs from the pattern it was loaded from / saved
   *  as — genuinely unsaved changes. Persisted in the working copy so a reload
   *  can tell "resume my edit" from "show what's running". */
  let dirty = false;
  /** Fetching/activating a pattern for the editor — cover the editor with a
   *  loading screen so a stale last-opened script never flashes first. */
  let patternLoading = false;
  /** First-load cover: hides the app until we've decided playground vs device
   *  (and, on a device, loaded its running pattern) so nothing flashes first. */
  let booting = true;
  let bootLabel = "loading…";
  let debugMode = false;
  let breakpoints: number[] = [];
  let dbg: DebugSnapshot = { paused: false };

  let debounce: ReturnType<typeof setTimeout> | undefined;
  let raf = 0;
  let lastT = 0;
  let lastPoll = 0;

  // ---- device mode ----
  let device: DeviceSession | null = null;
  let deviceError = "";
  /** The device's stored pattern library (empty on firmware without CRUD).
   *  `source` is filled lazily in the background so each row can show a live
   *  preview thumbnail. */
  let devicePatterns: { id: string; name: string; source?: string }[] = [];
  /** Set while the editor holds a device-stored pattern. */
  let devicePatternId = "";
  /** The device's fixed hardware pixel count — the source of truth for the
   *  pixel total in device mode (layout changes only rearrange the preview,
   *  they never change how many pixels the device drives). */
  let devicePixels = 0;
  /** Device output brightness (0–brightnessMax), from GET /api/brightness. */
  let brightness = 4;
  let brightnessMax = 31;
  /** Max pixel count the device firmware accepts (GET /api/config). */
  let pixelMax = 2048;
  /** LED protocol the device is driving + the selectable options. */
  let deviceProtocol = "sk9822";
  let protocolOptions: string[] = ["sk9822", "ws2812"];
  /** WiFi: the network the device will join next boot (never the password). */
  let wifiSsid: string | null = null;
  let wifiSource = "none";
  let wifiForm = { ssid: "", password: "" };
  let wifiNote = "";

  // ---- MQTT / Home Assistant (device mode) ----
  let mqttStatus: MqttStatus | null = null;
  let mqttForm = { host: "", port: 1883, user: "", pass: "" };
  let mqttNote = "";

  async function refreshMqtt(): Promise<void> {
    if (!device) return;
    try {
      mqttStatus = await device.mqtt();
    } catch {
      /* older firmware without /api/mqtt */
    }
  }

  // ---- output pipeline (device mode) ----
  let outputStatus: { order: string; gamma: number; capMa: number } | null = null;

  async function refreshOutput(): Promise<void> {
    if (!device) return;
    try {
      outputStatus = await device.output();
    } catch {
      /* older firmware without /api/output */
    }
  }

  function onOutputChange(): void {
    void (async () => {
      const o = outputStatus;
      if (!o) return;
      await device?.setOutput(o.order, o.gamma, o.capMa);
      void refreshOutput();
    })();
  }

  // ---- wall clock / timezone (device mode) ----
  let clockStatus: { synced: boolean; local: number; tzMinutes: number } | null = null;

  async function refreshClock(): Promise<void> {
    if (!device) return;
    try {
      clockStatus = await device.clock();
    } catch {
      /* older firmware without /api/clock */
    }
  }

  function onTzChange(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    if (!Number.isFinite(v)) return;
    void (async () => {
      await device?.setClock(v * 60); // UI is hours, API is minutes
      void refreshClock();
    })();
  }

  const fmtDeviceTime = (unixLocal: number): string => {
    // the value is already local — render it without the browser's tz
    const d = new Date(unixLocal * 1000);
    return d.toLocaleString("en-US", { timeZone: "UTC", hour12: false });
  };

  // ---- AP-mode provisioning (device mode) ----
  let apNote = "";

  function startApMode(): void {
    if (!window.confirm("Reboot the device into its setup access point? It leaves this network for one boot (rejoin it by saving WiFi from the AP, or just reboot it again).")) return;
    void (async () => {
      const r = await device?.startApMode();
      apNote = r?.ok ? "rebooting into AP \"luxel-…\" — connect to it at 192.168.4.1" : "failed";
      setTimeout(() => (apNote = ""), 8000);
    })();
  }

  // ---- Luxel-to-Luxel sync (device mode) ----
  let syncStatus: SyncStatus | null = null;

  async function refreshSync(): Promise<void> {
    if (!device) return;
    try {
      syncStatus = await device.sync();
    } catch {
      /* older firmware without /api/sync */
    }
  }

  function onSyncModeChange(e: Event): void {
    const mode = (e.target as HTMLSelectElement).value as "off" | "leader" | "follower";
    void (async () => {
      await device?.setSync(mode);
      void refreshSync();
    })();
  }

  function saveMqtt(): void {
    void (async () => {
      mqttNote = "saving…";
      const host = mqttForm.host.trim();
      const r = await device?.setMqtt(host, mqttForm.port, mqttForm.user.trim(), mqttForm.pass);
      if (r?.ok) {
        mqttNote = host ? "saved — connecting to the broker…" : "saved — MQTT disabled";
        mqttForm = { ...mqttForm, pass: "" };
      } else {
        mqttNote = r?.error ? `failed: ${r.error}` : "save failed";
      }
      void refreshMqtt();
    })();
  }
  /** Whether a pixel map is installed on the device (render2D geometry). */
  let deviceMap = { installed: false, dims: 0, count: 0 };

  async function refreshDeviceMap(): Promise<void> {
    if (!device) return;
    try {
      deviceMap = await device.map();
    } catch {
      /* older firmware without /api/map */
    }
  }

  /** Install the current computed map on the device (device patterns then
   *  render2D with this geometry). */
  function installDeviceMap(): void {
    if (!device || layout.kind !== "map") return;
    const coords = layout.coords;
    const dims = (coords[0]?.length ?? 2) >= 3 ? 3 : 2;
    void (async () => {
      const r = await device?.setMap(dims, coords);
      if (r?.ok) {
        deviceMap = { installed: true, dims, count: r.count ?? coords.length };
        saveNote = "map installed on the device";
        setTimeout(() => (saveNote = ""), 2500);
      }
    })();
  }

  function clearDeviceMap(): void {
    void (async () => {
      await device?.clearMap();
      deviceMap = { installed: false, dims: 0, count: 0 };
      saveNote = "device map cleared";
      setTimeout(() => (saveNote = ""), 2000);
    })();
  }

  // ---- playlist (device mode) ----
  let playlist: Playlist = { defaultSec: 0, crossfadeMs: 0, playing: false, index: 0, items: [] };
  let playlistDebounce: ReturnType<typeof setTimeout> | undefined;
  let playlistPoll: ReturnType<typeof setInterval> | undefined;

  async function refreshPlaylist(): Promise<void> {
    if (!device) return;
    try {
      playlist = await device.playlist();
    } catch {
      /* older firmware without /api/playlist — leave empty */
    }
  }

  /** Persist the playlist to the device (debounced — edits stream in). */
  function queuePlaylistSave(): void {
    clearTimeout(playlistDebounce);
    const snapshot = playlist;
    playlistDebounce = setTimeout(() => void device?.setPlaylist(snapshot), 400);
  }

  /** Append the CURRENT editor pattern (must be saved on the device) with its
   *  current control values — so the same pattern can be added repeatedly with
   *  different params. */
  function addToPlaylist(): void {
    if (!device || !devicePatternId) return;
    const name = devicePatterns.find((p) => p.id === devicePatternId)?.name ?? patternName;
    const controls: Record<string, number[]> = {};
    for (const [k, v] of Object.entries(controlValues)) controls[k] = v;
    playlist = {
      ...playlist,
      items: [...playlist.items, { id: devicePatternId, name, sec: null, controls }],
    };
    queuePlaylistSave();
    saveNote = "added to playlist";
    setTimeout(() => (saveNote = ""), 2000);
  }

  function removePlaylistItem(i: number): void {
    playlist = { ...playlist, items: playlist.items.filter((_, j) => j !== i) };
    queuePlaylistSave();
  }

  function clearPlaylist(): void {
    if (playlist.items.length === 0) return;
    if (!window.confirm("clear the whole playlist?")) return;
    playlist = { ...playlist, items: [] };
    queuePlaylistSave();
  }

  /** Total auto-advance run time (manual items — effective 0s — are excluded);
   *  also whether any item is manual. */
  $: playlistTotalSec = playlist.items.reduce(
    (s, it) => s + Math.max(0, it.sec ?? playlist.defaultSec),
    0,
  );
  $: playlistHasManual = playlist.items.some((it) => (it.sec ?? playlist.defaultSec) <= 0);
  const fmtDuration = (sec: number): string => {
    if (sec <= 0) return "0s";
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  };
  /** A playlist item whose pattern was deleted from the device. */
  const itemMissing = (id: string): boolean => !devicePatterns.some((p) => p.id === id);

  function movePlaylistItem(i: number, dir: number): void {
    const j = i + dir;
    const items = [...playlist.items];
    const a = items[i];
    const b = items[j];
    if (a === undefined || b === undefined) return;
    items[i] = b;
    items[j] = a;
    playlist = { ...playlist, items };
    queuePlaylistSave();
  }

  /** drag-to-reorder: index the grip drag started from. */
  let playlistDragFrom = -1;
  function dropPlaylistItem(to: number): void {
    const from = playlistDragFrom;
    playlistDragFrom = -1;
    if (from < 0 || from === to) return;
    const items = [...playlist.items];
    const [moved] = items.splice(from, 1);
    if (moved === undefined) return;
    items.splice(to, 0, moved);
    playlist = { ...playlist, items };
    queuePlaylistSave();
  }

  function onDefaultSecChange(e: Event): void {
    const v = (e.target as HTMLInputElement).value.trim();
    playlist = { ...playlist, defaultSec: v === "" ? 0 : Math.max(0, Math.round(Number(v) || 0)) };
    queuePlaylistSave();
  }

  function onCrossfadeChange(e: Event): void {
    const v = (e.target as HTMLInputElement).value.trim();
    // field is in seconds; store ms
    const ms = v === "" ? 0 : Math.max(0, Math.round((Number(v) || 0) * 1000));
    playlist = { ...playlist, crossfadeMs: ms };
    queuePlaylistSave();
  }

  async function playlistPlay(): Promise<void> {
    await device?.playlistPlay(0);
    void refreshPlaylist();
  }
  async function playlistStop(): Promise<void> {
    await device?.playlistStop();
    void refreshPlaylist();
  }
  async function playlistNext(): Promise<void> {
    await device?.playlistNext();
    void refreshPlaylist();
  }
  async function playlistPrev(): Promise<void> {
    await device?.playlistPrev();
    void refreshPlaylist();
  }

  /** Source for a playlist item's thumbnail/params, from the device library. */
  const itemSource = (id: string): string | undefined =>
    devicePatterns.find((p) => p.id === id)?.source;

  // follow the playing item while the Playlist tab is open (light status poll,
  // not pixel streaming) so the current entry highlights as it advances
  $: {
    clearInterval(playlistPoll);
    if (device && tab === "playlist" && !editing && playlist.playing) {
      playlistPoll = setInterval(refreshPlaylist, 1000);
    }
  }

  // ---- network input (DDP/E1.31) status, shown on the Settings tab ----
  let netLive: "ddp" | "e131" | null = null;
  let netPoll: ReturnType<typeof setInterval> | undefined;

  async function refreshNetLive(): Promise<void> {
    if (!device) return;
    try {
      netLive = (await device.status()).live ?? null;
    } catch {
      /* older firmware without the live field — stays idle */
    }
  }

  $: {
    clearInterval(netPoll);
    if (device && tab === "settings" && !editing) {
      void refreshNetLive();
      void refreshMqtt();
      void refreshSync();
      void refreshClock();
      void refreshOutput();
      netPoll = setInterval(() => {
        void refreshNetLive();
        void refreshMqtt();
        void refreshSync();
        void refreshClock();
      }, 2000);
    }
  }

  async function refreshDevicePatterns(): Promise<void> {
    if (!device) return;
    try {
      devicePatterns = await device.patterns();
    } catch {
      devicePatterns = []; // older firmware — no /api/patterns yet
      return;
    }
    void loadDevicePreviewSources();
  }

  // The device streams only source, not which library entry it came from — so
  // a freshly-opened running pattern shows as "untitled". If its source matches
  // a saved device pattern, adopt that name/id (so the header isn't "untitled"
  // and Add-to-playlist works). Runs as device pattern sources stream in.
  // deps passed as args so Svelte tracks devicePatterns (source-fill re-runs it)
  $: matchRunningToLibrary(devicePatterns, source, dirty, devicePatternId, editing, device);
  function matchRunningToLibrary(
    pats: typeof devicePatterns,
    src: string,
    drt: boolean,
    dpid: string,
    edt: boolean,
    dev: DeviceSession | null,
  ): void {
    if (!dev || !edt || drt || dpid || !src) return;
    const m = pats.find((p) => p.source && p.source.trim() === src.trim());
    if (m) {
      patternName = m.name;
      devicePatternId = m.id;
    }
  }

  /** Fetch each stored pattern's source one at a time (the device serves only
   *  ~2 connections, so never in parallel) to feed the row thumbnails. */
  async function loadDevicePreviewSources(): Promise<void> {
    const session = device;
    if (!session) return;
    for (const p of devicePatterns) {
      if (device !== session) return; // disconnected/reconnected mid-fetch
      if (p.source !== undefined) continue;
      try {
        const full = await session.patternSource(p.id);
        p.source = full.source;
        devicePatterns = devicePatterns; // reflect the filled-in thumbnail
      } catch {
        /* skip a pattern that won't load; its row just stays a spinner */
      }
    }
  }

  /** Compile source with the local wasm engine and return its LXBC bytecode
   *  (null if it doesn't compile). Fresh compile so the blob always matches
   *  the given source, not a stale preview engine. */
  function compileToBytecode(src: string): Uint8Array | null {
    if (!luxel) return null;
    const eng = luxel.compile(src, pixelCount());
    if (!(eng instanceof Engine)) return null;
    try {
      return eng.bytecode();
    } finally {
      eng.free();
    }
  }

  async function loadDevicePattern(id: string): Promise<void> {
    if (!device) return;
    try {
      const p = await device.patternSource(id);
      let r = await device.activatePattern(id);
      if (!r.ok && r.code === "bc-version") {
        // the stored bytecode predates a firmware format bump (the device
        // can't recompile — it has no compiler): recompile from the stored
        // source, re-save, and retry once
        const bc = compileToBytecode(p.source);
        if (bc) {
          await device.savePattern(p.name, p.source, bc);
          r = await device.activatePattern(id);
        }
      }
      if (!r.ok) {
        deviceError = `activate failed: ${r.error}`;
        return;
      }
      devicePatternId = id;
      patternName = p.name;
      exampleName = "";
      source = p.source;
      dirty = false; // freshly loaded from the device — matches what's running
      hints = parseControlHints(source);
      compileError = null;
      // controls come from the local recompile the caller runs next
    } catch (e) {
      deviceError = `cannot load pattern: ${String(e)}`;
    }
  }
  /** Debounced push of the working source to the device (over WiFi, so slower
   *  than the local preview recompile). */
  let pushDebounce: ReturnType<typeof setTimeout> | undefined;

  /** The device this app is bound to, if any. `""` = served from the device
   *  itself (same origin); a URL = a `?device=` dev/e2e override; `null` = a
   *  plain playground (no hardware — no connection UI at all). Set once we
   *  know we're on a device; survives disconnect so reconnect needs no URL. */
  let deviceBase: string | null = null;

  /** No device involved — a hosted/standalone playground. Devices are only
   *  reached by loading the UI *from* a device (or a `?device=` override), so
   *  the playground never shows connect/share-to-device affordances. */
  $: isPlayground = deviceBase === null;

  /** "device" whenever we're bound to hardware (even while disconnected),
   *  "playground" otherwise — drives which affordances the header shows. */
  $: mode = isPlayground ? "playground" : "device";

  // ---- navigation ----
  // Home tabs: Patterns Library (always), Device Patterns + Settings (device
  // only). The editor is NOT a tab — it opens full-screen over the home tab
  // when you pick a pattern or create one, with a back button. `tab` is the
  // home you return to.
  type Tab = "library" | "pixelblaze" | "device" | "playlist" | "settings";
  let tab: Tab = "library";
  /** The "PixelBlaze Library" tab browses the scraped corpus (original
   *  Pixelblaze community patterns). It's a local-only convenience: the tab
   *  only exists when tools/gen-corpus-gallery.mjs found a populated corpus/
   *  and wrote public/pixelblaze-library.json (see onMount probe). */
  let hasPixelblazeLibrary = false;
  let pixelblazeMounted = false;
  $: if (tab === "pixelblaze" && !editing) pixelblazeMounted = true;
  /** Full-screen editor open (over the home tab). */
  let editing = false;
  /** File-actions overflow menu (import/export). */
  let menuOpen = false;
  /** Lazy-mount the gallery on first Library visit, then keep it alive so its
   *  compiled tile engines persist. */
  let galleryMounted = false;
  $: if (tab === "library" && !editing) galleryMounted = true;

  const NEW_PATTERN = `export function render(index) {
  hsv(index / pixelCount, 1, 1)
}`;

  /** Open the editor full-screen; `home` is the tab the back button returns
   *  to (Library for local patterns, Device Patterns for device ones). */
  function openEditor(home: Tab): void {
    tab = home;
    editing = true;
    // A new/switched pattern gets a fresh waterfall: wipe the previous
    // pattern's scroll history now, synchronously. recompile() also clears on a
    // successful local compile, but that path is skipped in device mode (push
    // returns early) and on a compile error — so clearing here covers every
    // open path (device-pattern switch included) regardless of outcome.
    preview?.clear();
  }

  function closeEditor(): void {
    editing = false;
  }

  /** Editor keyboard shortcuts: ⌘/Ctrl+S save, ⌘/Ctrl+Enter force run + push. */
  function onKeydown(e: KeyboardEvent): void {
    if (!editing) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      saveToLibrary();
    } else if (mod && e.key === "Enter") {
      e.preventDefault();
      applyEdit(); // recompile the preview + push to the device
    }
  }

  /** Start a brand-new pattern in the editor. `onDevice` routes save to the
   *  device (from the Device Patterns tab) vs the local library. */
  function newPattern(onDevice: boolean): void {
    source = NEW_PATTERN;
    patternName = "";
    exampleName = "";
    devicePatternId = "";
    importError = "";
    controlValues = {};
    dirty = false; // a fresh template — not yet edited
    subTab = "pattern";
    if (layout.kind === "map") layout = { kind: "strip", pixels: pixelCount() };
    void tick().then(applyEdit);
    openEditor(onDevice ? "device" : "library");
  }

  function openSavedPattern(name: string): void {
    loadSaved(name);
    openEditor("library");
  }

  async function openDevicePatternInEditor(id: string): Promise<void> {
    openEditor("device");
    patternLoading = true; // cover the editor until the source is fetched
    try {
      await loadDevicePattern(id); // activates it on the device
      recompile(); // build the local preview (no push — it's already running)
    } finally {
      patternLoading = false;
    }
  }

  /** The label/target the editor's back button returns to. */
  $: backLabel =
    tab === "device"
      ? "Device Patterns"
      : tab === "pixelblaze"
        ? "PixelBlaze Library"
        : "Patterns Library";
  /** Total installed pixels — reactive so the Settings readout tracks the
   *  active layout (device connect sets it from the hardware). */
  $: pixelTotal =
    layout.kind === "strip"
      ? layout.pixels
      : layout.kind === "grid"
        ? layout.w * layout.h
        : layout.coords.length;
  // the map sub-tab exists only while a 2D map is the active layout
  $: if (layout.kind !== "map" && subTab === "map") subTab = "pattern";

  const pixelCount = () =>
    layout.kind === "strip"
      ? layout.pixels
      : layout.kind === "grid"
        ? layout.w * layout.h
        : layout.coords.length;

  /** Bind to the device and open its running pattern. The base is always known
   *  (served-from-device same-origin, or a `?device=` override). No pixel
   *  streaming: the preview runs locally; the device is a sink we push code +
   *  controls to. `pullPattern` loads the device's running pattern into the
   *  editor; pass false to keep an in-progress edit (working copy). */
  async function connectDevice(base: string, pullPattern = true): Promise<void> {
    deviceError = "";
    base = base.trim().replace(/\/+$/, "");
    const session = new DeviceSession(base);
    try {
      const st = await session.status();
      device = session;
      deviceBase = base;
      devicePixels = st.pixels; // hardware pixel count (fixed; layout only rearranges)
      layout = { kind: "strip", pixels: st.pixels };
      if (pullPattern) {
        source = await session.pattern(); // show what's running on the device
        dirty = false; // editor now matches the running pattern
        hints = parseControlHints(source);
        patternName = "";
        exampleName = "";
        devicePatternId = "";
      }
      try {
        const b = await session.brightness();
        brightness = b.brightness;
        brightnessMax = b.max || 31;
      } catch {
        /* older firmware without /api/brightness — leave the default */
      }
      try {
        const c = await session.config();
        pixelMax = c.max || 2048;
        if (c.protocol) deviceProtocol = c.protocol;
      } catch {
        /* older firmware without /api/config — pixel count stays read-only */
      }
      try {
        const p = await session.protocol();
        deviceProtocol = p.protocol;
        if (p.options?.length) protocolOptions = p.options;
      } catch {
        /* older firmware without /api/protocol — leave the default */
      }
      try {
        const w = await session.wifi();
        wifiSsid = w.ssid;
        wifiSource = w.source;
        wifiForm = { ssid: w.ssid ?? "", password: "" };
      } catch {
        /* older firmware — leave defaults */
      }
      try {
        const m = await session.mqtt();
        mqttStatus = m;
        mqttForm = { host: m.host, port: m.port || 1883, user: m.user, pass: "" };
      } catch {
        /* older firmware without /api/mqtt — leave defaults */
      }
      try {
        outputStatus = await session.output();
      } catch {
        /* older firmware without /api/output — card shows unavailable */
      }
      compileError = null;
      await refreshDevicePatterns();
      await refreshPlaylist();
      await refreshDeviceMap();
    } catch (e) {
      device = null;
      deviceError = `cannot reach device: ${String(e)}`;
    }
  }

  /** Send the current source + its compiled LXBC bytecode to the device so
   *  its real LEDs follow the local preview. The device has no compiler —
   *  the local engine's bytecode IS what it executes; a rejection (or
   *  network error) surfaces as a device error. */
  async function devicePush(): Promise<void> {
    if (!device) return;
    if (compileError || !engine) return; // never push a pattern the local compile rejected
    try {
      const r = await device.run(source, engine.bytecode());
      if (!r.ok) deviceError = `device rejected the pattern: ${r.error}`;
      else deviceError = "";
    } catch (e) {
      deviceError = `push failed: ${String(e)}`;
    }
  }

  /** Local recompile (for the preview) plus an immediate device push — used
   *  when a whole new pattern is opened/created (typing debounces separately). */
  function applyEdit(): void {
    recompile();
    if (device) void devicePush();
  }

  /** Rebuild the local preview engine from `source`. Device-independent now:
   *  the preview always runs on the local WASM engine, even on a device. */
  function recompile(): void {
    if (!luxel) return;
    const result = luxel.compile(source, pixelCount());
    if (result instanceof Engine) {
      engine?.free();
      engine = result;
      compileError = null;
      runtimeError = null;
      if (layout.kind === "grid") engine.setMapGrid(layout.w, layout.h);
      if (layout.kind === "map") engine.setMap(layout.coords);
      engine.setWallClock(Date.now() / 1000);
      hints = parseControlHints(source);
      controls = engine.controls();
      if (debugMode) {
        engine.debugEnable(true);
        applyBreakpoints();
      }
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      // seed //# defaults, then reapply saved control values (PB persists
      // control state per pattern)
      for (const c of controls) {
        if (c.kind === "showNumber" || c.kind === "gauge" || c.kind === "trigger") continue;
        const d = hints.get(c.name)?.default;
        if (!(c.name in controlValues) && d !== undefined) {
          controlValues = { ...controlValues, [c.name]: [d] };
        }
        const saved = controlValues[c.name];
        if (saved) engine.setControl(c.name, saved);
      }
      preview?.clear(); // fresh program → fresh history
    } else {
      compileError = result; // keep the old engine running while typing
    }
  }

  function onSourceChange(e: CustomEvent<string>): void {
    source = e.detail;
    dirty = true; // the user edited away from the loaded/saved pattern
    // local preview recompiles fast; the device push (over WiFi) is throttled
    clearTimeout(debounce);
    debounce = setTimeout(recompile, 150);
    if (device) {
      clearTimeout(pushDebounce);
      pushDebounce = setTimeout(() => void devicePush(), 500);
    }
  }

  // ---- .epe import / export (Pixel Blaze pattern interchange) ----
  // An .epe is JSON: { name, id, sources: { main } } — we recompile from
  // sources.main, the only portable part (PB byte/blob keys are its own
  // compiled artifacts).

  let fileInput: HTMLInputElement;
  let importError = "";
  /** Name from an imported .epe; used for the export filename. */
  let patternName = "";

  async function importEpeFile(file: File): Promise<void> {
    importError = "";
    try {
      const epe = JSON.parse(await file.text()) as {
        name?: unknown;
        sources?: { main?: unknown };
      };
      const main = epe.sources?.main;
      if (typeof main !== "string" || main.length === 0) {
        throw new Error("no sources.main — is this a Pixel Blaze .epe export?");
      }
      patternName =
        typeof epe.name === "string" && epe.name !== ""
          ? epe.name
          : file.name.replace(/\.(epe|json)$/i, "");
      exampleName = "";
      devicePatternId = "";
      source = main;
      controlValues = {};
      dirty = true; // an imported .epe isn't in the library/device until saved
      editing = true; // a dropped/imported .epe opens straight in the editor
      preview?.clear(); // fresh waterfall for the imported pattern
      await tick();
      applyEdit();
    } catch (e) {
      importError = `.epe import failed: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  function onImportPick(e: Event): void {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) void importEpeFile(f);
    input.value = ""; // allow re-importing the same file
  }

  function onDrop(e: DragEvent): void {
    e.preventDefault();
    const f = e.dataTransfer?.files?.[0];
    if (f && /\.(epe|json)$/i.test(f.name)) void importEpeFile(f);
  }

  // ---- local pattern library + working-copy autosave ----

  let saved: SavedPattern[] = listPatterns();
  let saveNote = "";
  let autosave: ReturnType<typeof setTimeout> | undefined;

  /** Debounced: the working copy survives closed tabs and reloads. */
  function queueAutosave(): void {
    clearTimeout(autosave);
    autosave = setTimeout(() => {
      saveWorkingCopy({ source, layout, patternName, exampleName, dirty });
    }, 800);
  }
  // re-persist on any change to the fields the working copy stores — including
  // `dirty`, which flips to false on save WITHOUT a source change (so a reload
  // then correctly defers to the device instead of resuming a saved pattern)
  $: {
    source;
    layout;
    dirty;
    queueAutosave();
  }

  function saveToLibrary(): void {
    const suggestion = patternName || exampleName || "my pattern";
    const where = device ? "save pattern on the DEVICE as:" : "save pattern as:";
    const name = window.prompt(where, suggestion)?.trim();
    if (!name) return;
    if (device) {
      void (async () => {
        const bc = compileToBytecode(source);
        if (!bc) {
          saveNote = "save failed: pattern does not compile";
          setTimeout(() => (saveNote = ""), 3000);
          return;
        }
        const r = await device?.savePattern(name, source, bc);
        if (r?.ok) {
          patternName = name;
          exampleName = "";
          devicePatternId = r.id ?? "";
          dirty = false; // now stored on the device
          saveNote = "saved to device";
          await refreshDevicePatterns();
        } else {
          saveNote = r && "error" in r ? `save failed: ${r.error}` : "save failed";
        }
        setTimeout(() => (saveNote = ""), 3000);
      })();
      return;
    }
    saved = savePattern(name, source);
    patternName = name;
    exampleName = "";
    dirty = false; // now stored in the library
    saveNote = "saved";
    setTimeout(() => (saveNote = ""), 2000);
  }

  function loadSaved(name: string): void {
    const p = saved.find((s) => s.name === name);
    if (!p) return;
    patternName = p.name;
    exampleName = "";
    importError = "";
    source = p.source;
    controlValues = {};
    dirty = false; // freshly loaded from the library
    void tick().then(applyEdit);
  }

  function deleteSaved(): void {
    if (device && devicePatternId) {
      if (!window.confirm(`delete "${patternName}" from the device?`)) return;
      void (async () => {
        await device?.deletePattern(devicePatternId);
        devicePatternId = "";
        saveNote = "deleted from device";
        await refreshDevicePatterns();
        setTimeout(() => (saveNote = ""), 2000);
      })();
      return;
    }
    if (!patternName || !saved.some((s) => s.name === patternName)) return;
    if (!window.confirm(`delete "${patternName}" from the library?`)) return;
    saved = deletePattern(patternName);
    saveNote = "deleted";
    setTimeout(() => (saveNote = ""), 2000);
  }

  // ---- mapper (a Luxel *map program*: plot() one point per pixel) ----
  // The map is a real Luxel program running on the VM, so it's edited in the
  // same CodeMirror as patterns and debuggable the same way (breakpoints /
  // stepping). It runs on its own engine (mapEngine); the collected
  // coordinates install into the pattern engine as a 2D/3D map.

  /** Which document the left editor shows: the pattern or the map program. */
  let subTab: "pattern" | "map" = "pattern";
  /** Lazy-mount the map editor on first visit (keeps a single CodeMirror in
   *  the DOM until the map tab is opened). */
  let mapMounted = false;
  $: if (subTab === "map") mapMounted = true;

  let mapSrc = `// Map program — runs once per pixel on the Luxel VM, so it's
// debuggable: set a gutter breakpoint and step through it.
// plot() one point per pixel (units are arbitrary; they normalize).
// This lays the strip out as a ring:
export function render(index) {
  a = index / pixelCount * PI2
  plot(cos(a), sin(a))
}`;
  let mapEditor: Editor;
  let mapEngine: Engine | undefined;
  let mapCompileError: Diagnostic | null = null;
  let mapError = ""; // runtime error from the last map run
  let mapDebugMode = false;
  let mapBreakpoints: number[] = [];
  let mapDbg: DebugSnapshot = { paused: false };
  let mapDebounce: ReturnType<typeof setTimeout> | undefined;

  /** Compile the map program into its own engine. On success runs it (unless
   *  we're debugging — then the user drives it with Run). */
  function recompileMap(autoRun = true): void {
    if (!luxel) return;
    // on a device the map lays out the fixed hardware pixel count (it's a local
    // preview aid); in the playground it's whatever the current layout declares
    const result = luxel.compileMap(mapSrc, device ? devicePixels : pixelCount());
    if (result instanceof Engine) {
      mapEngine?.free();
      mapEngine = result;
      mapCompileError = null;
      if (mapDebugMode) {
        mapEngine.debugEnable(true);
        applyMapBreakpoints();
        mapDbg = { paused: false };
        mapEditor?.setCurrentLine(null);
        if (autoRun) return; // don't auto-run under the debugger; user hits Run
      }
      if (autoRun) runMapNow();
    } else {
      mapCompileError = result; // keep the last good map installed
    }
  }

  /** Run (or resume) the map program; install coords when it finishes, or
   *  surface the debugger when it pauses at a breakpoint. */
  function runMapNow(): void {
    if (!mapEngine) return;
    mapError = "";
    const { paused, coords, dims } = mapEngine.runMap();
    if (paused) {
      mapDbg = mapEngine.debugState();
      mapEditor?.setCurrentLine(mapDbg.line ?? null);
      return;
    }
    const err = mapEngine.takeError();
    if (err) {
      mapError = err.message;
      return;
    }
    installMap(coords, dims);
  }

  function installMap(coords: number[][], _dims: number): void {
    if (coords.length === 0) return;
    layout = { kind: "map", coords };
    recompile(); // local preview only — a layout change never pushes to the device
  }

  function onMapSourceChange(e: CustomEvent<string>): void {
    mapSrc = e.detail;
    clearTimeout(mapDebounce);
    // once a map is installed, live-apply edits; before that (or while
    // debugging) just recompile so errors/breakpoints track without hijacking
    // the layout — the user applies the first time with "run map".
    const autoRun = layout.kind === "map" && !mapDebugMode;
    mapDebounce = setTimeout(() => recompileMap(autoRun), 200);
  }

  function applyMapBreakpoints(): void {
    if (!mapEngine) return;
    const resolved = [...new Set(mapEngine.setBreakpoints(mapBreakpoints))].sort((a, b) => a - b);
    const current = [...new Set(mapBreakpoints)].sort((a, b) => a - b);
    if (JSON.stringify(resolved) !== JSON.stringify(current)) {
      mapBreakpoints = resolved;
      mapEditor?.setBreakpointLines(resolved);
    }
  }

  function onMapBreakpoints(e: CustomEvent<number[]>): void {
    mapBreakpoints = e.detail;
    if (mapBreakpoints.length > 0 && !mapDebugMode) {
      toggleMapDebug(); // placing a breakpoint arms the map debugger
    } else if (mapDebugMode) {
      applyMapBreakpoints();
    }
  }

  function toggleMapDebug(): void {
    mapDebugMode = !mapDebugMode;
    if (!mapEngine) recompileMap(false);
    if (!mapEngine) return;
    mapEngine.debugEnable(mapDebugMode);
    if (mapDebugMode) {
      applyMapBreakpoints();
    } else {
      mapDbg = { paused: false };
      mapEditor?.setCurrentLine(null);
      runMapNow(); // resume live install once debugging is off
    }
  }

  function mapStep(kind: StepKind): void {
    if (!mapEngine || !mapDbg.paused) return;
    const still = mapEngine.debugStep(kind);
    if (still) {
      mapDbg = mapEngine.debugState();
      mapEditor?.setCurrentLine(mapDbg.line ?? null);
    } else {
      mapDbg = { paused: false };
      mapEditor?.setCurrentLine(null);
      const err = mapEngine.takeError();
      if (err) {
        mapError = err.message;
        return;
      }
      installMap(mapEngine.mapResult().coords, mapEngine.mapResult().dims); // run finished
    }
  }

  function mapRequestBreak(): void {
    mapEngine?.debugPause();
  }

  /** Hover inspection for the map editor (its own engine's scope). */
  function mapHoverValue(name: string): string | null {
    if (!mapEngine) return null;
    if (mapDbg.paused) {
      const local = mapDbg.stack?.[0]?.locals.find((l) => l.name === name);
      if (local) return fmtLocal(local);
      const g = mapDbg.globals?.find((g) => g.name === name);
      return g ? fmtLocal(g) : null;
    }
    const g = mapEngine.globals().find((g) => g.name === name);
    return g ? fmtLocal(g) : null;
  }


  // keep the map editor's squiggle in sync with its compile status
  $: if (mapEditor) {
    if (mapCompileError && mapCompileError.start !== undefined && mapCompileError.end !== undefined) {
      mapEditor.setErrorRange({
        from: byteToChar(mapSrc, mapCompileError.start),
        to: byteToChar(mapSrc, mapCompileError.end),
        message: mapCompileError.message,
      });
    } else {
      mapEditor.setErrorRange(null);
    }
  }

  // ---- pattern browser (Patterns tab) ----

  function onGalleryPick(
    e: CustomEvent<{ name: string; kind: "strip" | "grid" | "cloud"; source: string }>,
    home: Tab = "library",
  ): void {
    const p = e.detail;
    openEditor(home); // picking a pattern opens it in the editor
    patternName = p.name;
    exampleName = "";
    importError = "";
    devicePatternId = "";
    if (!device) {
      layout =
        p.kind === "grid"
          ? { kind: "grid", w: 16, h: 16 }
          : p.kind === "cloud"
            ? { kind: "map", coords: cubeLattice(5) } // render3D → rotating cloud
            : { kind: "strip", pixels: 60 };
    }
    source = p.source;
    controlValues = {};
    dirty = false; // freshly picked from the gallery
    void tick().then(applyEdit);
  }

  /** n×n×n lattice map — the default geometry for render3D patterns. */
  function cubeLattice(n: number): number[][] {
    const coords: number[][] = [];
    for (let z = 0; z < n; z++)
      for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) coords.push([x, y, z]);
    return coords;
  }

  // ---- shareable pattern URLs ----
  // The source rides in the fragment (deflate + base64url), so links are
  // self-contained: no server, works on the static playground and pasted
  // between people. `#p=` is compressed, `#ps=` the plain fallback.

  let shareNote = "";

  function b64url(bytes: Uint8Array): string {
    let s = "";
    for (const v of bytes) s += String.fromCharCode(v);
    return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }

  function b64urlDecode(s: string): Uint8Array {
    const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
    return Uint8Array.from(bin, (c) => c.charCodeAt(0));
  }

  async function pipe(data: Uint8Array, stream: GenericTransformStream): Promise<Uint8Array> {
    // our arrays always own their whole buffer, so this cast is sound
    const blob = new Blob([data.buffer as ArrayBuffer]);
    const body = new Response(blob.stream().pipeThrough(stream));
    return new Uint8Array(await body.arrayBuffer());
  }

  async function sharePattern(): Promise<void> {
    // a custom map is part of the look — carry its PROGRAM in the link
    // (JSON envelope under pj=; plain p= stays the mapless format)
    const withMap = layout.kind === "map";
    const payload = withMap ? JSON.stringify({ s: source, m: mapSrc }) : source;
    const key = withMap ? "pj" : "p";
    const bytes = new TextEncoder().encode(payload);
    let frag: string;
    try {
      frag = `${key}=${b64url(await pipe(bytes, new CompressionStream("deflate-raw")))}`;
    } catch {
      frag = `${key}s=${b64url(bytes)}`;
    }
    history.replaceState(null, "", `#${frag}`);
    const url = location.href;
    try {
      await navigator.clipboard.writeText(url);
      shareNote = "link copied";
    } catch {
      // clipboard needs a secure context — a device over plain http isn't
      window.prompt("copy this link:", url);
      shareNote = "link in address bar";
    }
    setTimeout(() => (shareNote = ""), 2500);
  }

  /** Set when a share link carried a map program — the boot path runs it
   *  once the wasm engine is up, restoring the sender's geometry. */
  let sharedMapPending = false;

  /** Restore a `#p=`/`#ps=`/`#pj=`/`#pjs=` fragment; true if one loaded. */
  async function loadFromHash(): Promise<boolean> {
    const m = /^(pj|p)(s?)=([A-Za-z0-9_-]+)$/.exec(location.hash.slice(1));
    const kind = m?.[1];
    const plain = m?.[2] === "s";
    const payload = m?.[3];
    if (!kind || !payload) return false;
    try {
      const data = b64urlDecode(payload);
      const bytes = plain ? data : await pipe(data, new DecompressionStream("deflate-raw"));
      const text = new TextDecoder().decode(bytes);
      if (kind === "pj") {
        const j = JSON.parse(text) as { s: string; m?: string };
        source = j.s;
        if (j.m) {
          mapSrc = j.m;
          sharedMapPending = true;
        }
      } else {
        source = text;
      }
      exampleName = "";
      patternName = "shared pattern";
      return true;
    } catch {
      return false;
    }
  }

  /** PB-style 17-char base-58 id, so exports round-trip into PB tooling. */
  function epeId(): string {
    const chars = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let id = "";
    for (let i = 0; i < 17; i++) id += chars[Math.floor(Math.random() * chars.length)];
    return id;
  }

  function exportEpe(): void {
    const name = patternName || exampleName || "luxel pattern";
    const epe = { name, id: epeId(), sources: { main: source } };
    const blob = new Blob([JSON.stringify(epe, null, 1)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `${name.replace(/[^\w\- ]+/g, "_")}.epe`;
    a.click();
    URL.revokeObjectURL(a.href);
  }

  // ---- layout editing ----

  function setLayoutKind(e: Event): void {
    const kind = (e.target as HTMLSelectElement).value;
    if (kind === "map") {
      // "2D map" is how mapping is enabled: reveal + run the map program (the
      // map sub-tab appears because layout.kind becomes "map"). This is the
      // only enable/disable — switching back to strip/grid turns it off.
      subTab = "map";
      if (!mapEngine) recompileMap(false);
      runMapNow(); // installMap() flips layout → map on success
      if (layout.kind !== "map") recompile();
      return;
    }
    subTab = "pattern";
    // on a device the pixel count is fixed by hardware; layout only rearranges
    const total = device ? devicePixels : pixelCount();
    if (kind === "strip") {
      layout = { kind: "strip", pixels: total };
    } else if (kind === "grid") {
      const side = Math.max(2, Math.round(Math.sqrt(total)));
      layout = { kind: "grid", w: side, h: side };
    }
    recompile(); // rebuild the local preview (a layout change never pushes)
  }

  function setLayoutNum(field: "pixels" | "w" | "h", e: Event): void {
    const v = Math.max(1, Math.min(4096, Number((e.target as HTMLInputElement).value) || 1));
    if (layout.kind === "strip" && field === "pixels") layout = { ...layout, pixels: v };
    if (layout.kind === "grid" && (field === "w" || field === "h")) {
      layout = { ...layout, [field]: v };
    }
    recompile(); // rebuild the local preview (a layout change never pushes)
  }

  function onFpsChange(e: Event): void {
    targetFps = Number((e.target as HTMLSelectElement).value);
  }

  /** Live brightness: the device applies it immediately and persists it. */
  function onBrightnessChange(e: Event): void {
    brightness = Number((e.target as HTMLInputElement).value);
    void device?.setBrightness(brightness);
  }

  /** Save WiFi creds — the device stores them and reboots to apply. */
  function saveWifi(): void {
    const ssid = wifiForm.ssid.trim();
    if (!ssid) return;
    if (!window.confirm(`Save WiFi and reboot the device to join "${ssid}"?`)) return;
    void (async () => {
      wifiNote = "saving…";
      const r = await device?.setWifi(ssid, wifiForm.password);
      if (r?.ok) {
        wifiNote = "saved — the device is rebooting to join the new network";
        wifiSsid = ssid;
        wifiSource = "flash";
      } else {
        wifiNote = r?.error ? `failed: ${r.error}` : "save failed";
      }
    })();
  }

  /** Live LED-protocol change: the device reconfigures its driver (no reboot). */
  function onProtocolChange(e: Event): void {
    const name = (e.target as HTMLSelectElement).value;
    deviceProtocol = name;
    void (async () => {
      const r = await device?.setProtocol(name);
      if (r?.ok && r.protocol) deviceProtocol = r.protocol;
      else if (r?.error) deviceError = `protocol: ${r.error}`;
    })();
  }

  /** Live pixel-count change: the device resizes its strip (no reboot); we
   *  re-anchor the local preview to the new count. */
  function onPixelCountChange(e: Event): void {
    const n = Math.max(1, Math.min(pixelMax, Number((e.target as HTMLInputElement).value) || 1));
    void (async () => {
      const r = await device?.setConfig(n);
      if (r?.ok) {
        devicePixels = r.pixels ?? n;
        // the arrangement resets to a plain strip at the new count (any grid/
        // map was derived from the old count)
        layout = { kind: "strip", pixels: devicePixels };
        subTab = "pattern";
        preview?.clear();
      } else if (r) {
        deviceError = r.error ? `config: ${r.error}` : "config change failed";
      }
    })();
  }

  function togglePause(): void {
    running = !running;
    lastT = 0; // don't integrate the paused time into delta
  }

  function onControlSet(e: CustomEvent<{ name: string; values: number[] }>): void {
    engine?.setControl(e.detail.name, e.detail.values); // drives the local preview
    if (device) void device.setControl(e.detail.name, e.detail.values); // and the strip
  }

  // ---- debugger ----

  /** Install breakpoints and echo the VM-resolved lines back into the
   *  gutter — a click on a comment/blank line snaps to the next executable
   *  line, so the dot always sits where execution will actually stop. */
  function applyBreakpoints(): void {
    if (!engine) return;
    const resolved = [...new Set(engine.setBreakpoints(breakpoints))].sort((a, b) => a - b);
    const current = [...new Set(breakpoints)].sort((a, b) => a - b);
    if (JSON.stringify(resolved) !== JSON.stringify(current)) {
      breakpoints = resolved;
      editor?.setBreakpointLines(resolved);
    }
  }

  function onBreakpoints(e: CustomEvent<number[]>): void {
    breakpoints = e.detail;
    // The device has no stepping debugger — breakpoints must not arm debug
    // mode while connected (it would show a UI that can't actually break).
    if (device) return;
    if (breakpoints.length > 0 && !debugMode) {
      toggleDebug(); // placing a breakpoint arms the debugger
    } else if (debugMode) {
      applyBreakpoints();
    }
  }

  // ---- microphone → sensor patterns (frequencyData etc.) ----
  const mic = new MicSource();
  let micOn = false;
  let micError = "";
  let sensorInFlight = false;
  let lastSensorPush = 0;

  function toggleMic(): void {
    if (micOn) {
      mic.stop();
      micOn = false;
      return;
    }
    void (async () => {
      try {
        await mic.start();
        micOn = true;
        micError = "";
      } catch {
        micError = "microphone unavailable";
        setTimeout(() => (micError = ""), 4000);
      }
    })();
  }

  function toggleDebug(): void {
    debugMode = !debugMode;
    if (!engine) return;
    engine.debugEnable(debugMode);
    if (debugMode) {
      applyBreakpoints();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
    }
  }

  function onPausedRefresh(): void {
    if (!engine) return;
    dbg = engine.debugState();
    editor?.setCurrentLine(dbg.line ?? null);
    preview?.draw(engine.pixels());
  }

  function step(kind: StepKind): void {
    if (!engine || !dbg.paused) return;
    const still = engine.debugStep(kind);
    if (still) {
      onPausedRefresh();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      preview?.draw(engine.pixels());
    }
  }

  function requestBreak(): void {
    engine?.debugPause(); // takes effect at the next executed instruction
  }

  function fmtRaw(raw: number): string {
    return (raw / 65536).toFixed(4).replace(/\.?0+$/, "") || "0";
  }

  function fmtLocal(l: { raw?: number; array?: number; fn?: number }): string {
    if (l.raw !== undefined) return fmtRaw(l.raw);
    if (l.array !== undefined) return `array[${l.array}]`;
    return `fn#${l.fn}`;
  }

  /** Hover inspection: paused → locals (shadowing globals) then globals;
   *  running → live globals. */
  function hoverValue(name: string): string | null {
    if (device) {
      const v = vars[name];
      return typeof v === "number" ? v.toFixed(4).replace(/\.?0+$/, "") || "0" : null;
    }
    if (!engine) return null;
    if (dbg.paused) {
      const local = dbg.stack?.[0]?.locals.find((l) => l.name === name);
      if (local) return fmtLocal(local);
      const g = dbg.globals?.find((g) => g.name === name);
      return g ? fmtLocal(g) : null;
    }
    const g = engine.globals().find((g) => g.name === name);
    return g ? fmtLocal(g) : null;
  }

  /** Diagnostic spans are UTF-8 byte offsets; CodeMirror wants char offsets. */
  function byteToChar(text: string, byte: number): number {
    const bytes = new TextEncoder().encode(text);
    return new TextDecoder().decode(bytes.subarray(0, Math.min(byte, bytes.length))).length;
  }

  // keep the squiggle in sync with the compile status
  $: if (editor) {
    if (compileError && compileError.start !== undefined && compileError.end !== undefined) {
      editor.setErrorRange({
        from: byteToChar(source, compileError.start),
        to: byteToChar(source, compileError.end),
        message: compileError.message,
      });
    } else {
      editor.setErrorRange(null);
    }
  }

  function jumpToError(): void {
    if (compileError) editor.jumpTo(compileError.line, compileError.col);
  }

  // ---- render loop ----

  function loop(t: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || !running) return;
    if (dbg.paused) return; // suspended at a debug stop — step buttons drive
    const minInterval = targetFps > 0 ? 1000 / targetFps - 1 : 0;
    if (lastT !== 0 && t - lastT < minInterval) return;
    const dt = lastT === 0 ? 1000 / (targetFps || 60) : Math.min(t - lastT, 200);
    lastT = t;
    if (micOn) {
      const sf = mic.frame();
      engine.setSensors(sf);
      // in device mode, the mic also stands in for the physical sensor
      // board: stream frames to the strip (throttled, one in flight)
      if (device && !sensorInFlight && t - lastSensorPush > 50) {
        lastSensorPush = t;
        sensorInFlight = true;
        device
          .sendSensors(toSensorBoardFrame(sf))
          .catch(() => {})
          .finally(() => (sensorInFlight = false));
      }
    }
    const px = engine.frame(dt);
    if (engine.debugPaused()) {
      onPausedRefresh();
      return;
    }
    preview?.draw(px);
    fps = fps * 0.9 + (1000 / Math.max(dt, 1)) * 0.1;
    const err = engine.takeError();
    if (err) runtimeError = err;
    if (t - lastPoll > 250) {
      lastPoll = t;
      vars = engine.vars();
      const r = new Map<string, number>();
      for (const c of controls) {
        if (c.kind === "showNumber" || c.kind === "gauge") {
          const v = engine.setControl(c.name, []);
          if (v !== null) r.set(c.name, v);
        }
      }
      readouts = r;
    }
  }

  /** How we bind to a device (a plain playground binds to none):
   *   1. `?device=<base>` — a dev/e2e override pointing the built UI at a
   *      device or the native mirror (known synchronously).
   *   2. served-from-device — the UI loaded from the device's own flash, so the
   *      device is this same origin (probe `/api/status`; a dev server's SPA
   *      fallback returns 200 HTML, so require a genuine device JSON shape). */
  async function detectDeviceBase(): Promise<string | null> {
    const override = new URLSearchParams(location.search).get("device");
    if (override !== null) return override.trim().replace(/\/+$/, "");
    try {
      const ctl = new AbortController();
      const t = setTimeout(() => ctl.abort(), 1500);
      const r = await fetch("/api/status", { signal: ctl.signal });
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

  onMount(async () => {
    // Probe for a device in parallel with the wasm load, so we can go straight
    // into device mode without ever flashing the playground first.
    const deviceProbe = detectDeviceBase();
    try {
      luxel = await Luxel.load(`${import.meta.env.BASE_URL}luxel.wasm`);
    } catch (e) {
      loadFailure = `failed to load luxel.wasm: ${String(e)}`;
      booting = false;
      return;
    }
    // Probe for a local corpus gallery; its tab appears only when present.
    void fetch(`${import.meta.env.BASE_URL}pixelblaze-library.json`)
      .then((r) => (r.ok ? r.json() : null))
      .then((list) => {
        hasPixelblazeLibrary = Array.isArray(list) && list.length > 0;
      })
      .catch(() => {});
    // In-progress work wins: a share link's pattern, else the autosaved
    // working copy. The editor opens on it (resume — never lose edits). A
    // device, though, only resumes it when it has *unsaved changes*; a clean
    // copy defers to whatever pattern is actually running on the device.
    const fromHash = await loadFromHash();
    let hadWip = fromHash;
    let wipDirty = fromHash; // a shared link is itself an unsaved edit to resume
    if (!fromHash) {
      const wc = loadWorkingCopy();
      if (wc) {
        source = wc.source;
        layout = wc.layout;
        patternName = wc.patternName;
        exampleName = wc.exampleName;
        dirty = wc.dirty;
        hadWip = true; // the playground always resumes the last working copy
        wipDirty = wc.dirty; // the device only resumes it if it's genuinely dirty
      }
    }
    // a share link never auto-connects to a device
    const base = fromHash ? null : await deviceProbe;

    if (base !== null) {
      // Device mode: keep the boot cover up (with device-aware text) through the
      // whole handshake so the running pattern is loaded before anything shows.
      bootLabel = "opening the pattern running on the device…";
      deviceBase = base;
      tab = "device"; // the editor's back button lands on Device Patterns
      editing = true;
      recompile(); // a local engine so the boot cover lifts onto a live preview
      raf = requestAnimationFrame(loop);
      // A genuinely-unsaved edit is resumed AND pushed so the device runs it too
      // (editor, preview and device all agree). A clean copy instead opens
      // whatever pattern is currently active on the device.
      await connectDevice(base, /* pullPattern */ !wipDirty);
      recompile(); // rebuild the preview from the pulled/resumed source
      if (wipDirty && device) await devicePush();
    } else {
      recompile();
      if (sharedMapPending) {
        // the link carried a map program: run it to restore the geometry
        sharedMapPending = false;
        mapMounted = true;
        recompileMap(true);
      }
      raf = requestAnimationFrame(loop);
      editing = hadWip; // resume in the editor if there was work in progress
    }
    booting = false;
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    clearTimeout(debounce);
    clearTimeout(pushDebounce);
    clearTimeout(mapDebounce);
    clearTimeout(playlistDebounce);
    clearInterval(playlistPoll);
    engine?.free();
    mapEngine?.free();
    mic.stop();
  });
</script>

<svelte:window on:click={() => (menuOpen = false)} on:keydown={onKeydown} />

<div
  class="shell"
  data-mode={mode}
  data-tab={tab}
  role="application"
  on:drop={onDrop}
  on:dragover|preventDefault
>
  {#if booting}
    <!-- first-load cover: nothing renders behind it, so the playground never
         flashes before we switch into device mode + load the running pattern -->
    <div class="boot" data-role="boot">
      <span class="spinner"></span>
      <span class="boot-label" data-role="boot-label">{bootLabel}</span>
    </div>
  {/if}
  <header>
    {#if editing}
      <button
        data-role="editor-back"
        class="back"
        on:click={closeEditor}
        title={`back to ${backLabel}`}
      >
        ← {backLabel}
      </button>
      <span class="pattern-name" data-role="pattern-name">
        {patternName || exampleName || "untitled pattern"}
      </span>
    {:else}
      <span class="wordmark">
        luxel <span class="dim">{isPlayground ? "playground" : (device?.base ?? deviceBase) || "device"}</span>
      </span>
      <nav class="tabs" data-role="tabs">
        <button
          data-role="tab-library"
          class="tab"
          class:active={tab === "library"}
          on:click={() => (tab = "library")}
        >
          Patterns Library
        </button>
        {#if hasPixelblazeLibrary}
          <button
            data-role="tab-pixelblaze"
            class="tab"
            class:active={tab === "pixelblaze"}
            on:click={() => (tab = "pixelblaze")}
          >
            PixelBlaze Library
          </button>
        {/if}
        {#if !isPlayground}
          <button
            data-role="tab-device"
            class="tab"
            class:active={tab === "device"}
            on:click={() => (tab = "device")}
          >
            Device Patterns
          </button>
        {/if}
        {#if device}
          <button
            data-role="tab-playlist"
            class="tab"
            class:active={tab === "playlist"}
            on:click={() => {
              tab = "playlist";
              void refreshPlaylist();
            }}
          >
            Playlist
          </button>
          <button
            data-role="tab-settings"
            class="tab"
            class:active={tab === "settings"}
            on:click={() => (tab = "settings")}
          >
            Settings
          </button>
        {/if}
      </nav>
    {/if}

    <span class="spacer"></span>

    <span class="mono dim" data-role="fps">{fps.toFixed(0)} fps</span>
  </header>

  <!-- ───────────── Editor tab ───────────── -->
  <main class="editor-view" hidden={!editing}>
    {#if patternLoading}
      <!-- cover the editor while a pattern is being fetched/activated so the
           previously-open script never flashes before the real one loads -->
      <div class="pattern-loading" data-role="pattern-loading">
        <span class="spinner"></span>
        {device ? "loading the pattern from the device…" : "loading pattern…"}
      </div>
    {/if}
    <section class="left">
      <!-- File actions live in a toolbar fixed above the editor (not in the
           header next to the device connection) — they act on the pattern
           being edited. -->
      <div class="editor-toolbar" data-role="editor-toolbar">
        <button
          data-role="save"
          title={device ? "save the current pattern on the device" : "save to this browser's library"}
          on:click={saveToLibrary}
        >
          save
        </button>
        {#if devicePatternId !== "" || saved.some((s) => s.name === patternName && exampleName === "")}
          <button
            data-role="delete"
            title={devicePatternId ? "remove from the device" : "remove from the library"}
            on:click={deleteSaved}
          >
            delete
          </button>
        {/if}
        {#if isPlayground}
          <button
            data-role="share"
            class="primary"
            title="copy a link that carries this pattern in the URL"
            on:click={() => void sharePattern()}
          >
            share
          </button>
        {/if}
        {#if device && devicePatternId}
          <button
            data-role="add-to-playlist"
            title="add this pattern (with its current parameters) to the playlist"
            on:click={addToPlaylist}
          >
            + playlist
          </button>
        {/if}
        <span class="overflow">
          <button
            class="more"
            data-role="overflow"
            title="more actions"
            aria-label="more actions"
            on:click|stopPropagation={() => (menuOpen = !menuOpen)}
          >
            ⋯
          </button>
          {#if menuOpen}
            <div class="menu" role="menu">
              <button data-role="epe-import" role="menuitem" on:click={() => fileInput.click()}>
                import .epe…
              </button>
              <button data-role="epe-export" role="menuitem" on:click={exportEpe}>
                export .epe
              </button>
            </div>
          {/if}
        </span>
        {#if saveNote}<span class="dim note" data-role="save-note">{saveNote}</span>{/if}
        {#if shareNote}<span class="dim note" data-role="share-note">{shareNote}</span>{/if}
        <input
          class="file-input"
          type="file"
          accept=".epe,.json,application/json"
          bind:this={fileInput}
          on:change={onImportPick}
        />
      </div>

      {#if layout.kind === "map"}
        <div class="subtabs" data-role="editor-subtabs">
          <button
            data-role="subtab-pattern"
            class="subtab"
            class:active={subTab === "pattern"}
            on:click={() => (subTab = "pattern")}
          >
            pattern
          </button>
          <button
            data-role="subtab-map"
            class="subtab"
            class:active={subTab === "map"}
            on:click={() => {
              subTab = "map";
              if (!mapEngine) recompileMap(!mapDebugMode);
            }}
          >
            map
          </button>
        </div>
      {/if}

      <div class="editor-host">
        <div class="editor-slot" hidden={subTab !== "pattern"}>
          <Editor
            bind:this={editor}
            value={source}
            {hoverValue}
            on:change={onSourceChange}
            on:breakpoints={onBreakpoints}
          />
        </div>
        {#if mapMounted && layout.kind === "map"}
          <div class="editor-slot" data-role="map-editor" hidden={subTab !== "map"}>
            <Editor
              bind:this={mapEditor}
              value={mapSrc}
              hoverValue={mapHoverValue}
              on:change={onMapSourceChange}
              on:breakpoints={onMapBreakpoints}
            />
          </div>
        {/if}
      </div>

      <div class="playback">
        <!-- Layout controls how the preview is arranged. In the playground it
             also sets how many pixels the local engine runs; on a device the
             pixel count is fixed by hardware and this only rearranges the live
             stream (strip row / grid / 2D map) — "2D map" is a local preview
             aid, not uploaded to the device. Choosing "2D map" reveals the
             pattern·map sub-tabs and runs the map program. -->
        <select value={layout.kind} data-role="layout-kind" on:change={setLayoutKind}>
          <option value="strip">strip</option>
          <option value="grid">grid</option>
          <option value="map">2D map</option>
        </select>
        {#if layout.kind === "strip"}
          <input
            class="num"
            data-role="layout-px"
            type="number"
            min="1"
            max="4096"
            value={layout.pixels}
            disabled={!isPlayground}
            title={isPlayground ? "pixel count" : "fixed by the device's hardware"}
            on:change={(e) => setLayoutNum("pixels", e)}
          />
          <span class="dim">px</span>
        {:else if layout.kind === "grid"}
          <input
            class="num"
            data-role="layout-w"
            type="number"
            min="1"
            max="256"
            value={layout.w}
            on:change={(e) => setLayoutNum("w", e)}
          />
          <span class="dim">×</span>
          <input
            class="num"
            data-role="layout-h"
            type="number"
            min="1"
            max="256"
            value={layout.h}
            on:change={(e) => setLayoutNum("h", e)}
          />
        {:else}
          <span class="dim mono" data-role="map-badge">{layout.coords.length} px mapped</span>
        {/if}
        {#if subTab === "map"}
          <button data-role="map-run" title="run the map program and install it" on:click={runMapNow}>
            run map
          </button>
          {#if device && layout.kind === "map"}
            <button
              data-role="map-install"
              title="upload this map to the device so its patterns render in 2D/3D"
              on:click={installDeviceMap}
            >
              install on device
            </button>
            {#if deviceMap.installed}
              <button
                data-role="map-clear"
                title="remove the map from the device"
                on:click={clearDeviceMap}
              >
                clear device map
              </button>
              <span class="dim mono" data-role="map-installed">
                {deviceMap.count}px {deviceMap.dims}D on device
              </span>
            {/if}
          {/if}
          {#if mapError}<span class="mapper-error" data-role="map-error">{mapError}</span>{/if}
        {/if}
        <span class="sep"></span>
        <select value={targetFps} on:change={onFpsChange}>
          <option value={0}>max fps</option>
          <option value={60}>60 fps</option>
          <option value={30}>30 fps</option>
          <option value={15}>15 fps</option>
          <option value={5}>5 fps</option>
        </select>
        <button data-role="pause" on:click={togglePause} title={running ? "pause" : "resume"}>
          {running ? "pause" : "play"}
        </button>
        {#if subTab === "map"}
          <button
            class="debug-toggle"
            class:active={mapDebugMode}
            data-role="map-debug"
            title="toggle the map debugger"
            on:click={toggleMapDebug}
          >
            debug
          </button>
        {:else}
          <!-- the preview runs on the local engine (even on a device), so the
               step-debugger works everywhere -->
          <button
            class="debug-toggle"
            class:active={micOn}
            data-role="mic-toggle"
            title="feed microphone audio to sensor patterns (frequencyData, energyAverage, maxFrequency)"
            on:click={toggleMic}
          >
            sound
          </button>
          {#if micError}<span class="mapper-error" data-role="mic-error">{micError}</span>{/if}
          <button
            class="debug-toggle"
            class:active={debugMode}
            data-role="debug"
            title="toggle debugger"
            on:click={toggleDebug}
          >
            debug
          </button>
        {/if}
      </div>
    </section>
    <section class="right">
      {#if loadFailure}
        <div class="banner error">{loadFailure}</div>
      {/if}
      {#if deviceError}
        <div class="banner error">{deviceError}</div>
      {/if}
      {#if importError}
        <div class="banner error" data-role="import-error">
          {importError}
          <button class="dismiss" on:click={() => (importError = "")}>×</button>
        </div>
      {/if}
      {#if compileError && subTab === "pattern"}
        <button class="banner error as-button" on:click={jumpToError}>
          line {compileError.line}:{compileError.col} — {compileError.message}
        </button>
      {/if}
      {#if mapCompileError && subTab === "map"}
        <button
          class="banner error as-button"
          data-role="map-compile-error"
          on:click={() =>
            mapCompileError && mapEditor?.jumpTo(mapCompileError.line, mapCompileError.col)}
        >
          map line {mapCompileError.line}:{mapCompileError.col} — {mapCompileError.message}
        </button>
      {/if}
      {#if runtimeError && !compileError && subTab === "pattern"}
        <div class="banner warn">
          runtime: {runtimeError.message}
          <button class="dismiss" on:click={() => (runtimeError = null)}>×</button>
        </div>
      {/if}

      {#if subTab === "map" && mapDebugMode}
        <Debugger
          snapshot={mapDbg}
          runningHint="set a gutter breakpoint, then Run map to step through it"
          on:step={(e) => mapStep(e.detail)}
          on:break={mapRequestBreak}
        />
      {:else if debugMode && subTab === "pattern"}
        <Debugger snapshot={dbg} on:step={(e) => step(e.detail)} on:break={requestBreak} />
      {/if}

      <div class="preview-wrap">
        <Preview bind:this={preview} {layout} />
      </div>

      <h2>Controls</h2>
      <Controls {controls} bind:values={controlValues} {readouts} {hints} on:set={onControlSet} />
      {#if controls.length === 0}
        <p class="dim hint">
          export <code>function sliderName(v)</code> to add controls — bound them with
          <code>//# min=0 max=5 step=0.5 default=2</code>
        </p>
      {/if}

      {#if layout.kind === "map"}
        <h2>Map</h2>
        <p class="dim hint">
          A {pixelTotal}-point map is installed. Edit it in the
          <button class="link" data-role="goto-map" on:click={() => (subTab = "map")}>map</button>
          sub-tab — it's a debuggable Luxel program (<code>plot(x, y)</code> per pixel).{" "}
          {#if device}It only arranges this preview — it isn't uploaded to the device.{/if} Choose
          a different layout to turn mapping off.
        </p>
      {/if}

      <h2>Vars</h2>
      <VarWatcher {vars} />
      {#if Object.keys(vars).length === 0}
        <p class="dim hint">export <code>var name</code> to watch values here</p>
      {/if}
    </section>
  </main>

  <!-- ───────────── Patterns Library tab ───────────── -->
  <div class="library-tab" data-role="library-panel" hidden={editing || tab !== "library"}>
    <div class="lib-head">
      <span class="lib-title">Patterns Library</span>
      <span class="dim">examples &amp; community patterns{saved.length ? " · your saved" : ""}</span>
      <span class="spacer"></span>
      <button class="primary" data-role="new-pattern" on:click={() => newPattern(false)}>
        + New pattern
      </button>
    </div>
    {#if saved.length > 0}
      <div class="saved-row">
        <span class="dim">your patterns:</span>
        {#each saved as s (s.name)}
          <button class="chip" data-role="saved-pattern" on:click={() => openSavedPattern(s.name)}>
            {s.name}
          </button>
        {/each}
      </div>
    {/if}
    <div class="lib-gallery">
      {#if galleryMounted && luxel}
        <Gallery {luxel} on:pick={onGalleryPick} />
      {:else}
        <div class="tab-empty dim">loading patterns…</div>
      {/if}
    </div>
  </div>

  <!-- ───────────── PixelBlaze Library tab (local corpus only) ───────────── -->
  {#if hasPixelblazeLibrary}
    <div class="library-tab" data-role="pixelblaze-panel" hidden={editing || tab !== "pixelblaze"}>
      <div class="lib-head">
        <span class="lib-title">PixelBlaze Library</span>
        <span class="dim">original Pixelblaze community patterns (local corpus)</span>
        <span class="spacer"></span>
      </div>
      <div class="lib-gallery">
        {#if pixelblazeMounted && luxel}
          <Gallery
            {luxel}
            src="pixelblaze-library.json"
            emptyNote="corpus unavailable (no pixelblaze-library.json)"
            on:pick={(e) => onGalleryPick(e, "pixelblaze")}
          />
        {:else}
          <div class="tab-empty dim">loading patterns…</div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- ───────────── Device Patterns tab ───────────── -->
  {#if !isPlayground}
    <div class="device-tab" data-role="device-panel" hidden={editing || tab !== "device"}>
      <div class="lib-head">
        <span class="lib-title">Device Patterns</span>
        <span class="dim">stored in the device's memory</span>
        <span class="spacer"></span>
        <button
          class="primary"
          data-role="device-new-pattern"
          disabled={!device}
          on:click={() => newPattern(true)}
        >
          + New pattern
        </button>
      </div>
      {#if !device}
        <p class="dim hint" data-role="device-offline">
          device unreachable — {deviceError || "reload to retry"}.
        </p>
      {:else if devicePatterns.length === 0}
        <p class="dim hint">no patterns stored on the device yet. Create one with “+ New pattern”.</p>
      {:else}
        <ul class="dev-list">
          {#each devicePatterns as p (p.id)}
            <li>
              <button
                class="dev-item"
                data-role="device-pattern"
                class:active={p.id === devicePatternId}
                on:click={() => openDevicePatternInEditor(p.id)}
              >
                {#if luxel}
                  <PatternThumb {luxel} source={p.source} />
                {/if}
                <span class="dev-name">{p.name}</span>
                <span class="dim">edit ›</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  <!-- ───────────── Playlist tab (device mode only) ───────────── -->
  {#if !isPlayground}
    <div class="playlist-tab" data-role="playlist-panel" hidden={editing || tab !== "playlist"}>
      <div class="lib-head">
        <span class="lib-title">Playlist</span>
        <span class="dim">plays your saved patterns in order</span>
        <span class="spacer"></span>
        <span class="pl-transport">
          {#if playlist.playing}
            <button data-role="pl-prev" title="previous" on:click={playlistPrev}>⏮</button>
            <button data-role="pl-stop" on:click={playlistStop}>■ stop</button>
            <button data-role="pl-next" title="next" on:click={playlistNext}>⏭</button>
          {:else}
            <button
              class="primary"
              data-role="pl-play"
              disabled={!device || playlist.items.length === 0}
              on:click={playlistPlay}
            >
              ▶ play
            </button>
          {/if}
          {#if playlist.items.length > 0}
            <button data-role="pl-clear" title="remove all items" on:click={clearPlaylist}>
              clear
            </button>
          {/if}
        </span>
      </div>

      <div class="pl-default">
        <span class="dim">default duration</span>
        <input
          class="num"
          data-role="pl-default-sec"
          type="number"
          min="0"
          placeholder="manual"
          value={playlist.defaultSec || ""}
          on:change={onDefaultSecChange}
        />
        <span class="dim">seconds (blank/0 = manual advance) · items can override</span>
        {#if playlist.items.length > 0}
          <span class="spacer"></span>
          <span class="dim" data-role="pl-total">
            {playlist.items.length} item{playlist.items.length === 1 ? "" : "s"} · loop ≈ {fmtDuration(
              playlistTotalSec,
            )}{playlistHasManual ? " + manual stops" : ""}
          </span>
        {/if}
      </div>

      <div class="pl-default">
        <span class="dim">crossfade</span>
        <input
          class="num"
          data-role="pl-crossfade"
          type="number"
          min="0"
          step="0.1"
          placeholder="0"
          value={playlist.crossfadeMs ? playlist.crossfadeMs / 1000 : ""}
          on:change={onCrossfadeChange}
        />
        <span class="dim">seconds to blend between items (blank/0 = hard cut)</span>
      </div>

      {#if !device}
        <p class="dim hint">device unreachable — {deviceError || "reload to retry"}.</p>
      {:else if playlist.items.length === 0}
        <p class="dim hint" data-role="pl-empty">
          Empty. Open a saved device pattern in the editor, set its parameters, and use
          <strong>“+ Add to playlist”</strong> — add the same pattern more than once for different
          looks.
        </p>
      {:else}
        <ul class="pl-list">
          {#each playlist.items as item, i (i)}
            {#if luxel}
              <PlaylistRow
                {luxel}
                source={itemSource(item.id)}
                {item}
                defaultSec={playlist.defaultSec}
                missing={itemMissing(item.id)}
                active={playlist.playing && playlist.index === i}
                first={i === 0}
                last={i === playlist.items.length - 1}
                on:change={() => {
                  playlist = playlist;
                  queuePlaylistSave();
                }}
                on:remove={() => removePlaylistItem(i)}
                on:move={(e) => movePlaylistItem(i, e.detail)}
                on:dragstart={() => (playlistDragFrom = i)}
                on:drop={() => dropPlaylistItem(i)}
              />
            {/if}
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  <!-- ───────────── Settings tab (device mode only) ───────────── -->
  {#if device}
    <div class="settings-tab" data-role="settings-panel" hidden={editing || tab !== "settings"}>
      <div class="settings">
        <h1>Device settings</h1>

        <section class="card">
          <h2>Device</h2>
          <div class="field">
            <span class="flabel">Address</span>
            <input class="mono grow" value={device.base || "served from device"} disabled />
          </div>
          <div class="field">
            <span class="flabel">Pixels</span>
            <input
              class="num"
              data-role="cfg-pixels"
              type="number"
              min="1"
              max={pixelMax}
              value={devicePixels}
              on:change={onPixelCountChange}
            />
            <span class="dim">resized live — max {pixelMax}, no reboot</span>
          </div>
          <div class="field">
            <span class="flabel">LED protocol</span>
            <select data-role="cfg-protocol" value={deviceProtocol} on:change={onProtocolChange}>
              {#each protocolOptions as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
            <span class="dim">match your strip — switched live (no reboot)</span>
          </div>
          <div class="field">
            <span class="flabel">Status</span>
            <span class="mono dim">
              {fps.toFixed(0)} fps (local preview){runtimeError
                ? ` · vmerr: ${runtimeError.message}`
                : ""}
            </span>
          </div>
        </section>

        <section class="card">
          <h2>Network input</h2>
          <div class="field">
            <span class="flabel">Status</span>
            <span class="mono" data-role="netin-status">
              {netLive === "ddp" ? "receiving DDP" : netLive === "e131" ? "receiving E1.31" : "idle"}
            </span>
          </div>
          <p class="dim hint">
            The device listens for DDP on UDP :4048 and E1.31/sACN on UDP :5568 (universe 1 and
            up, 170 pixels each; multicast or unicast). Frames from xLights, LedFx, Resolume, etc.
            drive the strip directly; the running pattern resumes a couple of seconds after the
            stream stops.
          </p>
        </section>

        <section class="card">
          <h2>Brightness</h2>
          <div class="field">
            <input
              type="range"
              class="grow"
              data-role="brightness"
              min="0"
              max={brightnessMax}
              step="1"
              value={brightness}
              on:input={onBrightnessChange}
            />
            <span class="mono dim" data-role="brightness-val">{brightness}/{brightnessMax}</span>
          </div>
          <p class="dim hint">
            Global output brightness (the LED driver's current limiter). Applied live and saved on
            the device. It dims the physical strip, not the preview above (which shows the pattern's
            colors at full range).
          </p>
        </section>

        <section class="card">
          <h2>WiFi</h2>
          <div class="field">
            <span class="flabel">Current</span>
            <span class="mono" data-role="wifi-current">
              {wifiSsid ?? "—"}
              <span class="dim">
                ({wifiSource === "flash"
                  ? "saved"
                  : wifiSource === "builtin"
                    ? "compiled-in"
                    : "none"})
              </span>
            </span>
          </div>
          <div class="field">
            <span class="flabel">Network</span>
            <input class="grow" data-role="wifi-ssid" placeholder="SSID" bind:value={wifiForm.ssid} />
          </div>
          <div class="field">
            <span class="flabel">Password</span>
            <input
              class="grow"
              data-role="wifi-pass"
              type="password"
              placeholder="password"
              bind:value={wifiForm.password}
            />
          </div>
          <div class="field">
            <button
              class="primary"
              data-role="wifi-save"
              disabled={!device || !wifiForm.ssid.trim()}
              on:click={saveWifi}
            >
              save &amp; reboot
            </button>
            {#if wifiNote}<span class="dim" data-role="wifi-note">{wifiNote}</span>{/if}
          </div>
          <p class="dim hint">
            The device stores the credentials in flash and <strong>reboots</strong> to join the new
            network. A device with no way onto any network boots as an open access point
            (<span class="mono">luxel-xxxx</span> → <span class="mono">http://192.168.4.1/</span>)
            where this same page provisions it.
          </p>
          <div class="field">
            <button data-role="apmode" on:click={startApMode}>reboot into setup AP</button>
            <span class="dim">
              one boot only — good for re-provisioning; it comes back as a station afterwards
            </span>
            {#if apNote}<span class="dim" data-role="apmode-note">{apNote}</span>{/if}
          </div>
        </section>

        <section class="card">
          <h2>Output</h2>
          {#if outputStatus}
            <div class="field">
              <span class="flabel">Color order</span>
              <select
                data-role="out-order"
                bind:value={outputStatus.order}
                on:change={onOutputChange}
              >
                {#each ["rgb", "rbg", "grb", "gbr", "brg", "bgr"] as o}
                  <option value={o}>{o.toUpperCase()}</option>
                {/each}
              </select>
              <span class="dim">match your strip's wiring (colors swapped? try GRB/BGR)</span>
            </div>
            <div class="field">
              <span class="flabel">Gamma</span>
              <input
                class="num"
                data-role="out-gamma"
                type="number"
                min="0"
                max="5"
                step="0.1"
                value={outputStatus.gamma / 10}
                on:change={(e) => {
                  if (outputStatus)
                    outputStatus.gamma = Math.round(Number(e.currentTarget.value) * 10);
                  onOutputChange();
                }}
              />
              <span class="dim">0 = off; 2.2 gives smoother dark fades on the strip</span>
            </div>
            <div class="field">
              <span class="flabel">Power cap</span>
              <input
                class="num"
                data-role="out-cap"
                type="number"
                min="0"
                max="20000"
                step="100"
                bind:value={outputStatus.capMa}
                on:change={onOutputChange}
              />
              <span class="dim">mA — frames estimated above this get scaled down; 0 = off</span>
            </div>
          {:else}
            <p class="dim hint">not available on this firmware</p>
          {/if}
        </section>

        <section class="card">
          <h2>Clock</h2>
          <div class="field">
            <span class="flabel">Device time</span>
            <span class="mono" data-role="clock-status">
              {clockStatus?.synced ? fmtDeviceTime(clockStatus.local) : "not NTP-synced yet"}
            </span>
          </div>
          <div class="field">
            <span class="flabel">UTC offset</span>
            <input
              class="num"
              data-role="clock-tz"
              type="number"
              step="0.5"
              min="-14"
              max="14"
              value={(clockStatus?.tzMinutes ?? 0) / 60}
              on:change={onTzChange}
            />
            <span class="dim">hours (e.g. -6 for Mountain DST) — drives clockHour() patterns</span>
          </div>
        </section>

        <section class="card">
          <h2>Multi-device sync</h2>
          <div class="field">
            <span class="flabel">Role</span>
            <select
              data-role="sync-mode"
              value={syncStatus?.mode ?? "off"}
              on:change={onSyncModeChange}
            >
              <option value="off">off</option>
              <option value="leader">leader</option>
              <option value="follower">follower</option>
            </select>
            <span class="mono dim" data-role="sync-status">
              {syncStatus?.mode === "follower"
                ? syncStatus.leader
                  ? `following (offset ${syncStatus.leader.offsetMs}ms)`
                  : "waiting for a leader…"
                : syncStatus?.mode === "leader"
                  ? "broadcasting the timebase"
                  : ""}
            </span>
          </div>
          <p class="dim hint">
            Run the same pattern on several Luxels and they stay phase-locked: one device leads
            (broadcasting its clock on UDP :4049) and the rest follow. The leader also relays its
            sensor data, so one microphone can drive every strip.
          </p>
        </section>

        <section class="card">
          <h2>MQTT / Home Assistant</h2>
          <div class="field">
            <span class="flabel">Status</span>
            <span class="mono" data-role="mqtt-status">
              {mqttStatus?.connected
                ? "connected"
                : mqttStatus?.enabled
                  ? "not connected"
                  : "disabled"}
            </span>
          </div>
          <div class="field">
            <span class="flabel">Broker</span>
            <input
              class="grow"
              data-role="mqtt-host"
              placeholder="host or IP (blank = disable)"
              bind:value={mqttForm.host}
            />
            <input
              class="num"
              data-role="mqtt-port"
              type="number"
              min="1"
              max="65535"
              bind:value={mqttForm.port}
            />
          </div>
          <div class="field">
            <span class="flabel">User</span>
            <input class="grow" data-role="mqtt-user" placeholder="optional" bind:value={mqttForm.user} />
          </div>
          <div class="field">
            <span class="flabel">Password</span>
            <input
              class="grow"
              data-role="mqtt-pass"
              type="password"
              placeholder={mqttStatus?.hasPass ? "(saved — retype to change)" : "optional"}
              bind:value={mqttForm.pass}
            />
          </div>
          <div class="field">
            <button class="primary" data-role="mqtt-save" disabled={!device} on:click={saveMqtt}>
              save
            </button>
            {#if mqttNote}<span class="dim" data-role="mqtt-note">{mqttNote}</span>{/if}
          </div>
          <p class="dim hint">
            Point this at your MQTT broker (e.g. the Home Assistant Mosquitto add-on) and the
            device shows up in HA automatically: a light (power + brightness) and a pattern
            selector for the device library. Applied live, no reboot. Saving stores exactly what's
            entered — including a blank password.
          </p>
        </section>

        <section class="card">
          <h2>Pattern library</h2>
          <p class="dim hint">
            {devicePatterns.length} pattern{devicePatterns.length === 1 ? "" : "s"} stored on the
            device. Manage them from the
            <button class="link" on:click={() => (tab = "device")}>Device Patterns</button> tab.
          </p>
        </section>
      </div>
    </div>
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
    flex-wrap: wrap;
  }

  .wordmark {
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--accent);
  }

  .dim {
    color: var(--text-dim);
  }

  .tabs {
    display: flex;
    gap: 2px;
  }

  .tab {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 6px 12px;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .num {
    width: 64px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .editor-view {
    position: relative;
    display: grid;
    grid-template-columns: minmax(360px, 1fr) minmax(320px, 420px);
    flex: 1;
    min-height: 0;
  }

  .pattern-loading {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-dim);
    background: var(--bg);
  }

  /* first-load cover over the whole app (header included) */
  .boot {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: var(--bg);
    color: var(--text-dim);
    font-size: 15px;
  }

  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .editor-view[hidden],
  .library-tab[hidden],
  .device-tab[hidden],
  .playlist-tab[hidden],
  .settings-tab[hidden] {
    display: none;
  }

  .library-tab,
  .device-tab,
  .playlist-tab,
  .settings-tab {
    flex: 1;
    min-height: 0;
    background: var(--bg-panel);
  }

  .library-tab {
    display: flex;
    flex-direction: column;
  }

  .device-tab,
  .playlist-tab,
  .settings-tab {
    overflow-y: auto;
  }

  .pl-transport {
    display: inline-flex;
    gap: 6px;
  }

  .pl-default {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }

  .pl-list {
    list-style: none;
    margin: 0;
    padding: 12px 16px;
    max-width: 680px;
  }

  .lib-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .lib-title {
    font-weight: 700;
    letter-spacing: 0.03em;
    color: var(--accent);
    font-size: 14px;
  }

  .saved-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
  }

  .chip {
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 999px;
  }

  .lib-gallery {
    flex: 1;
    min-height: 0;
  }

  .dev-list {
    list-style: none;
    margin: 0;
    padding: 8px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 620px;
  }

  .dev-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    cursor: pointer;
  }

  .dev-item:hover {
    border-color: var(--accent);
  }

  .dev-item.active {
    border-color: var(--accent);
  }

  .dev-name {
    flex: 1;
    color: var(--text);
  }

  .back {
    font-weight: 600;
  }

  .pattern-name {
    color: var(--text);
    font-weight: 600;
  }

  .tab-empty {
    padding: 24px;
    font-size: 13px;
  }

  .left {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .note {
    font-size: 12px;
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  .editor-toolbar .primary {
    border-color: var(--accent);
    color: var(--accent);
  }

  .editor-toolbar .note {
    margin-left: auto;
  }

  .overflow {
    position: relative;
    display: inline-flex;
  }

  .more {
    padding: 2px 8px;
    line-height: 1;
  }

  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 140px;
    padding: 4px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-panel);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  }

  .menu button {
    background: transparent;
    border: none;
    border-radius: 4px;
    text-align: left;
    padding: 6px 8px;
    font-size: 12px;
    cursor: pointer;
    color: var(--text);
  }

  .menu button:hover {
    background: var(--bg-inset);
  }

  .subtabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  .subtab {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 5px 10px;
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
  }

  .subtab:hover {
    color: var(--text);
  }

  .subtab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .editor-host {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .editor-slot {
    height: 100%;
  }

  .editor-slot[hidden] {
    display: none;
  }

  .playback {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-top: 1px solid var(--border);
    background: var(--bg-panel);
    flex-wrap: wrap;
  }

  .playback .sep {
    width: 1px;
    align-self: stretch;
    margin: 2px 4px;
    background: var(--border);
  }

  /* ---- settings tab ---- */
  .settings {
    max-width: 620px;
    margin: 0 auto;
    padding: 20px 16px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .settings h1 {
    font-size: 18px;
    margin: 4px 0 6px;
    color: var(--text);
  }

  .card {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 14px;
    background: var(--bg-inset);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .flabel {
    width: 72px;
    color: var(--text-dim);
    font-size: 12px;
  }

  .grow {
    flex: 1;
    min-width: 160px;
  }

  .link {
    background: transparent;
    border: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
    font: inherit;
  }

  .right {
    padding: 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-panel);
  }

  h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-dim);
    margin: 10px 0 2px;
  }

  .banner {
    padding: 8px 10px;
    border-radius: 6px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
    text-align: left;
  }

  .banner.error {
    background: color-mix(in srgb, var(--error) 18%, transparent);
    border: 1px solid var(--error);
    color: #f2b8b8;
  }

  .banner.warn {
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    border: 1px solid var(--warn);
    color: #ecd9a8;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .as-button {
    cursor: pointer;
    width: 100%;
  }

  .dismiss {
    margin-left: auto;
    border: none;
    background: transparent;
    padding: 0 4px;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0;
  }

  .debug-toggle.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  .file-input {
    display: none;
  }

  .mapper-error {
    color: var(--error);
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .preview-wrap {
    position: relative;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: conn-spin 0.7s linear infinite;
  }

  @keyframes conn-spin {
    to {
      transform: rotate(1turn);
    }
  }
</style>
