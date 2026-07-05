<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import Controls from "./components/Controls.svelte";
  import Editor from "./components/Editor.svelte";
  import Preview from "./components/Preview.svelte";
  import VarWatcher from "./components/VarWatcher.svelte";
  import { EXAMPLES, type Example } from "./lib/examples";
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
  const controlValues = new Map<string, number[]>();
  let readouts = new Map<string, number>();
  let vars: Record<string, number | number[]> = {};
  let fps = 0;
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
      controls = engine.controls();
      // reapply saved control values (PB persists control state per pattern)
      for (const c of controls) {
        const saved = controlValues.get(c.name);
        if (saved && c.kind !== "showNumber" && c.kind !== "gauge" && c.kind !== "trigger") {
          engine.setControl(c.name, saved);
        }
      }
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
    layout = ex.layout;
    source = ex.source;
    controlValues.clear();
    void tick().then(recompile);
  }

  function onControlSet(e: CustomEvent<{ name: string; values: number[] }>): void {
    engine?.setControl(e.detail.name, e.detail.values);
  }

  function onExampleChange(e: Event): void {
    loadExample((e.target as HTMLSelectElement).value);
  }

  function jumpToError(): void {
    if (compileError) editor.jumpTo(compileError.line, compileError.col);
  }

  function loop(t: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine) return;
    const dt = lastT === 0 ? 16.7 : Math.min(t - lastT, 100);
    lastT = t;
    const px = engine.frame(dt);
    preview?.draw(px);
    fps = fps * 0.95 + (1000 / Math.max(dt, 1)) * 0.05;
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
    <span class="dim layout-label">
      {layout.kind === "strip" ? `${layout.pixels} px strip` : `${layout.w}×${layout.h} grid`}
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
      <Controls {controls} values={controlValues} {readouts} on:set={onControlSet} />
      {#if controls.length === 0}
        <p class="dim hint">
          export <code>function sliderName(v)</code> to add controls
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
  }

  .wordmark {
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--accent);
  }

  .dim {
    color: var(--text-dim);
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

  .layout-label {
    font-size: 12px;
  }
</style>
