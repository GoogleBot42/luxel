<script lang="ts">
  // One projection choice, drawn on THIS fixture (proposal §5.4d, S3e–S3h).
  //
  // The card is a live preview: a sample pattern of the row's dimensionality,
  // compiled at the Layout's pixel count and rendered through the projection
  // this card offers — so "Along x" is a picture of the strip stretched
  // across the panel rather than a word. Cheap by construction: the along-axis
  // modes render one strip and replicate it, which is exactly the engine win
  // the row is advertising.
  import { onDestroy } from "svelte";
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
  /** Animate only while the Settings tab is on screen. */
  export let active = false;

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

  function withMode(l: Layout, m: ProjectionMode): Layout["projection"] {
    const d = patternDims >= 3 ? 3 : patternDims === 2 ? 2 : 1;
    return {
      ...l.projection,
      ...(d === 1 ? { proj1d: m } : d === 2 ? { proj2d: m } : { proj3d: m }),
    };
  }

  function costHint(e: Effective, total: number): string {
    const unique = e.pixelCount;
    if (unique < total) {
      const times = Math.round(total / Math.max(1, unique));
      return `${unique} unique pixels${times > 1 ? ` · ${times}× cheaper` : ""}`;
    }
    return `${unique} unique pixels`;
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
    const key = `${luxel === null ? "-" : "x"}:${patternDims}:${mode}:${layout.dims}:${layout.w}x${layout.h}:${layout.pixels}:${layout.serpentine ? "s" : "-"}`;
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
  class="pcard"
  class:on={selected}
  data-role="projection-card"
  data-mode={mode}
  aria-pressed={selected}
  on:click
>
  <span class="pcanv" data-shape={shape}>
    <canvas bind:this={canvas} width="64" height="64"></canvas>
  </span>
  <span class="pnm">{label}</span>
  <span class="phint">{hint}</span>
</button>

<style>
  .pcard {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    cursor: pointer;
    text-align: center;
    color: var(--text);
    font: inherit;
  }

  .pcard.on {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .pcanv {
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

  .pcanv[data-shape="bar"] canvas {
    aspect-ratio: 6;
  }

  .pnm {
    font-size: 12.5px;
  }

  .pcard.on .pnm {
    font-weight: 600;
  }

  .phint {
    font-size: 10.5px;
    line-height: 1.4;
    color: var(--text-dim);
  }
</style>
