<script lang="ts">
  // One projection choice, drawn on THIS fixture (proposal §5.4d, mockups
  // S3e–S3h).
  //
  // The card is a live preview: a sample pattern of the row's dimensionality,
  // compiled at the Layout's pixel count and rendered through the projection
  // this card offers — so "Along x" is a picture of the strip stretched
  // across the panel rather than a word. Cheap by construction: the along-axis
  // modes render one strip and replicate it, which is exactly the engine win
  // the row is advertising.
  //
  // Two forms, chosen by the FIXTURE's shape (the mockups draw both):
  //  * a tile (`.pcard`, S3e/S3g/S3h) — a 120px square of the panel/cloud;
  //  * a row (`.barcard`, S3f) — a 200×24 bar, because that is a strip's
  //    shape and a square picture of a strip is a lie.
  // A glyph says where the picture came from where the art alone cannot: the
  // cube badge on a slice, the grid/cube diagram beside a bar.
  //
  // Reused by the editor's per-item projection popup
  // (`components/ProjectionRow.svelte`, #538), so the props are a contract:
  // `luxel`, `layout`, `patternDims`, `mode`, `label`, `selected`, `active`.
  // A pick is BOTH a forwarded `click` and a `select` — the editor's popup
  // listens for the former, Settings for the latter, and dropping either
  // silently stops one caller's cards from doing anything.
  import { createEventDispatcher, onDestroy } from "svelte";
  import { normalizePoints, paintBar, paintGrid, paintPoints, type PointRig } from "../lib/draw";
  import { Engine, Luxel } from "../lib/luxel";
  import {
    configureEngine,
    effectiveFor,
    type Effective,
    TILE_MAX_CELLS,
    thumbLayout,
    tileShape,
    type Layout,
    type PatternDims,
    type ProjectionMode,
    type TileShape,
  } from "../stores/geometry";

  export let luxel: Luxel | null;
  /** The fixture's Layout — the projection field is replaced per card. */
  export let layout: Layout;
  /** Which kind of pattern this row is about. */
  export let patternDims: PatternDims;
  export let mode: ProjectionMode;
  export let label: string;
  export let selected = false;
  /** Animate only while the surface is on screen. */
  export let active = false;

  const dispatch = createEventDispatcher<{ select: void }>();

  const FPS_MS = 120;
  const SAMPLE: Record<number, string> = {
    1: "export function render(index) { hsv(index / pixelCount, 1, 1) }",
    2: "export function render2D(index, x, y) { hsv(x * 0.6 + y * 0.4, 1, 1) }",
    3: "export function render3D(index, x, y, z) { hsv(x * 0.5 + z * 0.5, 1, 1 - y * 0.4) }",
  };

  let canvas: HTMLCanvasElement | undefined;
  let engine: Engine | undefined;
  let rig: Layout | undefined;
  let shape: TileShape = "bar";
  let points: PointRig = { pts: [], is3D: false };
  let angle = 0;
  let raf = 0;
  let last = 0;
  let builtKey = "";

  /** What the pattern actually sees here — the card's own cost caption.
   *  `hint` names `eff` and `layout` in the block itself: a `$:` only tracks
   *  what its own syntax mentions, and `$: hint = costHint()` would be free to
   *  run BEFORE `eff` exists (.claude/rules/web.md). */
  $: eff = effectiveFor(patternDims, { ...layout, projection: withMode(layout, mode) });
  $: hint = costHint(eff, layout.pixels);
  /** A strip fixture gets the row form (mockup S3f). */
  $: barForm = layout.dims === 1;
  /** The 40×40 diagram: which cut of a cube, or which line of a grid. The
   *  mockups draw one wherever the picture alone does not say (S3e's 3D row,
   *  S3f's bars); a 1D-pattern card on a grid or a lattice has none. */
  $: glyph = glyphFor(patternDims, mode, layout.dims);

  function withMode(l: Layout, m: ProjectionMode): Layout["projection"] {
    const d = patternDims >= 3 ? 3 : patternDims === 2 ? 2 : 1;
    return {
      ...l.projection,
      ...(d === 1 ? { proj1d: m } : d === 2 ? { proj2d: m } : { proj3d: m }),
    };
  }

  /** What the mode DOES, in the mockups' words — `rows`, `stretched across`,
   *  `the xy image on every layer`. The cost follows it; together they are
   *  the card's `.phint` (S3e/S3f/S3g). "" where the picture says it. */
  function whatHint(pd: PatternDims, m: ProjectionMode, ld: number): string {
    if (pd <= 1) {
      if (m === "index") return ld === 3 ? "layer by layer, row by row" : "rows";
      if (m === "x") return ld === 3 ? "one strip along x, repeated over y and z" : "stretched across";
      if (m === "y") return ld === 3 ? "repeated over x and z" : "stretched down";
      if (m === "z") return "repeated over x and y";
      return "";
    }
    if (pd === 2) {
      if (ld === 3) {
        if (m === "z") return "the xy image on every layer";
        if (m === "y") return "the xz image at every y";
        if (m === "x") return "the yz image at every x";
      }
      if (m === "x") return "the strip shows y = 0.5";
      if (m === "y") return "the strip shows x = 0.5";
      return "";
    }
    if (m === "xy") return "z = 0.5";
    if (m === "xz") return "y = 0.5";
    if (m === "yz") return "x = 0.5";
    if (m === "x") return "line through the centre, y = z = 0.5";
    if (m === "y") return "line through the centre, x = z = 0.5";
    if (m === "z") return "line through the centre, x = y = 0.5";
    return "";
  }

  function costHint(e: Effective, total: number): string {
    const what = whatHint(patternDims, mode, e.layoutDims);
    const unique = e.pixelCount;
    // Only a 1D pattern's projection changes what it COSTS — stretching one
    // along an axis renders a strip instead of the whole fixture. A slice or
    // an extrusion runs the same pattern over the same pixels whichever way
    // it is cut, so the mockups print the cut and no number (S3e, S3g).
    if (patternDims >= 2) return what;
    const times = Math.round(total / Math.max(1, unique));
    const cost = `${unique} unique pixels${unique < total && times > 1 ? ` · ${times}× cheaper` : ""}`;
    return what ? `${what} · ${cost}` : cost;
  }

  /** Which glyph, if any: `cube-<face>` for a slice of a 3D pattern,
   *  `cube-line-<axis>` for a line through one, `grid-<axis>` for a row or a
   *  column of a 2D one. */
  function glyphFor(pd: PatternDims, m: ProjectionMode, ld: number): string {
    // A 3D pattern's picture cannot say WHICH cut it is, so a cube says it
    // (S3e, S3h); on a strip the bar cannot say where the line came from
    // either (S3f). Everywhere else the art is the answer and the mockups
    // draw no glyph — a lattice card IS the lattice (S3g).
    if (pd >= 3 && ld === 2 && (m === "xy" || m === "xz" || m === "yz")) return `cube-${m}`;
    if (pd >= 3 && ld === 1 && (m === "x" || m === "y" || m === "z")) return `cube-line-${m}`;
    if (pd === 2 && ld === 1 && (m === "x" || m === "y")) return `grid-${m}`;
    return "";
  }

  function build(): void {
    engine?.free();
    engine = undefined;
    const lx = luxel;
    const src = SAMPLE[patternDims >= 3 ? 3 : patternDims === 2 ? 2 : 1];
    if (!lx || !src) return;
    const want = thumbLayout({ ...layout, projection: withMode(layout, mode) }, TILE_MAX_CELLS);
    const e = lx.compile(src, want.pixels);
    if (!(e instanceof Engine)) return;
    configureEngine(e, want);
    engine = e;
    rig = want;
    shape = want.coords === undefined && !want.regular ? "bar" : tileShape(want);
    points = want.coords ? normalizePoints(want.coords) : { pts: [], is3D: false };
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || !canvas || !rig || now - last < FPS_MS) return;
    const dt = last === 0 ? 16 : Math.min(now - last, 200);
    last = now;
    const px = engine.frame(dt);
    if (shape === "grid") paintGrid(canvas, px, rig.w, rig.h);
    else if (shape === "bar") paintBar(canvas, px);
    else {
      if (points.is3D) angle += 0.02;
      paintPoints(canvas, px, points, angle);
    }
    engine.takeError();
  }

  /** Rebuild on any input that changes the compiled engine, and start/stop
   *  the loop with the tab — every dependency named in the block itself.
   *  Nothing is compiled until the tab is first LOOKED at: a console mounts
   *  Settings on connect, and six wasm engines is not a price to pay for a
   *  page nobody opened. Once built they stay, so returning is instant. */
  $: {
    const key = `${luxel === null ? "-" : "x"}:${patternDims}:${mode}:${layout.dims}:${layout.w}x${layout.h}x${layout.d}:${layout.pixels}:${layout.serpentine ? "s" : "-"}`;
    if (active && key !== builtKey) {
      builtKey = key;
      build();
    }
    cancelAnimationFrame(raf);
    if (active && engine) raf = requestAnimationFrame(loop);
  }

  onDestroy(() => {
    cancelAnimationFrame(raf);
    engine?.free();
  });
</script>

<button
  class:pcard={!barForm}
  class:barcard={barForm}
  class:on={selected}
  data-role="projection-card"
  data-mode={mode}
  aria-pressed={selected}
  on:click
  on:click={() => dispatch("select")}
>
  <span class="pcanv" data-shape={shape}>
    {#if glyph}
    <!-- the mockups' 40×40 diagrams: a cube with one face cut, a cube with
         one axis drawn, or a grid with one line picked out -->
    <svg class="gl" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true">
      {#if glyph.startsWith("cube")}
        <polygon
          points="20,7 33,14.5 20,22 7,14.5"
          fill={glyph === "cube-xy" ? "rgba(232,163,61,.42)" : "none"}
          stroke={glyph === "cube-xy" ? "#e8a33d" : "#7e8798"}
          stroke-width={glyph === "cube-xy" ? "1.3" : "1"}
        />
        <polygon
          points="7,14.5 20,22 20,35 7,27.5"
          fill={glyph === "cube-xz" ? "rgba(232,163,61,.42)" : "none"}
          stroke={glyph === "cube-xz" ? "#e8a33d" : "#7e8798"}
          stroke-width={glyph === "cube-xz" ? "1.3" : "1"}
        />
        <polygon
          points="33,14.5 20,22 20,35 33,27.5"
          fill={glyph === "cube-yz" ? "rgba(232,163,61,.42)" : "none"}
          stroke={glyph === "cube-yz" ? "#e8a33d" : "#7e8798"}
          stroke-width={glyph === "cube-yz" ? "1.3" : "1"}
        />
        {#if glyph === "cube-line-x"}
          <line x1="7" y1="13.5" x2="33" y2="28.5" stroke="#e8a33d" stroke-width="2.2" stroke-linecap="round" />
        {:else if glyph === "cube-line-y"}
          <line x1="33" y1="13.5" x2="7" y2="28.5" stroke="#e8a33d" stroke-width="2.2" stroke-linecap="round" />
        {:else if glyph === "cube-line-z"}
          <line x1="20" y1="6" x2="20" y2="36" stroke="#e8a33d" stroke-width="2.2" stroke-linecap="round" />
        {/if}
      {:else}
        <rect x="7" y="7" width="26" height="26" fill="none" stroke="#5c6472" />
        <line x1="13.5" y1="7" x2="13.5" y2="33" stroke="#4a5160" />
        <line x1="26.5" y1="7" x2="26.5" y2="33" stroke="#4a5160" />
        <line x1="7" y1="13.5" x2="33" y2="13.5" stroke="#4a5160" />
        <line x1="7" y1="26.5" x2="33" y2="26.5" stroke="#4a5160" />
        {#if glyph === "grid-x"}
          <line x1="20" y1="7" x2="20" y2="33" stroke="#4a5160" />
          <line x1="7" y1="20" x2="33" y2="20" stroke="#e8a33d" stroke-width="2.4" />
        {:else}
          <line x1="7" y1="20" x2="33" y2="20" stroke="#4a5160" />
          <line x1="20" y1="7" x2="20" y2="33" stroke="#e8a33d" stroke-width="2.4" />
        {/if}
      {/if}
    </svg>
    {/if}
    <canvas
      bind:this={canvas}
      class:smooth={shape === "cloud" || shape === "scatter"}
      width={shape === "cloud" || shape === "scatter" ? 128 : 64}
      height={shape === "cloud" || shape === "scatter" ? 128 : 64}
    ></canvas>
  </span>
  <span class="nm">
    <span class="pnm">{label}</span>
    <span class="phint">{hint}</span>
  </span>
</button>

<style>
  /* ---- the tile form (mockups S3e/S3g/S3h `.pcard`) ---- */
  .pcard {
    display: block;
    position: relative;
    /* the editor's popup puts the card inside a `.cardslot` wrapper rather
       than straight into a grid track, and a `display:block` button shrinks
       to its content unless it is told otherwise */
    width: 100%;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    cursor: pointer;
    text-align: center;
    color: var(--text);
    font: inherit;
  }

  .pcard.on,
  .barcard.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: 0 0 0 1px rgba(232, 163, 61, 0.3);
  }

  .pcanv {
    position: relative;
    display: block;
    line-height: 0;
  }

  canvas {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    border-radius: 4px;
    background: #000;
    image-rendering: pixelated;
  }

  /* mockup `.pcard canvas`: 120px, centred — it does not grow with the card */
  .pcard canvas {
    max-width: 120px;
    margin: 0 auto;
  }

  canvas.smooth {
    image-rendering: auto;
  }

  .pcanv[data-shape="bar"] canvas {
    aspect-ratio: 6;
  }

  .pcard .nm {
    display: block;
  }

  .pnm {
    display: block;
    margin-top: 10px;
    font-size: 13px;
  }

  .on .pnm {
    font-weight: 600;
  }

  .phint {
    display: block;
    margin-top: 5px;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-dim);
    white-space: normal;
  }

  /* the badge in the picture's lower-right corner (mockup S3e `.gl`) */
  .pcard .gl {
    position: absolute;
    right: 8px;
    bottom: 6px;
    z-index: 1;
    background: rgba(8, 10, 14, 0.74);
    border-radius: 5px;
  }

  /* ---- the row form (mockup S3f `.barcard`) ---- */
  .barcard {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    cursor: pointer;
    text-align: left;
    color: var(--text);
    font: inherit;
  }

  /* a bar's glyph sits BESIDE the picture rather than on it (mockup S3f) */
  .barcard .pcanv {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .barcard .gl {
    flex: none;
  }

  .barcard canvas {
    width: 200px;
    height: 24px;
    aspect-ratio: auto;
    border-radius: 3px;
  }

  .barcard .nm {
    min-width: 0;
  }

  .barcard .pnm {
    margin-top: 0;
  }

  .barcard .phint {
    margin-top: 3px;
  }
</style>
