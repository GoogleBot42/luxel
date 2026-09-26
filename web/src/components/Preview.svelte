<script lang="ts">
  // The live preview canvas. It draws whatever shape the ONE Layout says
  // (Gitea #463): a bar on a strip, a pixel grid on a matrix, a rotating
  // point cloud in 3D, a flat scatter for an irregular 2D map. Nothing here
  // decides geometry — `stores/geometry.ts` does, and this component is
  // handed the result. The painters are shared with the tiles (`lib/draw.ts`).
  import { createEventDispatcher, tick } from "svelte";
  import {
    makePanelSurfaces,
    normalizePoints,
    paintBar,
    paintGrid,
    paintGridPanel,
    paintPoints,
    type PanelSurfaces,
    type PreviewStyle,
  } from "../lib/draw";
  import { tileShape, type Layout, type TileShape } from "../lib/geometry";
  import { previewStyle } from "../stores/prefs";

  export let layout: Layout;

  /** bar · grid · cloud · scatter. A dims>1 Layout whose coordinates we do
   *  not know (an irregular map installed before this page loaded — see
   *  `deviceLayout`; closed by #465) has nothing to scatter, so it degrades
   *  to the index bar rather than drawing a lie. */
  $: shape = (layout.coords === undefined && !layout.regular ? "bar" : tileShape(layout)) as TileShape;
  $: points = layout.coords ? normalizePoints(layout.coords) : { pts: [], is3D: false };
  $: is3D = shape === "cloud" && points.is3D;

  // Click/drag anywhere on the preview → an `inject` event with the
  // normalized hit position — the editor feeds it to the engine's event queue
  // (readEvent) and, on a device, forwards it to the strip.
  const dispatch = createEventDispatcher<{ inject: { x: number; y: number } }>();
  let injecting = false;
  let angle = 0;

  function injectAt(e: PointerEvent): void {
    const c = e.currentTarget as HTMLCanvasElement;
    const r = c.getBoundingClientRect();
    const clamp = (v: number) => Math.min(1, Math.max(0, v));
    dispatch("inject", {
      x: clamp((e.clientX - r.left) / r.width),
      // 1D previews (strip + waterfall) only have a meaningful x
      y: shape === "bar" ? 0 : clamp((e.clientY - r.top) / r.height),
    });
  }

  function onDown(e: PointerEvent): void {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    injecting = true;
    injectAt(e);
  }

  function onMove(e: PointerEvent): void {
    if (injecting) injectAt(e);
  }

  function onUp(): void {
    injecting = false;
  }

  let strip: HTMLCanvasElement;
  let waterfall: HTMLCanvasElement;
  let grid: HTMLCanvasElement;
  let map: HTMLCanvasElement;

  /** The LED-panel look (Gitea #786), when the user asked for it and the grid
   *  is small enough to draw as dots — `null` means plain squares, which is
   *  the default and is what every tile and thumbnail always draws. Built once
   *  per geometry: the per-frame work is the same `paintGrid` either way plus
   *  five canvas ops. */
  let panel: PanelSurfaces | null = null;
  let panelKey = "";
  /** The last frame handed to `draw`, so switching the look repaints what is
   *  on screen instead of waiting for the next frame (the editor may be
   *  paused, or the pattern may be static). */
  let lastFrame: Uint8Array | null = null;

  // every input is named in the block itself, so every one is tracked — a
  // bare `$: syncPanel()` would silently stop re-running (.claude/rules/web.md)
  $: syncPanel($previewStyle, shape, layout.w, layout.h);

  function syncPanel(style: PreviewStyle, sh: TileShape, w: number, h: number): void {
    const key = style === "panel" && sh === "grid" ? `${w}x${h}` : "";
    if (key === panelKey) return;
    panelKey = key;
    panel = key ? makePanelSurfaces(w, h) : null;
    // AFTER the DOM update: switching the look changes the canvas's intrinsic
    // size, and writing `width` blanks it — repainting first would be undone.
    // Matters when the preview is paused or the pattern is static; a running
    // one would have covered it in 16 ms.
    void tick().then(() => {
      if (lastFrame && grid) draw(lastFrame);
    });
  }

  /** Blank everything (pattern reset / recompile). */
  export function clear(): void {
    lastFrame = null;
    for (const c of [strip, waterfall, grid, map]) {
      const ctx = c?.getContext("2d");
      if (c && ctx) {
        ctx.fillStyle = "#000";
        ctx.fillRect(0, 0, c.width, c.height);
      }
    }
  }

  /** Draw one frame of RGB bytes. */
  export function draw(px: Uint8Array): void {
    lastFrame = px;
    if (shape === "bar") {
      drawStrip(px);
    } else if (shape === "grid") {
      if (grid) {
        if (panel) paintGridPanel(grid, px, panel);
        else paintGrid(grid, px, layout.w, layout.h);
      }
    } else if (map) {
      if (is3D) angle += 0.012;
      paintPoints(map, px, points, angle);
    }
  }

  /** The bar plus its scrolling history (the PB preview-strip idiom). */
  function drawStrip(px: Uint8Array): void {
    if (!strip || !waterfall) return;
    paintBar(strip, px);
    const wc = waterfall.getContext("2d");
    if (!wc) return;
    if (waterfall.width !== strip.width) waterfall.width = strip.width;
    wc.drawImage(waterfall, 0, 1);
    wc.drawImage(strip, 0, 0);
  }
</script>

<div class="preview" data-role="preview" data-shape={shape}>
  {#if shape === "bar"}
    <canvas
      class="strip"
      bind:this={strip}
      width={layout.pixels}
      height="1"
      on:pointerdown={onDown}
      on:pointermove={onMove}
      on:pointerup={onUp}
      on:pointercancel={onUp}
    ></canvas>
    <canvas
      class="waterfall"
      bind:this={waterfall}
      width={layout.pixels}
      height="160"
      on:pointerdown={onDown}
      on:pointermove={onMove}
      on:pointerup={onUp}
      on:pointercancel={onUp}
    ></canvas>
  {:else if shape === "grid"}
    <canvas
      class="grid"
      class:panel
      data-style={panel ? "panel" : "squares"}
      bind:this={grid}
      width={panel ? panel.rig.width : layout.w}
      height={panel ? panel.rig.height : layout.h}
      on:pointerdown={onDown}
      on:pointermove={onMove}
      on:pointerup={onUp}
      on:pointercancel={onUp}
    ></canvas>
  {:else}
    <canvas
      class="map"
      class:cube={is3D}
      data-3d={is3D}
      bind:this={map}
      width="320"
      height="320"
      on:pointerdown={onDown}
      on:pointermove={onMove}
      on:pointerup={onUp}
      on:pointercancel={onUp}
    ></canvas>
    {#if is3D}<span class="map-3d-badge" data-role="map-3d">3D · auto-rotating</span>{/if}
  {/if}
</div>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  canvas {
    width: 100%;
    image-rendering: pixelated;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: #000;
    /* preview clicks inject events; keep touch drags from scrolling */
    touch-action: none;
    cursor: crosshair;
  }

  .strip {
    height: 26px;
  }

  .waterfall {
    height: 160px;
  }

  /* mockup `.prev`: full width of the rail, square, no height cap — the
     rail is a fixed 360px, so that is a 328px picture */
  .grid {
    aspect-ratio: 1;
    object-fit: contain;
  }

  /* The panel look draws its own dots into a surface several device pixels per
     cell, so the browser must be allowed to SMOOTH it down to the rail's
     ~328px — `pixelated` would alias the discs into lumps (Gitea #786). */
  .grid.panel {
    image-rendering: auto;
  }

  .map {
    aspect-ratio: 1;
    image-rendering: auto; /* smooth dots, unlike the pixelated grid */
  }

  /* S2b: centred at 280px on a phone */
  @media (max-width: 600px) {
    .grid,
    .map {
      max-width: 280px;
      margin: 0 auto;
    }
  }

  .map-3d-badge {
    align-self: center;
    margin-top: 4px;
    font-size: 11px;
    color: var(--text-dim);
  }
</style>
