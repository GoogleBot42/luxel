<script lang="ts">
  // A small live thumbnail of a single pattern, compiled and animated on the
  // local wasm engine — the same idea as a Gallery tile, but standalone so it
  // can preview device-stored patterns in the Device Patterns list and the
  // playlist rows. It renders the pattern locally (device patterns are just
  // source), so it works even while the device runs a different one.
  //
  // Its SHAPE is the Layout's (Gitea #463): a bar on a strip console, a grid
  // on a matrix, a cloud/scatter in 3D or on a custom map — never the fixed
  // 64-px bar it used to be on every board. Geometry comes from
  // `stores/geometry.ts`, so a row, a tile and the editor's preview agree.
  import { onDestroy, onMount } from "svelte";
  import { normalizePoints, paintBar, paintGrid, paintPoints, type PointRig } from "../lib/draw";
  import { Engine, Luxel } from "../lib/luxel";
  import {
    captionFor,
    compileForLayout,
    layoutSignature,
    THUMB_MAX_CELLS,
    tileShape,
    type Layout,
    type PatternDims,
    type ProjectionMode,
    type TileShape,
  } from "../stores/geometry";

  export let luxel: Luxel;
  /** Pattern source; `undefined` while the device fetch is still in flight. */
  export let source: string | undefined;
  /** Per-item projection override (a playlist row's, §5.4d) — null = the
   *  device default. The thumbnail has to render through it or it shows a
   *  different picture from the one the device will play. */
  export let proj: ProjectionMode | null = null;
  /** `row` is the thumbnail a playlist row or a list entry carries; `summary`
   *  is the larger one the Settings page's LED layout headline sits beside
   *  (Gitea #469). Same engine, same shape — only the CSS size differs. */
  export let size: "row" | "summary" = "row";

  const FPS_MS = 100; // ~10 fps is plenty for a thumbnail

  let canvas: HTMLCanvasElement | undefined;
  let engine: Engine | undefined;
  let rig: Layout | undefined;
  let dims: PatternDims = 0;
  let dead = false;
  let ready = false;
  let built = ""; // the source the current engine was built from
  let builtRig = ""; // the Layout it was built for
  let builtProj: ProjectionMode | null = null; // the override it was built with
  let raf = 0;
  let last = 0;
  let angle = 0;

  /** Everything derived from the rig is assigned WITH it, in `build()` — a
   *  `$:` here would be ordered before the reactive block that calls `build`
   *  and could render one frame behind the Layout it drew. */
  let shape: TileShape = "bar";
  let points: PointRig = { pts: [], is3D: false };
  /** `1D · by index` when the pattern is not native to this Layout. */
  let caption: string | null = null;
  /** Rebuild when the Layout actually moves, not on every store tick. */
  $: rigKey = $layoutSignature;

  function adopt(l: Layout | undefined, d: PatternDims): void {
    rig = l;
    dims = d;
    // a Layout whose coordinates we don't have has nothing to scatter
    shape = l === undefined ? "bar" : l.coords === undefined && !l.regular ? "bar" : tileShape(l);
    points = l?.coords ? normalizePoints(l.coords) : { pts: [], is3D: false };
    caption = l ? captionFor(d, l) : null;
  }

  function build(src: string): void {
    engine?.free();
    engine = undefined;
    dead = false;
    ready = false;
    const r = compileForLayout(luxel, src, THUMB_MAX_CELLS, proj);
    if ("engine" in r) {
      r.engine.setWallClock(Date.now() / 1000);
      engine = r.engine;
      adopt(r.layout, r.dims);
    } else {
      dead = true; // won't compile — show a muted placeholder
      adopt(undefined, 0);
    }
  }

  function draw(px: Uint8Array): void {
    if (!canvas || !rig) return;
    if (shape === "grid") paintGrid(canvas, px, rig.w, rig.h);
    else if (shape === "bar") paintBar(canvas, px);
    else {
      if (points.is3D) angle += 0.02;
      paintPoints(canvas, px, points, angle);
    }
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || now - last < FPS_MS) return;
    const dt = last === 0 ? 16 : Math.min(now - last, 100);
    last = now;
    draw(engine.frame(dt));
    engine.takeError(); // tolerate runtime errors; many patterns recover
    ready = true;
  }

  // rebuild whenever the source arrives or changes, the Layout moves, or the
  // per-item projection override changes
  $: if (
    luxel &&
    source !== undefined &&
    (source !== built || rigKey !== builtRig || proj !== builtProj)
  ) {
    built = source;
    builtRig = rigKey;
    builtProj = proj;
    build(source);
  }

  onMount(() => {
    raf = requestAnimationFrame(loop);
  });
  onDestroy(() => {
    cancelAnimationFrame(raf);
    engine?.free();
  });
</script>

<span
  class="thumb"
  class:dead
  class:summary={size === "summary"}
  data-shape={shape}
  title={caption ?? undefined}
>
  {#if shape === "bar"}
    <canvas class="bar" bind:this={canvas} width="64" height="1"></canvas>
  {:else}
    <canvas class="sq" bind:this={canvas} width="48" height="48"></canvas>
  {/if}
  {#if source === undefined || (!ready && !dead)}
    <span class="spinner" data-role="thumb-spinner" aria-label="loading preview"></span>
  {/if}
</span>

<style>
  .thumb {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
  }

  canvas {
    image-rendering: pixelated;
    border-radius: 3px;
    background: #000;
  }

  /* the canvas's intrinsic size is the pixel grid; CSS fixes how big it looks */
  .bar {
    width: 72px;
    height: 16px;
  }

  .sq {
    width: 48px;
    height: 48px;
  }

  /* the Settings LED-layout summary (#469): same engine, more room */
  .thumb.summary .bar {
    width: 148px;
    height: 26px;
  }

  .thumb.summary .sq {
    width: 76px;
    height: 76px;
  }

  .thumb.dead canvas {
    opacity: 0.3;
  }

  .spinner {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: thumb-spin 0.7s linear infinite;
  }

  @keyframes thumb-spin {
    to {
      transform: rotate(1turn);
    }
  }
</style>
