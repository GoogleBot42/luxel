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
  // The tile itself is the mock's card (mockups.html `.tile`, frames S1/S1b/
  // S1c): a panel-coloured box with a FULL-BLEED canvas whose aspect is the
  // fixture's (a 6:1 bar on a strip, the lattice's own w:h on a matrix), a
  // left-aligned meta block under it, and the verbs as a gradient strip that
  // fades in on hover. The playing tile wears the ▶ pill AND its strip — the
  // page drops only the Play verb from it (#555).
  //
  // `only` splits the grid by whether the Layout can show the pattern at all
  // (#538) — see the prop.
  //
  // The tile's verbs are the PAGE's (pages/Patterns.svelte fills the `actions`
  // and `meta` slots) — this component owns geometry and scheduling only.
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import { normalizePoints, paintBar, paintGrid, paintPoints, type PointRig } from "../lib/draw";
  import { Engine, Luxel } from "../lib/luxel";
  import { gatedFetch } from "../lib/fetchgate";
  import {
    autoLayoutFor,
    captionFor,
    compileForLayout,
    guessPatternDims,
    layout,
    layoutSignature,
    projectionCompatible,
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

  /**
   * Which half of the layout-compatibility split this grid shows (#538,
   * Jeremy's rule: a strip never shows a 2D/3D pattern, a plane never a 3D
   * one). `null` = no split, every pattern.
   *
   * The rule only bites when the Layout is a real FIXTURE — a device, or an
   * explicit "Preview as" choice. Under playground Auto the Layout follows
   * whichever pattern the editor happens to hold, so filtering by it would
   * empty the library depending on what you last opened.
   */
  export let only: "compatible" | "incompatible" | null = null;
  /** Render each tile on the Layout its own pattern asks for (the playground's
   *  "Auto") instead of the device's — what the "Not for this layout" group
   *  needs, since the device's Layout is precisely the one that cannot show
   *  these patterns. */
  export let autoStyle = false;

  /** How many patterns this source SHOWS (bound by the page for its segment
   *  chip — the split above is part of the count), and whether it is still
   *  loading. */
  export let count = 0;
  export let loading = true;
  /** Set when the source loaded but holds nothing. */
  export let note = "";

  interface Tile extends GalleryItem {
    hint: PatternDims;
    /** The best dimensionality known for this pattern: `hint` (or a regex
     *  guess at its source) until it compiles, `preferredDims()` after. It is
     *  what the compatibility split reads, so the split is complete from the
     *  first render instead of resolving tile by tile as they scroll in. */
    guess: PatternDims;
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
  /** paintPoints draws at the canvas's INTRINSIC size; the tile then scales
   *  it, so a cloud is drawn big enough to survive a full-bleed card. */
  const CLOUD_PX = 192;

  let tiles: Tile[] = [];
  let raf = 0;
  let cursor = 0;

  const norm = (s: string): string => s.toLowerCase().trim();
  $: filter = norm(search);
  const hiddenBy = (t: Tile, f: string): boolean => f !== "" && !norm(t.name).includes(f);

  /** Adopt a new `items` list without throwing away engines that are still
   *  valid: a device source re-publishes its array every time one pattern's
   *  source streams in, and re-compiling 40 tiles for that would be absurd. */
  function syncItems(list: GalleryItem[]): void {
    const by = new Map(tiles.map((t) => [t.key, t]));
    const next: Tile[] = list.map((it) => {
      const prev = by.get(it.key);
      if (!prev) {
        const hint = it.hint ?? (it.source === undefined ? 1 : guessPatternDims(it.source));
        return {
          key: it.key,
          name: it.name,
          source: it.source,
          hint,
          guess: hint,
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
        // …and so is the dimensionality that placed it in this grid
        if (it.hint === undefined && it.source !== undefined) {
          prev.hint = guessPatternDims(it.source);
          prev.guess = prev.hint;
        }
      }
      return prev;
    });
    for (const gone of by.values()) gone.engine?.free(); // deleted patterns
    tiles = next;
    loading = false;
    note = next.length === 0 ? emptyNote : "";
  }

  $: if (items !== null) syncItems(items);

  // NB: everything below reads `tiles`, which `syncItems` fills — and Svelte
  // orders reactive statements by the assignments it can SEE, which does not
  // include one made inside a called function. Source order is the ordering
  // here, so these must stay after the line above or a grid whose items are
  // supplied (not fetched) renders empty until something else invalidates.

  /** The Layout is a real FIXTURE — a device, or an explicit "Preview as"
   *  choice — rather than playground Auto, which just follows whatever the
   *  editor holds. Only a fixture filters the grid, and only a fixture's
   *  shape decides the column count (under Auto the tiles are a mix). */
  $: fixture = $layout.source === "device" || $layout.source === "user";
  $: splitting = only !== null && fixture;
  /** `preferredDims()` once the tile has compiled, the advisory guess before. */
  const dimsOf = (t: Tile): PatternDims => (t.rig ? t.dims : t.guess);
  const inThisGrid = (t: Tile, ld: number, split: boolean): boolean =>
    !split || projectionCompatible(dimsOf(t), ld) === (only === "compatible");

  /** The tiles this grid owns. A split grid RENDERS only its half rather
   *  than hiding the other one: both halves are mounted over the same item
   *  list, so a merely-hidden tile would still answer `.tile` queries in the
   *  other grid — and the page has two of them stacked.
   *
   *  The pattern that is RUNNING sorts to the front (mockups S1/S1b/S1c all
   *  draw the playing tile first): the console opens on this page to see what
   *  the LEDs are doing, and hunting for the green ring in row four is not
   *  that. Everything else keeps the source's own order. */
  $: shown = tiles
    .filter((t) => inThisGrid(t, $layout.dims, splitting))
    .sort((a, b) => Number(b.key === playingKey) - Number(a.key === playingKey));
  $: count = shown.length;

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
    return {
      destroy: () => {
        io.unobserve(node);
        // unobserving fires no final callback, so a tile the split just moved
        // to the other grid would stay `visible` and keep stepping its engine
        // into a detached canvas
        if (t) t.visible = false;
      },
    };
  }

  function ensureEngine(t: Tile): void {
    if (t.engine || t.dead || t.source === undefined) return;
    const r = compileForLayout(
      luxel,
      t.source,
      TILE_MAX_CELLS,
      null,
      autoStyle ? autoLayoutFor : undefined,
    );
    if ("engine" in r) {
      r.engine.setWallClock(Date.now() / 1000);
      t.engine = r.engine;
      t.rig = r.layout;
      t.dims = r.dims;
      t.guess = r.dims; // the compiler's answer replaces the regex guess
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

  /** The card's canvas is full-bleed (mockups.html `.tile canvas`), so its
   *  ASPECT carries the fixture's shape: the mock's 6:1 bar on a strip, the
   *  lattice's own w:h on a matrix, a square for a point cloud. */
  function aspectOf(t: Tile, shape: TileShape): string {
    if (shape === "bar") return "6 / 1";
    const l = t.rig;
    if (shape === "grid" && l && l.w > 0 && l.h > 0) return `${l.w} / ${l.h}`;
    return "1 / 1";
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
            hint: (p.kind === "grid" ? 2 : p.kind === "cloud" ? 3 : 1) as PatternDims,
            guess: (p.kind === "grid" ? 2 : p.kind === "cloud" ? 3 : 1) as PatternDims,
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

<div class="tiles" class:bars={!autoStyle && fixture && tileShape($layout) === "bar"}>
  {#each shown as t (t.key)}
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
      data-name={t.name}
      hidden={hiddenBy(t, filter)}
      use:register={t.key}
    >
      <div class="thumb">
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
          <canvas
            class:pts={shape === "cloud" || shape === "scatter"}
            style:aspect-ratio={aspectOf(t, shape)}
            width="96"
            height="96"
          ></canvas>
          {#if t.visible && !t.ready && !t.dead}
            <span
              class="spinner"
              data-role="tile-spinner"
              aria-label="loading"
              title="computing preview…"
            ></span>
          {/if}
        </svelte:element>
        {#if playing}
          <span class="pill" data-role="tile-playing">▶ playing</span>
        {/if}
        <!-- The playing tile keeps its verbs; what it must not offer is
             PLAY, and that is the page's call — `playing` is handed to the
             slot so the strip drops one verb instead of all of them (#555). -->
        {#if $$slots.actions}
          <div class="actions">
            <slot name="actions" item={t} dead={t.dead} {playing} />
          </div>
        {/if}
      </div>
      <div class="meta">
        <div class="nm" data-role="tile-name">{t.name}</div>
        {#if t.dead}<div class="sub err" data-role="tile-dead">does not compile</div>{/if}
        {#if t.rig}
          {@const cap = captionFor(t.dims, t.rig)}
          {#if cap}<div class="sub" data-role="tile-caption">{cap}</div>{/if}
        {/if}
        <slot name="meta" item={t} dead={t.dead} />
      </div>
    </div>
  {/each}
</div>

<style>
  /* Fixed column counts, not `auto-fill`: the mock's own grids (S1 `c6`,
     S1b `c3`, S1c `c2`). Six squares on a matrix console, three 6:1 bars on
     a strip — the tile's SHAPE decides how many fit, not its pixel width. */
  .tiles {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 16px;
    padding: 20px;
    align-content: start;
  }

  .tiles.bars {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .tile[hidden] {
    display: none;
  }

  /* mockups.html `.tile` */
  .tile {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-panel);
    overflow: hidden;
  }

  /* the pattern the device is running right now (proposal §5.1, S1) */
  .tile.playing {
    box-shadow: 0 0 0 2px var(--ok);
    border-color: transparent;
  }

  .tile.dead {
    opacity: 0.35;
  }

  .thumb {
    position: relative;
    background: #000;
    line-height: 0;
  }

  .face {
    display: block;
    position: relative;
    width: 100%;
    padding: 0;
    border: none;
    border-radius: 0;
    background: transparent;
    line-height: 0;
    cursor: pointer;
  }

  span.face {
    cursor: default;
  }

  /* full-bleed: the canvas's intrinsic size is the pixel grid, and the card
     stretches it to its own width at the shape's aspect (`aspectOf`) */
  .tile canvas {
    display: block;
    width: 100%;
    image-rendering: pixelated;
  }

  /* a point cloud is drawn with round dots, not pixels — upscaling those
     with `pixelated` turns every dot into a lego brick */
  .tile canvas.pts {
    image-rendering: auto;
  }

  .meta {
    padding: 8px 10px 10px;
  }

  .nm {
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* mockups.html `.pill`: top-left, dark translucent, --ok text */
  .pill {
    position: absolute;
    top: 8px;
    left: 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 8px;
    border-radius: 999px;
    background: rgba(8, 14, 10, 0.82);
    border: 1px solid rgba(95, 191, 122, 0.55);
    color: var(--ok);
    font: 11px/1 var(--sans);
    pointer-events: none;
  }

  /* per-tile verbs live on the tile they act on, and only on hover (§5.1) —
     a bottom gradient strip, left-aligned, that fades in (mockups.html
     `.actions`). Kept in the layout at `opacity:0` so it never reflows. */
  .actions {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    background: linear-gradient(
      to top,
      rgba(10, 12, 15, 0.94),
      rgba(10, 12, 15, 0.72) 60%,
      transparent
    );
    opacity: 0;
    transition: opacity 0.12s;
    /* The gradient is NOT a click target: it covers the whole thumb of a
       6:1 bar tile, and a click there must still play/open the pattern the
       way the rest of the card does. Only the verbs take the mouse. */
    pointer-events: none;
  }

  .tile:hover .actions,
  .actions:focus-within {
    opacity: 1;
  }

  /* mockups.html `.actions .btn` — the verbs sit on their own dark chip so
     they stay legible over a bright pattern. Slot content, so `:global`. */
  .actions :global(.btn) {
    background: rgba(28, 31, 38, 0.95);
    pointer-events: none;
  }

  .tile:hover .actions :global(.btn),
  .actions:focus-within :global(.btn) {
    pointer-events: auto;
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

  /* mockups.html `.tile .sub` */
  .sub {
    margin-top: 4px;
    font: 11px/1.2 var(--mono);
    color: var(--text-dim);
  }

  /* why a dead tile has no verbs (§5.7: absent, with the reason shown) */
  .sub.err {
    color: var(--error);
  }

  /* Between the mock's two console widths: six squares would be thumbnails
     at 700 px. The mock pins 1200 px (6 / 3 by shape) and 390 px (2). */
  @media (max-width: 1000px) {
    .tiles,
    .tiles.bars {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  /* Mobile (D9, S1c): two columns, and the hover strip is gone — a finger has
     no hover. Tapping the tile plays/opens it; `Edit` moves under the name
     (the page fills the `meta` slot with it). */
  @media (max-width: 600px) {
    /* `minmax(0, 1fr)`, not `1fr`: a column's implicit `auto` minimum is the
       item's min-content, and a long nowrap pattern name would widen it past
       half the screen. */
    .tiles,
    .tiles.bars {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      /* the mock keeps the 16px gutter on a phone and only tightens the
         padding to 12px (S1c `.tiles.c2`) */
      padding: 12px;
    }

    .actions {
      display: none;
    }
  }
</style>
