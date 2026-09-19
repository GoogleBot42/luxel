<script lang="ts">
  // Pattern browser: every tile is the real pattern running live on the wasm
  // engine as a small looping thumbnail.
  //
  // A tile's SHAPE is the Layout's (Gitea #463) — bars on a strip console,
  // squares on a matrix, a cloud on a 3D rig — and its dimensionality comes
  // from the COMPILED pattern (`Engine.preferredDims()`), never from
  // gen-gallery's regex, which is only an advisory hint that saves a second
  // compile in the common case. A pattern that is not native to the Layout
  // renders through the effective projection and gets a dim caption
  // (`1D · by index`), which is exactly what the device will show.
  //
  // Budgeting: only visible tiles animate (IntersectionObserver), a shared
  // rAF steps at most STEP_BUDGET engines per frame at ~11 fps each, engines
  // for tiles long out of view are freed beyond a cap, and every tile Layout
  // is capped at TILE_MAX_CELLS pixels — so 190 tiles cost no more than a
  // couple dozen small engines even on a 64×64 console.
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import { normalizePoints, paintBar, paintGrid, paintPoints, type PointRig } from "../lib/draw";
  import { Engine, Luxel } from "../lib/luxel";
  import { gatedFetch } from "../lib/fetchgate";
  import {
    captionFor,
    compileForLayout,
    layoutSignature,
    TILE_MAX_CELLS,
    tileShape,
    type Layout,
    type PatternDims,
    type TileShape,
  } from "../stores/geometry";

  export let luxel: Luxel;
  /** Which generated JSON to browse (relative to BASE_URL). Defaults to the
   *  clean-room library; the "PixelBlaze Library" tab passes the corpus one. */
  export let src = "gallery.json";
  /** Shown when the JSON is missing/empty. */
  export let emptyNote = "patterns unavailable (no gallery.json)";

  interface GalleryPick {
    name: string;
    source: string;
  }

  interface Tile {
    name: string;
    /** gen-gallery's regex guess at the pattern's dims — ADVISORY only: it
     *  seeds the first compile's pixel count, and the engine's answer wins. */
    hint: PatternDims;
    source: string;
    engine?: Engine;
    rig?: Layout;
    points?: PointRig;
    dims: PatternDims;
    canvas?: HTMLCanvasElement;
    dead: boolean;
    ready: boolean; // has drawn at least one frame (spinner off)
    last: number; // last stepped (ms)
    seen: number; // last visible (ms)
    visible: boolean;
  }

  const dispatch = createEventDispatcher<{ pick: GalleryPick; close: void }>();

  const STEP_BUDGET = 6; // engine frames per rAF tick
  const TILE_FPS_MS = 90; // ~11 fps per tile
  const ENGINE_CAP = 40;

  let tiles: Tile[] = [];
  let search = "";
  const matches = (t: Tile): boolean =>
    !search || t.name.toLowerCase().includes(search.toLowerCase().trim());
  $: shown = search ? tiles.filter(matches).length : tiles.length;
  let corpusNote = "";
  let loading = true; // gallery.json still streaming in
  let raf = 0;
  let cursor = 0;

  /** The Layout moved (the "Preview as" chip, or the device's own geometry
   *  changed): every tile engine was built for the old one, so drop them and
   *  let the scheduler rebuild the visible few. */
  let rigKey = "";
  $: {
    const next = $layoutSignature;
    if (next !== rigKey) {
      rigKey = next;
      resetEngines();
    }
  }

  function resetEngines(): void {
    for (const t of tiles) {
      t.engine?.free();
      t.engine = undefined;
      t.rig = undefined;
      t.ready = false;
      t.dead = false;
    }
    tiles = tiles;
  }

  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      const t = tiles[Number((e.target as HTMLElement).dataset.tile)];
      if (!t) continue;
      t.visible = e.isIntersecting;
      if (t.visible) t.seen = performance.now();
    }
    tiles = tiles; // reflect visibility so spinners only run for in-view tiles
  });

  function register(node: HTMLElement, i: number): { destroy: () => void } {
    node.dataset.tile = String(i);
    const t = tiles[i];
    if (t) t.canvas = node.querySelector("canvas") ?? undefined;
    io.observe(node);
    return { destroy: () => io.unobserve(node) };
  }

  function ensureEngine(t: Tile): void {
    if (t.engine || t.dead) return;
    const r = compileForLayout(luxel, t.source, TILE_MAX_CELLS);
    if ("engine" in r) {
      r.engine.setWallClock(Date.now() / 1000);
      t.engine = r.engine;
      t.rig = r.layout;
      t.dims = r.dims;
      t.points = r.layout.coords ? normalizePoints(r.layout.coords) : undefined;
      tiles = tiles; // the tile's shape/caption follow the compiled pattern
    } else {
      t.dead = true;
      tiles = tiles; // reflect the grayed-out state
    }
  }

  /** bar · grid · cloud · scatter — the Layout's shape, with the advisory
   *  hint standing in until the tile has compiled (so it does not flicker). */
  function shapeOf(t: Tile): TileShape {
    const l = t.rig;
    if (!l) return t.hint === 0 || t.hint === 1 ? "bar" : t.hint === 3 ? "cloud" : "grid";
    if (l.coords === undefined && !l.regular) return "bar";
    return tileShape(l);
  }

  function draw(t: Tile, px: Uint8Array): void {
    const c = t.canvas;
    if (!c || !t.rig) return;
    const shape = shapeOf(t);
    if (shape === "grid") paintGrid(c, px, t.rig.w, t.rig.h);
    else if (shape === "bar") paintBar(c, px);
    else paintPoints(c, px, t.points ?? { pts: [], is3D: false }, performance.now() / 2500);
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (tiles.length === 0) return;
    let steps = 0;
    let compiles = 0;
    for (let k = 0; k < tiles.length && steps < STEP_BUDGET; k++) {
      cursor = (cursor + 1) % tiles.length;
      const t = tiles[cursor];
      if (!t || !t.visible || t.dead) continue;
      if (!t.engine) {
        // compiling is the expensive part — spread it across frames
        if (compiles >= 2) continue;
        compiles++;
        ensureEngine(t);
        if (!t.engine) continue;
      }
      if (now - t.last < TILE_FPS_MS) continue;
      const dt = t.last === 0 ? 16 : Math.min(now - t.last, 100);
      t.last = now;
      draw(t, t.engine.frame(dt));
      t.engine.takeError(); // tolerate runtime errors; many patterns recover
      if (!t.ready) {
        t.ready = true;
        tiles = tiles; // first frame drawn → drop the spinner
      }
      steps++;
    }
    const live = tiles.filter((t) => t.engine);
    if (live.length > ENGINE_CAP) {
      live.sort((a, b) => a.seen - b.seen);
      for (const t of live.slice(0, live.length - ENGINE_CAP)) {
        if (!t.visible) {
          t.engine?.free();
          t.engine = undefined;
        }
      }
    }
  }

  onMount(async () => {
    raf = requestAnimationFrame(loop);
    // Patterns come from a generated JSON (`src`): gallery.json (the
    // clean-room library/, via tools/gen-gallery.mjs) by default, or the
    // corpus one for the PixelBlaze Library tab. No inlined example set.
    try {
      const r = await gatedFetch(`${import.meta.env.BASE_URL}${src}`);
      if (r.ok) {
        const list = (await r.json()) as { name: string; kind: string; source: string }[];
        const seen = new Set<string>();
        tiles = list
          .filter((p) => {
            const k = p.name.toLowerCase();
            if (seen.has(k)) return false;
            seen.add(k);
            return true;
          })
          .map((p) => ({
            name: p.name,
            hint: p.kind === "grid" ? 2 : p.kind === "cloud" ? 3 : 1,
            source: p.source,
            dims: 0,
            dead: false,
            ready: false,
            last: 0,
            seen: 0,
            visible: false,
          }));
      }
    } catch {
      /* gallery.json missing — the browser just shows empty */
    }
    if (tiles.length === 0) corpusNote = emptyNote;
    loading = false;
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    io.disconnect();
    for (const t of tiles) t.engine?.free();
  });
</script>

<div class="browser" role="region" aria-label="pattern browser">
  <header>
    <input
      class="search"
      data-role="gallery-search"
      type="search"
      placeholder="search patterns…"
      bind:value={search}
    />
    {#if loading}
      <span class="spinner header-spinner" aria-hidden="true"></span>
      <span class="dim" data-role="gallery-loading">loading patterns…</span>
    {:else}
      <span class="dim" data-role="gallery-count">
        {search ? `${shown} of ${tiles.length}` : `${tiles.length} patterns`} — click one to open it
      </span>
    {/if}
    {#if corpusNote}<span class="dim">· {corpusNote}</span>{/if}
  </header>
  <div class="tiles">
    {#each tiles as t, i (i)}
      <button
        class="tile"
        class:dead={t.dead}
        data-kind={shapeOf(t)}
        data-dims={t.rig ? t.dims : ""}
        title={t.dead ? `${t.name} (does not compile)` : t.name}
        hidden={search.trim() !== "" &&
          !t.name.toLowerCase().includes(search.trim().toLowerCase())}
        use:register={i}
        on:click={() => !t.dead && dispatch("pick", { name: t.name, source: t.source })}
      >
        <span class="thumb" class:strip={shapeOf(t) === "bar"}>
          {#if shapeOf(t) === "bar"}
            <canvas class="bar" width="64" height="1"></canvas>
          {:else}
            <canvas class="sq" width="96" height="96"></canvas>
          {/if}
          {#if t.visible && !t.ready && !t.dead}
            <span
              class="spinner"
              data-role="tile-spinner"
              aria-label="loading"
              title="computing preview…"
            ></span>
          {/if}
        </span>
        <span class="tname">{t.name}</span>
        {#if t.rig}
          {@const cap = captionFor(t.dims, t.rig)}
          {#if cap}<span class="tsub" data-role="tile-caption">{cap}</span>{/if}
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .browser {
    height: 100%;
    background: var(--bg, #14161a);
    display: flex;
    flex-direction: column;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
    font-size: 13px;
  }

  .dim {
    color: var(--text-dim);
  }

  .tiles {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
    padding: 12px;
    align-content: start;
  }

  .search {
    flex: none;
    width: 200px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-inset);
    color: var(--text);
    font-size: 13px;
  }

  .tile[hidden] {
    display: none;
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    cursor: pointer;
  }

  .tile:hover {
    border-color: var(--accent);
  }

  .tile.dead {
    opacity: 0.35;
    cursor: default;
  }

  .thumb {
    position: relative;
    display: inline-flex;
    max-width: 100%;
  }

  .thumb.strip {
    width: 100%;
    justify-content: center;
  }

  .tile canvas {
    image-rendering: pixelated;
    border-radius: 3px;
    background: #000;
    max-width: 100%;
  }

  /* the canvas's intrinsic size is the pixel grid; CSS fixes how big it looks */
  .tile canvas.bar {
    width: 128px;
    height: 18px;
  }

  .tile canvas.sq {
    width: 96px;
    height: 96px;
  }

  .spinner {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: tile-spin 0.7s linear infinite;
  }

  .header-spinner {
    position: static;
    display: inline-block;
    margin: 0;
    vertical-align: middle;
  }

  @keyframes tile-spin {
    to {
      transform: rotate(1turn);
    }
  }

  .tname {
    font-size: 11px;
    color: var(--text-dim);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tsub {
    font-size: 10px;
    color: var(--text-dim);
    opacity: 0.75;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
