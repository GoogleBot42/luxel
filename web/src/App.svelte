<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import Controls from "./components/Controls.svelte";
  import Debugger from "./components/Debugger.svelte";
  import Editor from "./components/Editor.svelte";
  import Gallery from "./components/Gallery.svelte";
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
  let debugMode = false;
  let breakpoints: number[] = [];
  let dbg: DebugSnapshot = { paused: false };

  let debounce: ReturnType<typeof setTimeout> | undefined;
  let raf = 0;
  let lastT = 0;
  let lastPoll = 0;

  // ---- device mode ----
  let device: DeviceSession | null = null;
  let deviceUrl = "";
  let deviceError = "";
  /** The device's stored pattern library (empty on firmware without CRUD). */
  let devicePatterns: { id: string; name: string }[] = [];
  /** Set while the editor holds a device-stored pattern. */
  let devicePatternId = "";

  async function refreshDevicePatterns(): Promise<void> {
    if (!device) return;
    try {
      devicePatterns = await device.patterns();
    } catch {
      devicePatterns = []; // older firmware — no /api/patterns yet
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

  const DEVICE_URL_KEY = "luxel:deviceUrl";
  /** Base of the last successful connection ("" = served-from-device), kept
   *  so reconnect works without re-typing the URL. */
  let lastBase: string | null = null;

  /** "device" once a session is up, "playground" otherwise — drives which
   *  affordances the header shows (share vs. device console). */
  $: mode = device ? "device" : "playground";

  // ---- top-level tabs ----
  // Both modes get the tab bar; device mode adds Settings. Panels stay
  // mounted and hide via CSS so the render loop, editor state, and gallery
  // engines survive a tab switch.
  type Tab = "editor" | "patterns" | "settings";
  let tab: Tab = "editor";
  /** File-actions overflow menu (import/export). */
  let menuOpen = false;
  /** Lazy-mount the gallery on first Patterns visit, then keep it alive so
   *  its compiled tile engines persist across tab switches. */
  let galleryMounted = false;
  $: if (tab === "patterns") galleryMounted = true;
  /** Total installed pixels — reactive so the Settings readout tracks the
   *  active layout (device connect sets it from the hardware). */
  $: pixelTotal =
    layout.kind === "strip"
      ? layout.pixels
      : layout.kind === "grid"
        ? layout.w * layout.h
        : layout.coords.length;
  // the map sub-tab is playground-only (device map upload comes later)
  $: if (device && subTab === "map") subTab = "pattern";

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
        if (running && px.length >= pixelCount() * 3) preview?.draw(px);
      },
      onStatus: (st) => {
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

  /** Connect from the UI: reuse the last known base when the field is empty
   *  (e.g. reconnecting after a served-from-device session), else use the
   *  typed URL. */
  function connectFromUi(): void {
    if (deviceUrl.trim() === "" && lastBase !== null) void connectDevice(lastBase);
    else void connectDevice();
  }

  async function connectDevice(baseOverride?: string): Promise<void> {
    deviceError = "";
    const base = (baseOverride ?? deviceUrl).trim().replace(/\/+$/, "");
    const session = new DeviceSession(base);
    try {
      const st = await session.status();
      device = session;
      lastBase = base; // remember it so reconnect needs no re-typing
      if (base !== "") {
        deviceUrl = base;
        try {
          localStorage.setItem(DEVICE_URL_KEY, base);
        } catch {
          /* private mode — non-fatal */
        }
      }
      if (debugMode) toggleDebug();
      layout = { kind: "strip", pixels: st.pixels };
      source = await session.pattern();
      hints = parseControlHints(source);
      controls = await session.controls();
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
    device = null;
    deviceError = "";
    devicePatterns = [];
    devicePatternId = "";
    if (tab === "settings") tab = "editor"; // Settings only exists in device mode
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
    clearTimeout(debounce);
    // device pushes go over WiFi — debounce longer than local recompiles
    debounce = setTimeout(recompile, device ? 500 : 150);
  }

  function loadExample(name: string): void {
    const ex = EXAMPLES.find((x) => x.name === name);
    if (!ex) return;
    exampleName = ex.name;
    patternName = "";
    importError = "";
    if (!device) layout = structuredClone(ex.layout); // device layout is fixed
    source = ex.source;
    controlValues = {};
    void tick().then(recompile);
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
      saveWorkingCopy({ source, layout, patternName, exampleName });
    }, 800);
  }
  $: if (source || layout) queueAutosave();

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
    const result = luxel.compileMap(mapSrc, pixelCount());
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
    recompile();
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

  function backToStrip(): void {
    layout = { kind: "strip", pixels: pixelCount() };
    recompile();
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
    tab = "editor"; // picking a pattern jumps back to the editor
    patternName = p.name;
    exampleName = "";
    importError = "";
    devicePatternId = "";
    if (!device) {
      layout = p.kind === "grid" ? { kind: "grid", w: 16, h: 16 } : { kind: "strip", pixels: 60 };
    }
    source = p.source;
    controlValues = {};
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

  function onExampleChange(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    devicePatternId = "";
    if (v.startsWith("saved:")) {
      loadSaved(v.slice(6));
    } else if (v.startsWith("device:")) {
      void loadDevicePattern(v.slice(7));
    } else {
      loadExample(v);
    }
  }

  /** What the picker shows: an example, a saved/device pattern, or the
   *  ad-hoc imported/shared entry. */
  $: selectValue =
    devicePatternId !== ""
      ? "device:" + devicePatternId
      : exampleName !== ""
        ? exampleName
        : saved.some((s) => s.name === patternName)
          ? "saved:" + patternName
          : "";

  // ---- layout editing ----

  function setLayoutKind(e: Event): void {
    const kind = (e.target as HTMLSelectElement).value;
    if (kind === "strip" && layout.kind !== "strip") {
      layout = { kind: "strip", pixels: pixelCount() };
    } else if (kind === "grid" && layout.kind !== "grid") {
      const side = Math.max(2, Math.round(Math.sqrt(pixelCount())));
      layout = { kind: "grid", w: side, h: side };
    }
    recompile();
  }

  function setLayoutNum(field: "pixels" | "w" | "h", e: Event): void {
    const v = Math.max(1, Math.min(4096, Number((e.target as HTMLInputElement).value) || 1));
    if (layout.kind === "strip" && field === "pixels") layout = { ...layout, pixels: v };
    if (layout.kind === "grid" && (field === "w" || field === "h")) {
      layout = { ...layout, [field]: v };
    }
    recompile();
  }

  function onFpsChange(e: Event): void {
    targetFps = Number((e.target as HTMLSelectElement).value);
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
        if (px.length >= pixelCount() * 3) preview?.draw(px);
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
    // pre-fill the device URL from the last successful connection so manual
    // reconnect needs no re-typing
    try {
      deviceUrl = localStorage.getItem(DEVICE_URL_KEY) ?? "";
    } catch {
      /* private mode — non-fatal */
    }

    // a share link's pattern beats the default example — and suppresses
    // device auto-connect (the link's intent is "look at this pattern").
    // Otherwise restore the autosaved working copy: never lose edits.
    const fromHash = await loadFromHash();
    if (!fromHash) {
      const wc = loadWorkingCopy();
      if (wc) {
        source = wc.source;
        layout = wc.layout;
        patternName = wc.patternName;
        exampleName = wc.exampleName;
      }
    }
    recompile();
    raf = requestAnimationFrame(loop);
    if (fromHash) return;

    // Served from a device (playground installed in its flash)? Auto-enter
    // device mode against the same origin. Must be a genuine device
    // response — a dev server's SPA fallback returns 200 HTML for
    // /api/status, so require JSON with the device's shape before
    // connecting (otherwise we'd strand a "device unreachable" banner on
    // the local playground).
    try {
      const ctl = new AbortController();
      const t = setTimeout(() => ctl.abort(), 1500);
      const r = await fetch("/api/status", { signal: ctl.signal });
      clearTimeout(t);
      const isJson = r.headers.get("content-type")?.includes("application/json");
      if (r.ok && isJson) {
        const st = (await r.json()) as { pixels?: unknown };
        if (typeof st.pixels === "number") {
          await connectDevice(""); // served-from-device → same origin
        }
      }
    } catch {
      /* not a device */
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
    <span class="wordmark">
      luxel <span class="dim">{device ? (device.base ? device.base : "device") : "playground"}</span>
    </span>

    <nav class="tabs" data-role="tabs">
      <button
        data-role="tab-editor"
        class="tab"
        class:active={tab === "editor"}
        on:click={() => (tab = "editor")}
      >
        Editor
      </button>
      <button
        data-role="tab-patterns"
        class="tab"
        class:active={tab === "patterns"}
        on:click={() => (tab = "patterns")}
      >
        Patterns
      </button>
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

    <span class="spacer"></span>

    {#if device}
      <span
        class="mono dim"
        title={wsLive ? "live pixel stream over websocket" : "polling over HTTP"}
      >
        {wsLive ? "streaming" : `polling · ${pollMs.toFixed(0)} ms`}
      </span>
    {/if}
    <span class="mono dim" data-role="fps">{fps.toFixed(0)} fps</span>

    <span class="conn">
      {#if device}
        <span class="device-badge mono">device{device.base ? ` ${device.base}` : ""}</span>
        <button on:click={disconnectDevice}>disconnect</button>
      {:else}
        <input
          class="device-url"
          type="text"
          placeholder={lastBase ? lastBase || "reconnect device" : "device url (http://…)"}
          bind:value={deviceUrl}
          on:keydown={(e) => e.key === "Enter" && connectFromUi()}
        />
        <button
          on:click={connectFromUi}
          disabled={deviceUrl.trim() === "" && lastBase === null}
          title={deviceUrl.trim() === "" && lastBase !== null
            ? `reconnect to ${lastBase || "the device"}`
            : "connect to a device"}
        >
          connect
        </button>
      {/if}
    </span>
  </header>

  <!-- ───────────── Editor tab ───────────── -->
  <main class="editor-tab" hidden={tab !== "editor"}>
    <section class="left">
      <div class="toolbar">
        <select data-role="pattern-picker" value={selectValue} on:change={onExampleChange}>
          {#if selectValue === ""}
            <option value="">{patternName || "imported"}</option>
          {/if}
          <optgroup label="examples">
            {#each EXAMPLES as ex (ex.name)}
              <option value={ex.name}>{ex.name}</option>
            {/each}
          </optgroup>
          {#if saved.length > 0}
            <optgroup label="saved">
              {#each saved as s (s.name)}
                <option value={"saved:" + s.name}>{s.name}</option>
              {/each}
            </optgroup>
          {/if}
          {#if device && devicePatterns.length > 0}
            <optgroup label="on device">
              {#each devicePatterns as p (p.id)}
                <option value={"device:" + p.id}>{p.name}</option>
              {/each}
            </optgroup>
          {/if}
        </select>

        <span class="spacer"></span>

        {#if saveNote}<span class="dim note" data-role="save-note">{saveNote}</span>{/if}
        {#if shareNote}<span class="dim note" data-role="share-note">{shareNote}</span>{/if}

        <span class="file-actions">
          <button
            data-role="save"
            title={device
              ? "save the current pattern on the device"
              : "save to this browser's library"}
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
          {#if !device}
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
          <input
            class="file-input"
            type="file"
            accept=".epe,.json,application/json"
            bind:this={fileInput}
            on:change={onImportPick}
          />
        </span>
      </div>

      {#if !device}
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
            map{#if layout.kind === "map"}<span class="dot" title="a map is installed">●</span>{/if}
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
        {#if !device && mapMounted}
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
        {#if !device && subTab === "map"}
          <button data-role="map-run" title="run the map program and install it" on:click={runMapNow}>
            run map
          </button>
          {#if layout.kind === "map"}
            <button data-role="map-back" title="drop the map, back to a strip" on:click={backToStrip}>
              back to strip
            </button>
            <span class="dim mono" data-role="map-badge">{layout.coords.length} px mapped</span>
          {/if}
          {#if mapError}<span class="mapper-error" data-role="map-error">{mapError}</span>{/if}
          <span class="sep"></span>
        {:else if !device}
          <select value={layout.kind} on:change={setLayoutKind}>
            <option value="strip">strip</option>
            <option value="grid">grid</option>
            {#if layout.kind === "map"}
              <option value="map">2D map</option>
            {/if}
          </select>
          {#if layout.kind === "map"}
            <span class="dim mono" data-role="map-badge">{layout.coords.length} px mapped</span>
          {:else if layout.kind === "strip"}
            <input
              class="num"
              data-role="layout-px"
              type="number"
              min="1"
              max="4096"
              value={layout.pixels}
              on:change={(e) => setLayoutNum("pixels", e)}
            />
            <span class="dim">px</span>
          {:else}
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
          {/if}
          <span class="sep"></span>
        {/if}
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
        {#if !device && subTab === "map"}
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

      {#if !device && subTab === "map" && mapDebugMode}
        <Debugger
          snapshot={mapDbg}
          runningHint="set a gutter breakpoint, then Run map to step through it"
          on:step={(e) => mapStep(e.detail)}
          on:break={mapRequestBreak}
        />
      {:else if debugMode && !device && subTab === "pattern"}
        <Debugger snapshot={dbg} on:step={(e) => step(e.detail)} on:break={requestBreak} />
      {/if}

      <Preview bind:this={preview} {layout} />

      <h2>Controls</h2>
      <Controls {controls} bind:values={controlValues} {readouts} {hints} on:set={onControlSet} />
      {#if controls.length === 0}
        <p class="dim hint">
          export <code>function sliderName(v)</code> to add controls — bound them with
          <code>//# min=0 max=5 step=0.5 default=2</code>
        </p>
      {/if}

      {#if !device}
        <h2>Map</h2>
        <p class="dim hint">
          {#if layout.kind === "map"}
            A {pixelTotal}-point map is installed. Edit it in the
            <button class="link" data-role="goto-map" on:click={() => (subTab = "map")}>map</button>
            tab — it's a debuggable Luxel program.
          {:else}
            Lay pixels out in 2D/3D from the
            <button class="link" data-role="goto-map" on:click={() => (subTab = "map")}>map</button>
            tab. The map is a Luxel program (<code>plot(x, y)</code> per pixel) you can step through.
          {/if}
        </p>
      {/if}

      <h2>Vars</h2>
      <VarWatcher {vars} />
      {#if Object.keys(vars).length === 0}
        <p class="dim hint">export <code>var name</code> to watch values here</p>
      {/if}
    </section>
  </main>

  <!-- ───────────── Patterns tab ───────────── -->
  <div class="patterns-tab" data-role="patterns-panel" hidden={tab !== "patterns"}>
    {#if galleryMounted && luxel}
      <Gallery {luxel} on:pick={onGalleryPick} on:close={() => (tab = "editor")} />
    {:else}
      <div class="tab-empty dim">loading patterns…</div>
    {/if}
  </div>

  <!-- ───────────── Settings tab (device mode only) ───────────── -->
  {#if device}
    <div class="settings-tab" data-role="settings-panel" hidden={tab !== "settings"}>
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
            <input class="num" data-role="cfg-pixels" type="number" value={pixelTotal} disabled />
            <span class="dim">strip — editable once firmware config lands (Phase 3)</span>
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
          <input type="range" min="0" max="31" value="31" disabled />
          <p class="dim hint">
            Runtime brightness needs firmware support (today it's a compile-time constant,
            <code>APA_BRIGHTNESS</code>). Wiring <code>GET/POST&nbsp;/api/brightness</code> is Phase&nbsp;3.
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
            device. Browse and load them from the
            <button class="link" on:click={() => (tab = "patterns")}>Patterns</button> tab; save the
            editor's current pattern with <em>save</em>.
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

  .editor-tab {
    display: grid;
    grid-template-columns: minmax(360px, 1fr) minmax(320px, 420px);
    flex: 1;
    min-height: 0;
  }

  /* one panel visible at a time; hidden ones stay mounted (state survives) */
  .editor-tab[hidden],
  .patterns-tab[hidden],
  .settings-tab[hidden] {
    display: none;
  }

  .patterns-tab,
  .settings-tab {
    flex: 1;
    min-height: 0;
  }

  .settings-tab {
    overflow-y: auto;
    background: var(--bg-panel);
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

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  .toolbar .note {
    font-size: 12px;
  }

  .file-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .file-actions .primary {
    border-color: var(--accent);
    color: var(--accent);
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
    right: 0;
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

  .subtab .dot {
    color: var(--accent);
    font-size: 8px;
    vertical-align: middle;
    margin-left: 4px;
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

  .device-url {
    width: 180px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .device-badge {
    color: var(--accent);
    font-size: 12px;
  }
</style>
