<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import Controls from "./components/Controls.svelte";
  import Editor from "./components/Editor.svelte";
  import Preview from "./components/Preview.svelte";
  import VarWatcher from "./components/VarWatcher.svelte";
  import { DeviceSession } from "./lib/device";
  import { EXAMPLES, type Example } from "./lib/examples";
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
  let layout: Example["layout"] = EXAMPLES[0]?.layout ?? { kind: "strip", pixels: 60 };
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
  let devicePreviewBusy = false;
  let lastDevicePreview = 0;
  let lastDeviceStatus = 0;
  let pollMs = 0; // measured preview request latency (HTTP fallback only)
  let deviceWs: WebSocket | null = null;
  let wsLive = false; // push socket delivering — HTTP polling stands down
  let statusMisses = 0;
  let wsGraceUntil = 0; // suppress HTTP polls while the handshake runs

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

  const pixelCount = () => (layout.kind === "strip" ? layout.pixels : layout.w * layout.h);

  async function connectDevice(): Promise<void> {
    deviceError = "";
    const base = deviceUrl.trim().replace(/\/+$/, "");
    const session = new DeviceSession(base);
    try {
      const st = await session.status();
      device = session;
      if (debugMode) toggleDebug();
      layout = { kind: "strip", pixels: st.pixels };
      source = await session.pattern();
      hints = parseControlHints(source);
      controls = await session.controls();
      compileError = null;
      runtimeError = st.vmerr ? { message: st.vmerr, fn: 0, pc: 0 } : null;
      fps = st.fps;
      preview?.clear();
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
    if (!device) layout = structuredClone(ex.layout); // device layout is fixed
    source = ex.source;
    controlValues = {};
    void tick().then(recompile);
  }

  function onExampleChange(e: Event): void {
    loadExample((e.target as HTMLSelectElement).value);
  }

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
    recompile();
    raf = requestAnimationFrame(loop);

    // Served from a device (playground installed in its flash)? Auto-enter
    // device mode against the same origin. On dev servers /api/status 404s
    // and we stay local.
    try {
      const ctl = new AbortController();
      const t = setTimeout(() => ctl.abort(), 1500);
      const r = await fetch("/api/status", { signal: ctl.signal });
      clearTimeout(t);
      if (r.ok) {
        deviceUrl = "";
        await connectDevice();
      }
    } catch {
      /* not a device */
    }
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    clearTimeout(debounce);
    engine?.free();
  });
</script>

<div class="shell">
  <header>
    <span class="wordmark">luxel <span class="dim">playground</span></span>
    <select value={exampleName} on:change={onExampleChange}>
      {#each EXAMPLES as ex (ex.name)}
        <option value={ex.name}>{ex.name}</option>
      {/each}
    </select>

    <span class="group">
      <select value={layout.kind} on:change={setLayoutKind} disabled={device !== null}>
        <option value="strip">strip</option>
        <option value="grid">grid</option>
      </select>
      {#if layout.kind === "strip"}
        <input
          class="num"
          type="number"
          min="1"
          max="4096"
          value={layout.pixels}
          disabled={device !== null}
          on:change={(e) => setLayoutNum("pixels", e)}
        />
        <span class="dim">px</span>
      {:else}
        <input
          class="num"
          type="number"
          min="1"
          max="256"
          value={layout.w}
          on:change={(e) => setLayoutNum("w", e)}
        />
        <span class="dim">×</span>
        <input
          class="num"
          type="number"
          min="1"
          max="256"
          value={layout.h}
          on:change={(e) => setLayoutNum("h", e)}
        />
      {/if}
    </span>

    <span class="group">
      <select value={targetFps} on:change={onFpsChange}>
        <option value={0}>max fps</option>
        <option value={60}>60 fps</option>
        <option value={30}>30 fps</option>
        <option value={15}>15 fps</option>
        <option value={5}>5 fps</option>
      </select>
      <button on:click={togglePause} title={running ? "pause" : "resume"}>
        {running ? "pause" : "play"}
      </button>
      <button
        class="debug-toggle"
        class:active={debugMode}
        disabled={device !== null}
        title={device ? "debugging runs on the local engine only (for now)" : "toggle debugger"}
        on:click={toggleDebug}
      >
        debug
      </button>
    </span>

    <span class="group">
      {#if device}
        <span class="device-badge mono">device{device.base ? ` ${device.base}` : ""}</span>
        <button on:click={disconnectDevice}>disconnect</button>
      {:else}
        <input
          class="device-url"
          type="text"
          placeholder="device url (http://…)"
          bind:value={deviceUrl}
          on:keydown={(e) => e.key === "Enter" && connectDevice()}
        />
        <button on:click={connectDevice} disabled={deviceUrl.trim() === ""}>connect</button>
      {/if}
    </span>

    <span class="spacer"></span>
    {#if device}
      <span class="mono dim">{wsLive ? "ws push" : `${pollMs.toFixed(0)}ms poll`}</span>
    {/if}
    <span class="mono dim">{fps.toFixed(0)} fps</span>
  </header>

  <main>
    <section class="left">
      <Editor
        bind:this={editor}
        value={source}
        {hoverValue}
        on:change={onSourceChange}
        on:breakpoints={onBreakpoints}
      />
    </section>
    <section class="right">
      {#if loadFailure}
        <div class="banner error">{loadFailure}</div>
      {/if}
      {#if deviceError}
        <div class="banner error">{deviceError}</div>
      {/if}
      {#if compileError}
        <button class="banner error as-button" on:click={jumpToError}>
          line {compileError.line}:{compileError.col} — {compileError.message}
        </button>
      {/if}
      {#if runtimeError && !compileError}
        <div class="banner warn">
          runtime: {runtimeError.message}
          <button class="dismiss" on:click={() => (runtimeError = null)}>×</button>
        </div>
      {/if}

      {#if debugMode && !device}
        <div class="debugger" data-paused={dbg.paused}>
          <div class="debug-bar">
            {#if dbg.paused}
              <button class="db-continue" on:click={() => step("continue")}>▶ continue</button>
              <button class="db-over" on:click={() => step("over")}>step</button>
              <button class="db-into" on:click={() => step("into")}>into</button>
              <button class="db-out" on:click={() => step("out")}>out</button>
            {:else}
              <button class="db-break" on:click={requestBreak}>break</button>
              <span class="dim">running — click the gutter to set breakpoints</span>
            {/if}
          </div>
          {#if dbg.paused}
            <div class="debug-status mono">
              paused at line {dbg.line}{dbg.pixel !== null && dbg.pixel !== undefined
                ? ` · pixel ${dbg.pixel}`
                : ""}
            </div>
            {#if dbg.stack && dbg.stack.length > 0}
              <div class="stack" data-role="stack">
                {#each dbg.stack as f, i (i)}
                  <div class="stack-frame mono">
                    <span class="fn-name">{f.name}</span>
                    <span class="dim">line {f.line}</span>
                  </div>
                  {#if i === 0}
                    <table class="locals">
                      <tbody>
                        {#each f.locals as l (l.name)}
                          <tr>
                            <td class="name mono">{l.name}</td>
                            <td class="value mono">
                              {#if l.raw !== undefined}{fmtRaw(l.raw)}
                              {:else if l.array !== undefined}array[{l.array}]
                              {:else}fn#{l.fn}{/if}
                            </td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  {/if}
                {/each}
              </div>
            {/if}
            {#if dbg.globals && dbg.globals.length > 0}
              <div class="scope-title dim">globals</div>
              <table class="locals" data-role="globals">
                <tbody>
                  {#each dbg.globals as g (g.name)}
                    <tr>
                      <td class="name mono">{g.name}</td>
                      <td class="value mono">{fmtLocal(g)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          {/if}
        </div>
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

      <h2>Vars</h2>
      <VarWatcher {vars} />
      {#if Object.keys(vars).length === 0}
        <p class="dim hint">export <code>var name</code> to watch values here</p>
      {/if}
    </section>
  </main>
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

  .group {
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

  main {
    display: grid;
    grid-template-columns: minmax(360px, 1fr) minmax(320px, 420px);
    flex: 1;
    min-height: 0;
  }

  .left {
    min-width: 0;
    min-height: 0;
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

  .device-url {
    width: 180px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .device-badge {
    color: var(--accent);
    font-size: 12px;
  }

  .debugger {
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-inset);
  }

  .debug-bar {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 12px;
  }

  .debug-status {
    color: var(--accent);
    font-size: 12px;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stack-frame {
    display: flex;
    gap: 8px;
    font-size: 12px;
  }

  .fn-name {
    color: var(--text);
  }

  .locals {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    margin: 2px 0 6px 12px;
  }

  .locals td {
    padding: 1px 6px;
  }

  .locals .name {
    color: var(--text-dim);
    width: 40%;
  }

  .locals .value {
    color: var(--accent);
  }

  .scope-title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-top: 4px;
  }
</style>
