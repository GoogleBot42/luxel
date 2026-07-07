<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import Controls from "./components/Controls.svelte";
  import Debugger from "./components/Debugger.svelte";
  import Editor from "./components/Editor.svelte";
  import Gallery from "./components/Gallery.svelte";
  import PatternThumb from "./components/PatternThumb.svelte";
  import Preview from "./components/Preview.svelte";
  import VarWatcher from "./components/VarWatcher.svelte";
  import { DeviceSession } from "./lib/device";
  import { EXAMPLES, type Layout } from "./lib/examples";
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

  let luxel: Luxel | undefined;
  let engine: Engine | undefined;
  let editor: Editor;
  let preview: Preview;

  let source = EXAMPLES[0]?.source ?? "";
  let layout: Layout = EXAMPLES[0]?.layout ?? { kind: "strip", pixels: 60 };
  let exampleName = EXAMPLES[0]?.name ?? "";

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

  async function loadDevicePattern(id: string): Promise<void> {
    if (!device) return;
    try {
      const p = await device.patternSource(id);
      const r = await device.activatePattern(id);
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
      setTimeout(async () => {
        if (device) controls = await device.controls();
      }, 300);
    } catch (e) {
      deviceError = `cannot load pattern: ${String(e)}`;
    }
  }
  let devicePreviewBusy = false;
  let lastDevicePreview = 0;
  let lastDeviceStatus = 0;
  let pollMs = 0; // measured preview request latency (HTTP fallback only)
  let deviceWs: WebSocket | null = null;
  let wsLive = false; // push socket delivering — HTTP polling stands down
  let statusMisses = 0;
  let wsGraceUntil = 0; // suppress HTTP polls while the handshake runs

  /** Connection phase. `connecting` holds the preview blank so the async
   *  status→source→controls→stream handshake never leaks pre-stabilization
   *  frames (old playground content, resize artifacts, HTTP/WS jitter) into
   *  the waterfall; `live` starts the moment the real stream delivers. */
  let deviceConn: "idle" | "connecting" | "live" = "idle";

  /** First real datum from the device → go live, wiping anything drawn before
   *  the stream stabilized. Idempotent. */
  function markLive(): void {
    if (deviceConn === "connecting") {
      deviceConn = "live";
      preview?.clear();
    }
  }

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
  type Tab = "library" | "device" | "settings";
  let tab: Tab = "library";
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
    void tick().then(recompile);
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
      await loadDevicePattern(id);
    } finally {
      patternLoading = false;
    }
  }

  /** The label/target the editor's back button returns to. */
  $: backLabel = tab === "device" ? "Device Patterns" : "Patterns Library";
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

  function openDeviceSocket(): void {
    if (!device) return;
    // The device serves 2 connections and the server reaps idle keep-alive
    // sockets after ~1 s. Quiet the HTTP polls first, wait out the reaper,
    // THEN dial — otherwise the handshake races our own idle connections.
    wsGraceUntil = performance.now() + 6000;
    const session = device;
    setTimeout(() => {
      if (!device || deviceWs) return;
      dialDeviceSocket(session);
    }, 1600);
  }

  function dialDeviceSocket(session: DeviceSession): void {
    deviceWs = session.openSocket({
      onPixels: (px) => {
        wsLive = true;
        if (px.length >= pixelCount() * 3) {
          markLive(); // first good frame → clear the connecting hold
          if (running) preview?.draw(px);
        }
      },
      onStatus: (st) => {
        markLive(); // a status frame confirms the stream even while paused
        fps = st.fps;
        runtimeError = st.vmerr ? { message: st.vmerr, fn: 0, pc: 0 } : null;
      },
      onVars: (v) => (vars = v),
      onReadouts: (r) => (readouts = r),
      onControls: (c) => (controls = c),
      onClose: () => {
        wsLive = false;
        deviceWs = null; // deviceTick's HTTP polling takes back over
        // retry: the socket can lose the connection race against asset
        // loading on the 2-connection device
        setTimeout(() => {
          if (device && !deviceWs) openDeviceSocket();
        }, 3000);
      },
    });
  }

  const pixelCount = () =>
    layout.kind === "strip"
      ? layout.pixels
      : layout.kind === "grid"
        ? layout.w * layout.h
        : layout.coords.length;

  /** Connect (or reconnect) to the bound device. The base is always known —
   *  from served-from-device detection or a `?device=` override — so there's
   *  never a URL to type. `pullPattern` loads the device's running pattern into
   *  the editor; pass false to keep an in-progress edit (working copy). */
  async function connectDevice(base: string, pullPattern = true): Promise<void> {
    deviceError = "";
    base = base.trim().replace(/\/+$/, "");
    const session = new DeviceSession(base);
    try {
      const st = await session.status();
      device = session;
      deviceBase = base; // bind (survives disconnect → reconnect needs no URL)
      deviceConn = "connecting"; // hold the preview blank until the stream is live
      // suppress HTTP preview polling from the very start of the handshake, so
      // the ws wins the race and no HTTP frame flips us live before the stream
      // is really up (openDeviceSocket refreshes this + schedules the dial)
      wsGraceUntil = performance.now() + 6000;
      if (debugMode) toggleDebug();
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
      controls = await session.controls();
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
      compileError = null;
      runtimeError = st.vmerr ? { message: st.vmerr, fn: 0, pc: 0 } : null;
      fps = st.fps;
      preview?.clear();
      await refreshDevicePatterns();
      openDeviceSocket();
    } catch (e) {
      device = null;
      deviceError = `cannot reach device: ${String(e)}`;
    }
  }

  function disconnectDevice(): void {
    deviceWs?.close();
    deviceWs = null;
    wsLive = false;
    deviceConn = "idle";
    device = null;
    deviceError = "";
    devicePatterns = [];
    devicePatternId = "";
    if (tab === "settings") tab = "library"; // Settings needs a live connection
    recompile();
  }

  /** byte offset of (1-based line, col) for squiggles on device errors */
  function lineColToByte(text: string, line: number, col: number): number {
    const lines = text.split("\n");
    let off = 0;
    for (let i = 0; i < Math.min(line - 1, lines.length); i++) {
      off += new TextEncoder().encode(lines[i]).length + 1;
    }
    return off + Math.max(0, col - 1);
  }

  async function devicePush(): Promise<void> {
    if (!device) return;
    const r = await device.run(source);
    if (r.ok) {
      compileError = null;
      runtimeError = null;
      hints = parseControlHints(source);
      // the swap happens on the device's next render tick; give the
      // controls snapshot a moment (the status tick also refreshes them)
      setTimeout(async () => {
        if (device) controls = await device.controls();
      }, 300);
    } else {
      const start = lineColToByte(source, r.line, r.col);
      compileError = { line: r.line, col: r.col, message: r.error, start, end: start + 1 };
    }
  }

  function recompile(): void {
    if (device) {
      void devicePush();
      return;
    }
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
    clearTimeout(debounce);
    // device pushes go over WiFi — debounce longer than local recompiles
    debounce = setTimeout(recompile, device ? 500 : 150);
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
      recompile();
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
        const r = await device?.savePattern(name, source);
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
    void tick().then(recompile);
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
    // on a device the map only rearranges the preview — the device keeps
    // running its own pattern, so don't re-push anything
    if (!device) recompile();
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

  function onGalleryPick(e: CustomEvent<{ name: string; kind: "strip" | "grid"; source: string }>): void {
    const p = e.detail;
    openEditor("library"); // picking a pattern opens it in the editor
    patternName = p.name;
    exampleName = "";
    importError = "";
    devicePatternId = "";
    if (!device) {
      layout = p.kind === "grid" ? { kind: "grid", w: 16, h: 16 } : { kind: "strip", pixels: 60 };
    }
    source = p.source;
    controlValues = {};
    dirty = false; // freshly picked from the gallery
    void tick().then(recompile);
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
    const bytes = new TextEncoder().encode(source);
    let frag: string;
    try {
      frag = `p=${b64url(await pipe(bytes, new CompressionStream("deflate-raw")))}`;
    } catch {
      frag = `ps=${b64url(bytes)}`;
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

  /** Restore a `#p=`/`#ps=` fragment; returns whether one was loaded. */
  async function loadFromHash(): Promise<boolean> {
    const m = /^(p|ps)=([A-Za-z0-9_-]+)$/.exec(location.hash.slice(1));
    const kind = m?.[1];
    const payload = m?.[2];
    if (!kind || !payload) return false;
    try {
      const data = b64urlDecode(payload);
      const bytes =
        kind === "p" ? await pipe(data, new DecompressionStream("deflate-raw")) : data;
      source = new TextDecoder().decode(bytes);
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
      if (layout.kind !== "map" && !device) recompile();
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
    if (!device) recompile(); // preview-only on a device (don't re-push)
  }

  function setLayoutNum(field: "pixels" | "w" | "h", e: Event): void {
    const v = Math.max(1, Math.min(4096, Number((e.target as HTMLInputElement).value) || 1));
    if (layout.kind === "strip" && field === "pixels") layout = { ...layout, pixels: v };
    if (layout.kind === "grid" && (field === "w" || field === "h")) {
      layout = { ...layout, [field]: v };
    }
    if (!device) recompile(); // preview-only on a device (don't re-push)
  }

  function onFpsChange(e: Event): void {
    targetFps = Number((e.target as HTMLSelectElement).value);
  }

  /** Live brightness: the device applies it immediately and persists it. */
  function onBrightnessChange(e: Event): void {
    brightness = Number((e.target as HTMLInputElement).value);
    void device?.setBrightness(brightness);
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
    if (device) {
      void device.setControl(e.detail.name, e.detail.values);
    } else {
      engine?.setControl(e.detail.name, e.detail.values);
    }
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

  async function deviceTick(t: number): Promise<void> {
    if (!device) return;
    if (wsLive) return; // push socket is delivering everything
    if (performance.now() < wsGraceUntil) return; // ws handshake in flight
    if (t - lastDeviceStatus > 1000) {
      lastDeviceStatus = t;
      try {
        const st = await device.status();
        statusMisses = 0;
        markLive(); // HTTP fallback is up → leave the connecting hold
        fps = st.fps;
        runtimeError = st.vmerr ? { message: st.vmerr, fn: 0, pc: 0 } : null;
        deviceError = "";
        controls = await device.controls(); // tracks pattern swaps
      } catch {
        // tolerate transient misses — the device serves only 2 connections
        if (++statusMisses >= 3) deviceError = "device unreachable";
      }
    }
    if (t - lastPoll > 300) {
      lastPoll = t;
      try {
        vars = await device.vars();
        readouts = await device.readouts();
      } catch {
        /* transient; status poll reports connectivity */
      }
    }
    const interval = 1000 / Math.min(targetFps || 30, 30);
    if (!devicePreviewBusy && running && t - lastDevicePreview >= interval) {
      devicePreviewBusy = true;
      lastDevicePreview = t;
      const t0 = performance.now();
      try {
        const px = await device.pixels();
        pollMs = pollMs * 0.8 + (performance.now() - t0) * 0.2;
        if (px.length >= pixelCount() * 3) {
          markLive();
          preview?.draw(px);
        }
      } catch {
        /* ditto */
      } finally {
        devicePreviewBusy = false;
      }
    }
  }

  function loop(t: number): void {
    raf = requestAnimationFrame(loop);
    if (device) {
      void deviceTick(t);
      return;
    }
    if (!engine || !running) return;
    if (dbg.paused) return; // suspended at a debug stop — step buttons drive
    const minInterval = targetFps > 0 ? 1000 / targetFps - 1 : 0;
    if (lastT !== 0 && t - lastT < minInterval) return;
    const dt = lastT === 0 ? 1000 / (targetFps || 60) : Math.min(t - lastT, 200);
    lastT = t;
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

  onMount(async () => {
    try {
      luxel = await Luxel.load(`${import.meta.env.BASE_URL}luxel.wasm`);
    } catch (e) {
      loadFailure = `failed to load luxel.wasm: ${String(e)}`;
      return;
    }
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
    recompile();
    raf = requestAnimationFrame(loop);
    editing = hadWip; // resume in the editor if there was work in progress
    if (fromHash) return; // a share link never auto-connects to a device

    // How we bind to a device (a plain playground binds to none):
    //  1. `?device=<base>` — a dev/e2e override pointing the built UI at a
    //     device or the native mirror.
    //  2. served-from-device — the UI loaded from the device's own flash, so
    //     the device is this same origin. Auto-connect.
    // Either way the base is then *known*, so reconnect never needs a URL.
    // On a device the editor opens over the Device Patterns tab, showing the
    // running pattern (unless the user had work in progress — then that wins).
    const override = new URLSearchParams(location.search).get("device");
    let base: string | null = null;
    if (override !== null) {
      base = override.trim().replace(/\/+$/, "");
    } else {
      // Served from a device? Require a genuine device response — a dev
      // server's SPA fallback returns 200 HTML for /api/status, so require
      // JSON with the device's shape before binding.
      try {
        const ctl = new AbortController();
        const t = setTimeout(() => ctl.abort(), 1500);
        const r = await fetch("/api/status", { signal: ctl.signal });
        clearTimeout(t);
        const isJson = r.headers.get("content-type")?.includes("application/json");
        if (r.ok && isJson) {
          const st = (await r.json()) as { pixels?: unknown };
          if (typeof st.pixels === "number") base = "";
        }
      } catch {
        /* not a device — stays a playground */
      }
    }
    if (base !== null) {
      deviceBase = base; // bind (device UI shows even if the connect races)
      tab = "device"; // the editor's back button lands on Device Patterns
      editing = true;
      patternLoading = true; // cover the editor through the async handshake
      try {
        // A genuinely-unsaved edit is resumed AND pushed so the device runs it
        // too (editor, preview and device all agree). A clean copy instead
        // opens whatever pattern is currently active on the device.
        await connectDevice(base, /* pullPattern */ !wipDirty);
        if (wipDirty && device) await devicePush();
      } finally {
        patternLoading = false;
      }
    }
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    clearTimeout(debounce);
    clearTimeout(mapDebounce);
    engine?.free();
    mapEngine?.free();
  });
</script>

<svelte:window on:click={() => (menuOpen = false)} />

<div
  class="shell"
  data-mode={mode}
  data-tab={tab}
  role="application"
  on:drop={onDrop}
  on:dragover|preventDefault
>
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

    {#if device}
      {#if deviceConn === "connecting"}
        <span class="mono connecting" data-role="conn-state" title="handshaking with the device">
          <span class="spinner" aria-hidden="true"></span> connecting…
        </span>
      {:else}
        <span
          class="mono dim"
          data-role="conn-state"
          title={wsLive ? "live pixel stream over websocket" : "polling over HTTP"}
        >
          {wsLive ? "streaming" : `polling · ${pollMs.toFixed(0)} ms`}
        </span>
      {/if}
    {/if}
    <span class="mono dim" data-role="fps">{fps.toFixed(0)} fps</span>

    {#if !isPlayground}
      <span class="conn">
        {#if device}
          <span class="device-badge mono">device{device.base ? ` ${device.base}` : ""}</span>
          <button on:click={disconnectDevice}>disconnect</button>
        {:else}
          <!-- the device address is already known — reconnect, no URL to type -->
          <button
            data-role="reconnect"
            on:click={() => void connectDevice(deviceBase ?? "")}
            title={`reconnect to ${deviceBase || "the device"}`}
          >
            {deviceError ? "retry" : "reconnect"}
          </button>
        {/if}
      </span>
    {/if}
  </header>

  <!-- ───────────── Editor tab ───────────── -->
  <main class="editor-view" hidden={!editing}>
    {#if patternLoading}
      <!-- cover the editor while a pattern is being fetched/activated so the
           previously-open script never flashes before the real one loads -->
      <div class="pattern-loading" data-role="pattern-loading">
        <span class="spinner"></span> loading pattern…
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
          <!-- the map is a local WASM program, so it's debuggable even on a
               device (where the pattern debugger itself is unavailable) -->
          <button
            class="debug-toggle"
            class:active={mapDebugMode}
            data-role="map-debug"
            title="toggle the map debugger"
            on:click={toggleMapDebug}
          >
            debug
          </button>
        {:else if !device}
          <button
            class="debug-toggle"
            class:active={debugMode}
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
      {:else if debugMode && !device && subTab === "pattern"}
        <Debugger snapshot={dbg} on:step={(e) => step(e.detail)} on:break={requestBreak} />
      {/if}

      <div class="preview-wrap">
        <Preview bind:this={preview} {layout} />
        {#if device && deviceConn === "connecting"}
          <div class="preview-connecting" data-role="preview-connecting">
            <span class="spinner"></span> connecting to device…
          </div>
        {/if}
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
        <p class="dim hint">device offline — reconnect to manage its patterns.</p>
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
              {fps.toFixed(0)} fps · {wsLive ? "streaming" : "polling"}{runtimeError
                ? ` · vmerr: ${runtimeError.message}`
                : ""}
            </span>
          </div>
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
          <p class="dim hint">
            The device stores WiFi credentials (<code>/api/wifi</code>) and reboots to apply them. A
            form to change them from here — plus AP-mode provisioning — arrives with the settings
            firmware work.
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

  .conn {
    display: flex;
    align-items: center;
    gap: 6px;
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

  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .editor-view[hidden],
  .library-tab[hidden],
  .device-tab[hidden],
  .settings-tab[hidden] {
    display: none;
  }

  .library-tab,
  .device-tab,
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
  .settings-tab {
    overflow-y: auto;
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

  .device-badge {
    color: var(--accent);
    font-size: 12px;
  }

  .connecting {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--warn);
    font-size: 12px;
  }

  .preview-wrap {
    position: relative;
  }

  .preview-connecting {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: var(--text-dim);
    font-size: 13px;
    pointer-events: none;
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
