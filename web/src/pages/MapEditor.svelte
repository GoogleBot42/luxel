<script lang="ts">
  // The map program: a real Luxel program that plot()s one point per pixel,
  // so it is edited in the same CodeMirror as patterns and debuggable the
  // same way (breakpoints / stepping). It runs on its own engine; the
  // collected coordinates are handed up as an `install` event.
  //
  // Everything map-shaped lives here — source, engine, compile state,
  // debugger, breakpoints, hover scope. The Editor page renders this
  // component's code pane inline and drives the rest through the exported
  // methods, because the map's buttons sit in the shared playback bar and its
  // debugger in the shared right rail. A10 (#471) promotes this component to
  // a screen of its own; at that point nothing else has to move.
  import { createEventDispatcher } from "svelte";
  import CodeEditor from "../components/Editor.svelte";
  import { device, devicePixels } from "../stores/device";
  import { pixelCount } from "../stores/geometry";
  import { note } from "../stores/notify";
  import { luxel, mapSrc } from "../stores/pattern";
  import { Engine, type DebugSnapshot, type Diagnostic, type StepKind } from "../lib/luxel";

  /** Render the code pane (the map rig is the active layout). */
  export let showPane = false;
  /** The code pane is the visible document. */
  export let visible = false;
  /** True once a map is the active rig: edits then re-install live. Before
   *  that (or while debugging) an edit only recompiles, so errors and
   *  breakpoints track without hijacking the layout — the user applies the
   *  first time with "run map". */
  export let liveApply = false;
  /** Bound out: the Editor renders the banner and the debugger. */
  export let compileError: Diagnostic | null = null;
  export let debugMode = false;
  export let dbg: DebugSnapshot = { paused: false };

  const dispatch = createEventDispatcher<{ install: { coords: number[][]; dims: number } }>();

  let editor: CodeEditor;
  let engine: Engine | undefined;
  let breakpoints: number[] = [];
  let debounce: ReturnType<typeof setTimeout> | undefined;
  /** Lazy-mount the code pane on first visit. */
  let mounted = false;
  $: if (visible) mounted = true;

  export function markMounted(): void {
    mounted = true;
  }

  export function hasEngine(): boolean {
    return engine !== undefined;
  }

  /** Compile the map program into its own engine. On success runs it (unless
   *  we're debugging — then the user drives it with Run). */
  export function recompile(autoRun = true): void {
    const lx = $luxel;
    if (!lx) return;
    // on a device the map lays out the fixed hardware pixel count (it's a local
    // preview aid); in the playground it's whatever the current layout declares
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
      compileError = result; // keep the last good map installed
    }
  }

  /** Run (or resume) the map program; install coords when it finishes, or
   *  surface the debugger when it pauses at a breakpoint. */
  export function run(): void {
    if (!engine) return;
    note("map", "");
    const { paused, coords, dims } = engine.runMap();
    if (paused) {
      dbg = engine.debugState();
      editor?.setCurrentLine(dbg.line ?? null);
      return;
    }
    const err = engine.takeError();
    if (err) {
      note("map", err.message);
      return;
    }
    if (coords.length > 0) dispatch("install", { coords, dims });
  }

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

  export function toggleDebug(): void {
    debugMode = !debugMode;
    if (!engine) recompile(false);
    if (!engine) return;
    engine.debugEnable(debugMode);
    if (debugMode) {
      applyBreakpoints();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      run(); // resume live install once debugging is off
    }
  }

  export function step(kind: StepKind): void {
    if (!engine || !dbg.paused) return;
    const still = engine.debugStep(kind);
    if (still) {
      dbg = engine.debugState();
      editor?.setCurrentLine(dbg.line ?? null);
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      const err = engine.takeError();
      if (err) {
        note("map", err.message);
        return;
      }
      const r = engine.mapResult(); // run finished
      if (r.coords.length > 0) dispatch("install", { coords: r.coords, dims: r.dims });
    }
  }

  export function requestBreak(): void {
    engine?.debugPause();
  }

  export function jumpToError(): void {
    if (compileError) editor?.jumpTo(compileError.line, compileError.col);
  }

  export function free(): void {
    clearTimeout(debounce);
    engine?.free();
    engine = undefined;
  }

  /** Hover inspection for the map editor (its own engine's scope). */
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

  function onSourceChange(e: CustomEvent<string>): void {
    mapSrc.set(e.detail);
    clearTimeout(debounce);
    const autoRun = liveApply && !debugMode;
    debounce = setTimeout(() => recompile(autoRun), 200);
  }

  /** Diagnostic spans are UTF-8 byte offsets; CodeMirror wants char offsets. */
  function byteToChar(text: string, byte: number): number {
    const bytes = new TextEncoder().encode(text);
    return new TextDecoder().decode(bytes.subarray(0, Math.min(byte, bytes.length))).length;
  }

  // keep the map editor's squiggle in sync with its compile status
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
</script>

{#if mounted && showPane}
  <div class="editor-slot" data-role="map-editor" hidden={!visible}>
    <CodeEditor
      bind:this={editor}
      value={$mapSrc}
      {hoverValue}
      on:change={onSourceChange}
      on:breakpoints={onBreakpoints}
    />
  </div>
{/if}

<style>
  .editor-slot {
    height: 100%;
  }

  .editor-slot[hidden] {
    display: none;
  }
</style>
