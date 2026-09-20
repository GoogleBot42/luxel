<script lang="ts" context="module">
  import type { PatternDims } from "../stores/geometry";

  /** One browsable pattern. `source` is `undefined` while a device fetch for
   *  it is still in flight — the tile spins until it arrives. */
  export interface GalleryItem {
    /** Stable identity: a device pattern's id, else its name. */
    key: string;
    name: string;
    source?: string;
    /** gen-gallery's regex guess at the pattern's dims — ADVISORY only: it
     *  seeds the first compile's pixel count, and the engine's answer wins. */
    hint?: PatternDims;
  }
</script>

<script lang="ts">
  // The tile grid: every tile is the real pattern running live on the wasm
  // engine as a small looping thumbnail. One instance per SOURCE on the
  // Patterns page (on-device / library / corpus / this browser's saved), all
  // mounted at once so their compiled engines survive a source switch.
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
  // couple dozen small engines even on a 64×64 console. A grid the page has
  // hidden intersects nothing, so an inactive source costs nothing either.
  //
  // The tile's verbs are the PAGE's (pages/Patterns.svelte fills the `actions`
  // and `meta` slots) — this component owns geometry and scheduling only.
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
    type TileShape,
  } from "../stores/geometry";

  export let luxel: Luxel;
  /** The patterns to browse. `null` = fetch them from `src` instead (the
   *  generated library / corpus JSON). */
  export let items: GalleryItem[] | null = null;
  /** Which generated JSON to browse (relative to BASE_URL) when `items` is
   *  null. Defaults to the clean-room library; the corpus source passes its
   *  own. */
  export let src = "gallery.json";
  /** Shown by the page when the JSON is missing/empty. */
  export let emptyNote = "patterns unavailable (no gallery.json)";
  /** The page's one search box (filters by name). */
  export let search = "";
  /** `key` of the pattern running on the device — that tile gets the ring and
   *  the "playing" pill. Empty = nothing here is running. */
  export let playingKey = "";

  /** How many patterns this source holds (bound by the page for its segment
   *  chip), and whether it is still loading. */
  export let count = 0;
  export let loading = true;
  /** Set when the source loaded but holds nothing. */
  export let note = "";

  interface Tile extends GalleryItem {
    hint: PatternDims;
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

  const dispatch = createEventDispatcher<{ pick: GalleryItem }>();

  const STEP_BUDGET = 6; // engine frames per rAF tick
  const TILE_FPS_MS = 90; // ~11 fps per tile
  const ENGINE_CAP = 40;
  const CLOUD_PX = 96; // paintPoints draws at the canvas's intrinsic size

  let tiles: Tile[] = [];
  let raf = 0;
  let cursor = 0;

  const norm = (s: string): string => s.toLowerCase().trim();
  $: filter = norm(search);
  const hiddenBy = (t: Tile, f: string): boolean => f !== "" && !norm(t.name).includes(f);
  $: count = tiles.length;

  /** Adopt a new `items` list without throwing away engines that are still
   *  valid: a device source re-publishes its array every time one pattern's
   *  source streams in, and re-compiling 40 tiles for that would be absurd. */
  function syncItems(list: GalleryItem[]): void {
    const by = new Map(tiles.map((t) => [t.key, t]));
    const next: Tile[] = list.map((it) => {
      const prev = by.get(it.key);
      if (!prev) {
        return {
          key: it.key,
          name: it.name,
          source: it.source,
          hint: it.hint ?? 1,
          dims: 0,
          dead: false,
          ready: false,
          last: 0,
          seen: 0,
          visible: false,
        };
      }
      by.delete(it.key);
      prev.name = it.name;
      if (prev.source !== it.source) {
        // the source arrived (or changed) — the old engine is not this pattern
        prev.engine?.free();
        prev.engine = undefined;
        prev.rig = undefined;
        prev.ready = false;
        prev.dead = false;
        prev.source = it.source;
      }
      return prev;
    });
    for (const gone of by.values()) gone.engine?.free(); // deleted patterns
    tiles = next;
    loading = false;
    note = next.length === 0 ? emptyNote : "";
  }

  $: if (items !== null) syncItems(items);

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
      const key = (e.target as HTMLElement).dataset.tile;
      const t = tiles.find((x) => x.key === key);
      if (!t) continue;
      t.visible = e.isIntersecting;
      if (t.visible) t.seen = performance.now();
    }
    tiles = tiles; // reflect visibility so spinners only run for in-view tiles
  });

  function register(node: HTMLElement, key: string): { destroy: () => void } {
    node.dataset.tile = key;
    const t = tiles.find((x) => x.key === key);
    if (t) t.canvas = node.querySelector("canvas") ?? undefined;
    io.observe(node);
    return { destroy: () => io.unobserve(node) };
  }

  function ensureEngine(t: Tile): void {
    if (t.engine || t.dead || t.source === undefined) return;
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
    else {
      // paintBar/paintGrid resize the canvas to the pixel grid; a point cloud
      // has no grid, so restore the square the projection is drawn into.
      if (c.width !== CLOUD_PX || c.height !== CLOUD_PX) {
        c.width = CLOUD_PX;
        c.height = CLOUD_PX;
      }
      paintPoints(c, px, t.points ?? { pts: [], is3D: false }, performance.now() / 2500);
    }
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
    if (items !== null) return; // the page supplied the list
    // Patterns come from a generated JSON (`src`): gallery.json (the
    // clean-room library/, via tools/gen-gallery.mjs) by default, or the
    // corpus one for the PixelBlaze source. No inlined example set.
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
            key: p.name,
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
    if (tiles.length === 0) note = emptyNote;
    loading = false;
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    io.disconnect();
    for (const t of tiles) t.engine?.free();
  });
</script>

<div class="tiles">
  {#each tiles as t (t.key)}
    {@const shape = shapeOf(t)}
    {@const playing = playingKey !== "" && t.key === playingKey}
    <div
      class="tile"
      class:dead={t.dead}
      class:playing
      data-role="tile"
      data-kind={shape}
      data-dims={t.rig ? t.dims : ""}
      data-key={t.key}
      hidden={hiddenBy(t, filter)}
      use:register={t.key}
    >
      <div class="frame">
        <!-- §5.7: absent, never disabled. A pattern that does not compile is
             not something to pick, so the face is a plain element instead of
             a dimmed dead button, and the tile SAYS why (Gitea #529). -->
        <svelte:element
          this={t.dead ? "span" : "button"}
          class="face"
          data-role="tile-face"
          role={t.dead ? "presentation" : "button"}
          title={t.dead ? `${t.name} (does not compile)` : t.name}
          on:click={() => !t.dead && dispatch("pick", t)}
        >
          <span class="thumb" class:strip={shape === "bar"} data-shape={shape}>
            <canvas class:bar={shape === "bar"} class:sq={shape !== "bar"} width="96" height="96"
            ></canvas>
            {#if t.visible && !t.ready && !t.dead}
              <span
                class="spinner"
                data-role="tile-spinner"
                aria-label="loading"
                title="computing preview…"
              ></span>
            {/if}
          </span>
        </svelte:element>
        {#if playing}
          <span class="pill" data-role="tile-playing">▶ playing</span>
        {/if}
        {#if $$slots.actions}
          <div class="actions"><slot name="actions" item={t} dead={t.dead} /></div>
        {/if}
      </div>
      <span class="tname">{t.name}</span>
      {#if t.dead}<span class="tsub err" data-role="tile-dead">does not compile</span>{/if}
      {#if t.rig}
        {@const cap = captionFor(t.dims, t.rig)}
        {#if cap}<span class="tsub" data-role="tile-caption">{cap}</span>{/if}
      {/if}
      <slot name="meta" item={t} dead={t.dead} />
    </div>
  {/each}
</div>

<style>
  .tiles {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
    padding: 12px;
    align-content: start;
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
  }

  .tile:hover {
    border-color: var(--accent);
  }

  /* the pattern the device is running right now (proposal §5.1) */
  .tile.playing {
    border-color: var(--ok);
    box-shadow: 0 0 0 1px var(--ok) inset;
  }

  .tile.dead {
    opacity: 0.35;
  }

  .frame {
    position: relative;
    display: flex;
    max-width: 100%;
  }

  .face {
    display: block;
    padding: 0;
    border: none;
    background: transparent;
    max-width: 100%;
    cursor: pointer;
  }

  span.face {
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
    display: block;
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

  .pill {
    position: absolute;
    left: 50%;
    bottom: 4px;
    transform: translateX(-50%);
    padding: 1px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--ok) 82%, #000);
    color: #06210a;
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
    pointer-events: none;
  }

  /* per-tile verbs live on the tile they act on, and only on hover (§5.1) */
  /* Centred over the thumb, and never shorter than its own buttons — a bar
     tile is only 18 px tall, so the strip has to overhang it. */
  .actions {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
    min-height: 100%;
    padding: 3px 0;
    display: none;
    align-items: center;
    justify-content: center;
    gap: 4px;
    background: color-mix(in srgb, #000 62%, transparent);
    border-radius: 3px;
  }

  .tile:hover .actions,
  .actions:focus-within {
    display: flex;
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

  /* why a dead tile has no verbs (§5.7: absent, with the reason shown) */
  .tsub.err {
    color: var(--error);
    opacity: 1;
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

  /* Mobile (D9, S1c): two columns, and the hover strip is gone — a finger has
     no hover. Tapping the tile plays/opens it; `Edit` moves under the name
     (the page fills the `meta` slot with it). */
  @media (max-width: 600px) {
    /* `minmax(0, 1fr)`, not `1fr`: a column's implicit `auto` minimum is the
       item's min-content, and a long nowrap pattern name would widen it past
       half the screen (the desktop track's 150 px minimum hid this). */
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 8px;
      padding: 10px;
    }

    .actions {
      display: none !important;
    }
  }
</style>
