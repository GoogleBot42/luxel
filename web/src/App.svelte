<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import Controls from "./components/Controls.svelte";
  import Editor from "./components/Editor.svelte";
  import Preview from "./components/Preview.svelte";
  import VarWatcher from "./components/VarWatcher.svelte";
  import { EXAMPLES, type Example } from "./lib/examples";
  import { parseControlHints, type ControlHint } from "./lib/hints";
  import { Engine, Luxel, type Control, type Diagnostic, type RuntimeError } from "./lib/luxel";

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

  let debounce: ReturnType<typeof setTimeout> | undefined;
  let raf = 0;
  let lastT = 0;
  let lastPoll = 0;

  const pixelCount = () => (layout.kind === "strip" ? layout.pixels : layout.w * layout.h);

  function recompile(): void {
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
    debounce = setTimeout(recompile, 150);
  }

  function loadExample(name: string): void {
    const ex = EXAMPLES.find((x) => x.name === name);
    if (!ex) return;
    exampleName = ex.name;
    layout = structuredClone(ex.layout);
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
    engine?.setControl(e.detail.name, e.detail.values);
  }

  function jumpToError(): void {
    if (compileError) editor.jumpTo(compileError.line, compileError.col);
  }

  // ---- render loop ----

  function loop(t: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || !running) return;
    const minInterval = targetFps > 0 ? 1000 / targetFps - 1 : 0;
    if (lastT !== 0 && t - lastT < minInterval) return;
    const dt = lastT === 0 ? 1000 / (targetFps || 60) : Math.min(t - lastT, 200);
    lastT = t;
    const px = engine.frame(dt);
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
      <select value={layout.kind} on:change={setLayoutKind}>
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
    </span>

    <span class="spacer"></span>
    <span class="mono dim">{fps.toFixed(0)} fps</span>
  </header>

  <main>
    <section class="left">
      <Editor bind:this={editor} value={source} on:change={onSourceChange} />
    </section>
    <section class="right">
      {#if loadFailure}
        <div class="banner error">{loadFailure}</div>
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
</style>
