<script lang="ts">
  // The map program editor — a SCREEN of its own since A10 (Gitea #471),
  // reached from the Layout picker and never from inside a pattern
  // (proposal §4 "What the map program becomes", §5.7: the map program editor
  // is visible only for a custom Layout).
  //
  // The map is a real Luxel program: it plot()s one point per pixel on the VM,
  // so it is edited in the same CodeMirror as a pattern and debugged with the
  // same breakpoints/stepping (research/ui-audit.md §7.6 — the mapper and its
  // debugger are kept whole; only their placement was wrong).
  //
  // Same three owners as the pattern editor (§5.2), same chrome
  // (components/editor-frame.css):
  //   · the HEADER owns the document — back, the title, the installed/in-use
  //     state, the ONE primary action (Install on device on a console, Use in
  //     preview in the playground) and the ⋯ menu (export/import/clear).
  //   · the CODE PANE owns its errors — gutter dot, wavy underline, and one
  //     status strip pinned to its bottom edge.
  //   · the PREVIEW HEADER owns the transport — run and debug, next to the
  //     scatter they drive.
  //
  // Drag-editing the points and the Fill/Contain framing belong here too
  // (Gitea #355); the scatter below is where they will attach.
  import { createEventDispatcher, onDestroy } from "svelte";
  import AsyncButton from "../components/AsyncButton.svelte";
  import Debugger from "../components/Debugger.svelte";
  import DeviceChip from "../components/DeviceChip.svelte";
  import CodeEditor from "../components/Editor.svelte";
  import Popover from "../components/Popover.svelte";
  import Preview from "../components/Preview.svelte";
  import "../components/editor-frame.css";
  import {
    clearDeviceMap,
    device,
    deviceMap,
    devicePixels,
    installDeviceMapCoords,
    isPlayground,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import {
    cloudLayout,
    mapCoords,
    pixelCount,
    previewAs,
    setMapCoords,
    setPreviewAs,
  } from "../stores/geometry";
  import { banners, note, notes } from "../stores/notify";
  import { luxel, MAP_PROGRAM_TEMPLATE, mapSrc } from "../stores/pattern";
  import type { Layout } from "../lib/geometry";
  import { Engine, type DebugSnapshot, type Diagnostic, type StepKind } from "../lib/luxel";

  /** The screen is the visible surface (gates the render loop and the
   *  keyboard shortcuts). */
  export let active = false;
  /** What the back button returns to — the shell knows, the screen doesn't. */
  export let backLabel = "back";

  const dispatch = createEventDispatcher<{ back: void }>();

  let editor: CodeEditor;
  let preview: Preview;
  let fileInput: HTMLInputElement;

  let engine: Engine | undefined;
  let compileError: Diagnostic | null = null;
  let debugMode = false;
  let dbg: DebugSnapshot = { paused: false };
  let breakpoints: number[] = [];
  let debounce: ReturnType<typeof setTimeout> | undefined;
  let menuOpen = false;
  /** The ⋯ button the menu hangs off (components/Popover.svelte). */
  let moreBtn: HTMLElement;
  let importError = "";
  let raf = 0;

  /** The points the last run plotted, and their dimensionality (2 for
   *  `plot(x, y)`, 3 for `plot(x, y, z)` — the program decides, nothing else). */
  let coords: number[][] | null = null;
  let dims = 2;

  /** Lazy-mount the code pane on first visit (CodeMirror is not cheap) and
   *  keep it mounted afterwards, so the document and its breakpoints survive
   *  leaving the screen. */
  let mounted = false;
  $: if (active) mounted = true;

  /** These points ARE the app's Layout already — the playground committed
   *  them with "Use in preview", or the console's device is running them.
   *  Drives the header state and makes an edit re-publish live. */
  $: inUse = $previewAs.mode === "map" && $mapCoords !== null;

  /** The Layout the rail's scatter draws through: the plotted points
   *  themselves, at their own dimensionality — NOT the app's Layout. This
   *  screen shows what the program computed even before it is used anywhere.
   *
   *  Assigned by `adopt()` rather than derived with a `$:`. A run is kicked
   *  off from a reactive block (opening the screen compiles and runs), and a
   *  `$:` whose input is assigned inside a function another reactive block
   *  calls runs one cycle stale and NEVER catches up — Svelte folds the dirty
   *  bit into the fragment patch but does not re-run reactive statements that
   *  already ran (.claude/rules/web.md; it cost this ticket an afternoon when
   *  the scatter said "not run yet" for a map it was holding). */
  let plotted: Layout | null = null;
  /** Point colours: a hue ramp by index, so the scatter shows the WIRING
   *  ORDER as well as the shape (which end of the strip is which is the
   *  question a map answers). */
  let ramp: Uint8Array = new Uint8Array(0);

  /** Take the points a run produced: they are this screen's picture, and —
   *  when this map is already the app's Layout — the app's geometry too. */
  function adopt(pts: number[][], d: number): void {
    coords = pts;
    dims = d >= 3 ? 3 : 2;
    plotted = cloudLayout(pts);
    ramp = hueRamp(pts.length);
    if (inUse) publish(pts); // already the Layout: keep it live as you type
    if (active) startLoop();
  }

  function hueRamp(n: number): Uint8Array {
    const out = new Uint8Array(n * 3);
    for (let i = 0; i < n; i++) {
      const h = ((i / Math.max(1, n)) * 6) % 6;
      const x = 1 - Math.abs((h % 2) - 1);
      let r = 0;
      let g = 0;
      let b = 0;
      if (h < 1) {
        r = 1;
        g = x;
      } else if (h < 2) {
        r = x;
        g = 1;
      } else if (h < 3) {
        g = 1;
        b = x;
      } else if (h < 4) {
        g = x;
        b = 1;
      } else if (h < 5) {
        r = x;
        b = 1;
      } else {
        r = 1;
        b = x;
      }
      out[i * 3] = Math.round(r * 255);
      out[i * 3 + 1] = Math.round(g * 255);
      out[i * 3 + 2] = Math.round(b * 255);
    }
    return out;
  }

  // A 2D scatter is a still picture: it is painted ONCE per run. A 3D cloud
  // auto-rotates (the Preview component advances its own angle on every draw),
  // so that one keeps a frame loop — and only while the screen is visible.
  $: {
    active;
    plotted;
    ramp;
    if (active && plotted) startLoop();
    else stopLoop();
  }

  function startLoop(): void {
    if (raf === 0) raf = requestAnimationFrame(loop);
  }

  function stopLoop(): void {
    cancelAnimationFrame(raf);
    raf = 0;
  }

  function loop(): void {
    if (!plotted || ramp.length === 0) {
      raf = 0;
      return;
    }
    if (!preview) {
      raf = requestAnimationFrame(loop); // the canvas is one tick behind the points
      return;
    }
    preview.draw(ramp);
    raf = plotted.dims === 3 ? requestAnimationFrame(loop) : 0;
  }

  // ---- the program ----

  /** Compile the map program into its own engine, and run it unless we are
   *  debugging (then the user drives it with Run). */
  function recompile(autoRun = true): void {
    const lx = $luxel;
    if (!lx) return;
    // On a device the map lays out the fixed hardware pixel count; in the
    // playground it is whatever the current Layout declares.
    const result = lx.compileMap($mapSrc, $device ? $devicePixels : pixelCount());
    if (result instanceof Engine) {
      engine?.free();
      engine = result;
      compileError = null;
      if (debugMode) {
        engine.debugEnable(true);
        applyBreakpoints();
        dbg = { paused: false };
        editor?.setCurrentLine(null);
        if (autoRun) return; // don't auto-run under the debugger; user hits Run
      }
      if (autoRun) run();
    } else {
      compileError = result; // keep the last good points
    }
  }

  /** Run (or resume) the map program. Its points fill this screen's scatter;
   *  they reach the app's Layout only through the primary action — or
   *  straight away when this map is already in use, so an edit stays live. */
  function run(): void {
    if (!engine) {
      recompile(false);
      if (!engine) return;
    }
    note("map", "");
    const r = engine.runMap();
    if (r.paused) {
      dbg = engine.debugState();
      editor?.setCurrentLine(dbg.line ?? null);
      return;
    }
    finishRun(r.coords, r.dims);
  }

  function finishRun(pts: number[][], d: number): void {
    const err = engine?.takeError();
    if (err) {
      note("map", err.message);
      return;
    }
    if (pts.length === 0) {
      note("map", "the program plotted no points — call plot(x, y) per pixel");
      return;
    }
    adopt(pts, d);
  }

  /** These points become the app's Layout (every preview, tile and thumbnail
   *  renders through it — stores/geometry.ts). */
  function publish(pts: number[][]): void {
    setMapCoords(pts);
    setPreviewAs({ mode: "map", pixels: pts.length });
  }

  // ---- the primary action ----

  /** Playground: adopt this map as what the page previews on. */
  function useInPreview(): void {
    if (!coords) {
      run();
      if (!coords) return;
    }
    publish(coords);
    note("save", `previewing on ${coords.length} mapped points`, 2500);
  }

  /** Console: upload the points so the DEVICE renders 2D/3D through them.
   *  The console's Layout follows the device, so nothing else has to be set —
   *  `installDeviceMapCoords` updates the wire state the reconciler reads. */
  async function installOnDevice(): Promise<boolean> {
    if (!coords) {
      run();
      if (!coords) return false;
    }
    const pts = coords;
    // Awaited rather than fired and forgotten (#738): the button is the thing
    // that has to know whether this landed, and a big map is seconds of POST.
    if (!(await installDeviceMapCoords(dims, pts))) {
      note("map", "the device rejected the map");
      return false;
    }
    note("save", `installed ${pts.length} points on the device`, 3000);
    return true;
  }

  async function onClearDeviceMap(): Promise<void> {
    menuOpen = false;
    const ok = await confirm({
      title: "Clear the map from the device?",
      body: "The device goes back to its plain pixel order. The program below is not deleted.",
      confirmLabel: "Clear",
      danger: true,
    });
    if (!ok) return;
    await clearDeviceMap();
    note("save", "device map cleared", 2500);
  }

  // ---- the ⋯ menu: the program as a document ----

  function exportProgram(): void {
    menuOpen = false;
    const blob = new Blob([$mapSrc], { type: "text/plain" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "map-program.js";
    a.click();
    URL.revokeObjectURL(a.href);
  }

  function onImportPick(e: Event): void {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    input.value = ""; // allow re-importing the same file
    if (!f) return;
    void (async () => {
      try {
        const text = await f.text();
        if (text.trim() === "") throw new Error("the file is empty");
        importError = "";
        mapSrc.set(text); // the CodeMirror wrapper swaps its document on the prop
        recompile();
      } catch (err) {
        importError = `could not read ${f.name}: ${String(err)}`;
      }
    })();
  }

  async function resetProgram(): Promise<void> {
    menuOpen = false;
    const ok = await confirm({
      title: "Reset the map program?",
      body: "The program below goes back to the built-in ring example. Nothing installed on a device changes.",
      confirmLabel: "Reset",
      danger: true,
    });
    if (!ok) return;
    mapSrc.set(MAP_PROGRAM_TEMPLATE);
    recompile();
  }

  // ---- the debugger (the same one the pattern editor uses) ----

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
      toggleDebug(); // placing a breakpoint arms the map debugger
    } else if (debugMode) {
      applyBreakpoints();
    }
  }

  function toggleDebug(): void {
    debugMode = !debugMode;
    if (!engine) recompile(false);
    if (!engine) return;
    engine.debugEnable(debugMode);
    if (debugMode) {
      applyBreakpoints();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      run(); // resume a whole run once debugging is off
    }
  }

  function step(kind: StepKind): void {
    if (!engine || !dbg.paused) return;
    const still = engine.debugStep(kind);
    if (still) {
      dbg = engine.debugState();
      editor?.setCurrentLine(dbg.line ?? null);
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      const r = engine.mapResult(); // the run finished
      finishRun(r.coords, r.dims);
    }
  }

  function requestBreak(): void {
    engine?.debugPause();
  }

  function jumpToError(): void {
    if (compileError) editor?.jumpTo(compileError.line, compileError.col);
  }

  // ---- hover inspection (this engine's own scope) ----

  function hoverValue(name: string): string | null {
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

  function fmtRaw(raw: number): string {
    return (raw / 65536).toFixed(4).replace(/\.?0+$/, "") || "0";
  }

  function fmtLocal(l: { raw?: number; array?: number; fn?: number }): string {
    if (l.raw !== undefined) return fmtRaw(l.raw);
    if (l.array !== undefined) return `array[${l.array}]`;
    return `fn#${l.fn}`;
  }

  // ---- editing ----

  function onSourceChange(e: CustomEvent<string>): void {
    mapSrc.set(e.detail);
    clearTimeout(debounce);
    debounce = setTimeout(() => recompile(!debugMode), 200);
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
        from: byteToChar($mapSrc, compileError.start),
        to: byteToChar($mapSrc, compileError.end),
        message: compileError.message,
      });
    } else {
      editor.setErrorRange(null);
    }
  }

  /** Compile and run once the screen is first opened, so the scatter is
   *  already showing what the program does. Named dependencies, not a bare
   *  call: `$:` only tracks what appears in its own syntax
   *  (.claude/rules/web.md). */
  let armed = false;
  $: {
    $luxel;
    mounted;
    if (mounted && $luxel && !armed) {
      armed = true;
      recompile(true);
    }
  }

  /** ⌘/Ctrl+Enter runs the program — the pattern editor's "apply" shortcut,
   *  meaning the same thing here. */
  function onKeydown(e: KeyboardEvent): void {
    if (!active) return;
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      run();
    }
  }

  onDestroy(() => {
    stopLoop();
    clearTimeout(debounce);
    engine?.free();
    engine = undefined;
  });
</script>

<svelte:window on:keydown={onKeydown} />

<main class="editor-view editor-frame" data-role="map-editor-view" hidden={!active}>
  <!-- ── the header owns the DOCUMENT ──
       back · the title · the installed / in-use state · ONE primary action ·
       the ⋯ menu. No geometry fields, no transport. -->
  <header class="editor-header" data-role="map-editor-header">
    <button
      data-role="map-editor-back"
      class="btn quiet back"
      title={`back to ${backLabel}`}
      on:click={() => dispatch("back")}
    >
      <span class="backglyph" aria-hidden="true">←</span>
      <span class="backlabel">{backLabel}</span>
    </button>

    <!-- the map program has one name, so this is a label rather than the
         pattern editor's editable `.nameedit` field -->
    <span class="title">Map program</span>

    <span class="savestate" data-role="map-state">
      {#if $device}
        {#if $deviceMap.installed}
          <span data-role="map-installed">{$deviceMap.count}px {$deviceMap.dims}D on device</span>
        {:else}
          no map on the device
        {/if}
      {:else if inUse}
        previewing this map
      {:else}
        not used in the preview yet
      {/if}
    </span>

    <span class="spacer"></span>

    {#if $notes.save}<span class="dim note" data-role="map-note">{$notes.save}</span>{/if}

    <!-- The screen's single primary action (proposal §4): the console
         installs the points on the hardware, the playground adopts them as
         what the page previews on. -->
    {#if $device}
      <!-- A big map is a big POST, and the old button gave nothing back until
           the note appeared (#738). -->
      <AsyncButton
        cls="btn primary"
        dataRole="map-install"
        label="Install on device"
        doneLabel="Installed"
        title="upload these points so the device renders 2D/3D through them"
        action={installOnDevice}
      />
    {:else}
      <button
        class="btn primary"
        data-role="map-use"
        title="preview every pattern on this page through these points"
        on:click={useInPreview}
      >
        Use in preview
      </button>
    {/if}

    <span class="overflow">
      <button
        class="btn icon"
        bind:this={moreBtn}
        data-role="map-overflow"
        title="more actions"
        aria-label="more actions"
        on:click={() => (menuOpen = !menuOpen)}
      >
        ⋯
      </button>
      <Popover
        open={menuOpen}
        anchor={moreBtn}
        dataRole="map-menu"
        on:close={() => (menuOpen = false)}
      >
        <button class="mi" data-role="map-export" role="menuitem" on:click={exportProgram}>
          Export map program
        </button>
        <button class="mi" data-role="map-import" role="menuitem" on:click={() => fileInput.click()}>
          Import map program…
        </button>
        <div class="sepr"></div>
        <button class="mi del" data-role="map-reset" role="menuitem" on:click={() => void resetProgram()}>
          Reset program
        </button>
        <!-- Clearing the DEVICE's map exists only when there is one to clear
             (§5.7: absent, never disabled). -->
        {#if $device && $deviceMap.installed}
          <button
            class="mi del"
            data-role="map-clear"
            role="menuitem"
            on:click={() => void onClearDeviceMap()}
          >
            Clear map from device
          </button>
        {/if}
      </Popover>
    </span>

    <input
      class="file-input"
      type="file"
      accept=".js,.txt,text/plain"
      bind:this={fileInput}
      on:change={onImportPick}
    />

    <!-- WHICH device this screen is bound to, over the rail it describes. The
         shell header is not rendered over an editor screen (#538), so the chip
         travels with it — mockup S2 puts it in the header's rail column. -->
    {#if !$isPlayground}
      <span class="edhdr-rail statusonly"><DeviceChip /></span>
    {/if}
  </header>

  <!-- The body is the mock's `.edbody`: code left, rail right, and the two
       stacked with the rail first on a phone (S2b). -->
  <div class="edbody" data-role="map-editor-body">
    <!-- ── the code column holds only code, and owns its own errors ── -->
    <section class="left">
      <!-- S2b: on a phone the code column wears the same section header the
           rail sections do, and its `.rdim` says why it is read-only. -->
      <div class="code-head">
        <span class="slabel">Code</span>
        <span class="rdim code-hint">edit on a larger screen to change code</span>
      </div>
      <div class="editor-host">
        {#if mounted}
          <div class="editor-slot" data-role="map-editor">
            <CodeEditor
              bind:this={editor}
              value={$mapSrc}
              {hoverValue}
              on:change={onSourceChange}
              on:breakpoints={onBreakpoints}
            />
          </div>
        {/if}
      </div>

      {#if compileError}
        <button class="codestatus err" data-role="map-compile-error" on:click={jumpToError}>
          ✗ line {compileError.line} · {compileError.message}
          <span class="jump">jump to line</span>
        </button>
      {/if}
    </section>

    <section class="right">
      <div class="railscroll">
        {#each $banners as b (b.id)}
          <div class="banner" class:error={b.level === "error"} class:warn={b.level === "warn"} data-role={b.role}>
            {b.text}
          </div>
        {/each}
        {#if importError}
          <div class="banner error" data-role="map-import-error">
            {importError}
            <button class="dismiss" on:click={() => (importError = "")}>×</button>
          </div>
        {/if}

        {#if debugMode}
          <div class="rsec">
            <Debugger
              snapshot={dbg}
              runningHint="set a gutter breakpoint, then Run to step through the map"
              on:step={(e) => step(e.detail)}
              on:break={requestBreak}
            />
          </div>
        {/if}

        <!-- ── the plotted points: the scatter (2D) or cloud (3D) this program
             computes, with its count and the dimensionality it chose ── -->
        <div class="rsec">
          <div class="rhead">
            <span class="slabel">Map</span>
            <span class="rdim" data-role="map-badge">
              {#if plotted}{plotted.pixels} points · {plotted.dims}D{:else}not run yet{/if}
            </span>
            <span class="grp">
              <!-- Gitea #739 on the third editor screen: this was a bare `▶`
                   text glyph, which a headless chromium with no symbol font
                   draws as tofu — and an unlabelled mark in a rail of labelled
                   buttons reads as an afterthought. Same inline SVG and same
                   mark-then-word shape as the pattern editor's transport;
                   `.editor-frame .grp .btn svg` already sizes it to the 13px
                   its neighbour's bug icon uses. -->
              <button
                class="btn sm"
                data-role="map-run"
                title="run the map program (⌘/Ctrl+Enter)"
                on:click={run}
              >
                <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M8 5.2 19.2 12 8 18.8Z" />
                </svg>
                Run
              </button>
              <!-- labelled like the pattern editor's, and for the same reason
                   (audit E8): the bug glyph alone does not say "debugger" -->
              <button
                class="btn sm"
                class:active={debugMode}
                data-role="map-debug"
                title="toggle the map debugger"
                on:click={toggleDebug}
              >
                Debug
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                  <rect x="8" y="7" width="8" height="12" rx="4" />
                  <path d="M4 10h4M16 10h4M4 16h4M16 16h4M9.5 5l1 2M14.5 5l-1 2" />
                </svg>
              </button>
            </span>
          </div>
          {#if plotted}
            <!-- Points coloured by index, so the picture shows the WIRING ORDER as
                 well as the shape. Drag-editing them and the Fill/Contain framing
                 attach here (Gitea #355). -->
            <Preview bind:this={preview} layout={plotted} />
          {:else}
            <p class="dim hint">
              Run the program to see its points. Each <code>plot(x, y)</code> is one pixel, in strip
              order; <code>plot(x, y, z)</code> makes it a 3D cloud.
            </p>
          {/if}
          {#if $notes.map}<p class="map-error" data-role="map-error">{$notes.map}</p>{/if}
        </div>

        <div class="rsec">
          <div class="rhead"><span class="slabel">About</span></div>
          <p class="dim hint">
            {#if $device}
              The map is the DEVICE's geometry, not a pattern's: install it and every pattern on the
              device renders <code>render2D</code>/<code>render3D</code> through these points.
            {:else}
              The map is the Layout, not a pattern's: use it and every preview, tile and thumbnail on
              this page renders through these points.
            {/if}
            It is a Luxel program on the VM, so it is debuggable — set a gutter breakpoint and step.
          </p>
        </div>
      </div>
    </section>
  </div>
</main>

<style>
  /* The header split, the transport boxes and the phone stacking are
     components/editor-frame.css — the pattern editor wears the same chrome. */

  .title {
    font-size: 14px;
    font-weight: 600;
  }

  .dim {
    color: var(--text-dim);
  }

  .hint {
    font-size: 12px;
    margin: 2px 0;
  }

  .map-error {
    margin: 0;
    color: var(--error);
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  /* ---- phone (D9) ---- S2b's back is icon-only: a 390 px header has no room
     for a destination name it is about to show you anyway. */
  @media (max-width: 600px) {
    .back .backlabel {
      display: none;
    }

    .back {
      width: 32px;
      padding: 0;
    }
  }
</style>
