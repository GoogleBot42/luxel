<script lang="ts">
  import type { Layout } from "../lib/examples";

  export let layout: Layout;

  let strip: HTMLCanvasElement;
  let waterfall: HTMLCanvasElement;
  let grid: HTMLCanvasElement;
  let map: HTMLCanvasElement;

  /** Blank everything (pattern reset / recompile). */
  export function clear(): void {
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
    if (layout.kind === "strip") {
      drawStrip(px);
    } else if (layout.kind === "grid") {
      drawGrid(px, layout.w, layout.h);
    } else {
      drawMap(px);
    }
  }

  // mapped pixel positions normalized to canvas space, cached per layout
  let mapNorm: { x: number; y: number }[] = [];
  $: if (layout.kind === "map") mapNorm = normalizeMap(layout.coords);

  function normalizeMap(coords: number[][]): { x: number; y: number }[] {
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const c of coords) {
      minX = Math.min(minX, c[0] ?? 0);
      maxX = Math.max(maxX, c[0] ?? 0);
      minY = Math.min(minY, c[1] ?? 0);
      maxY = Math.max(maxY, c[1] ?? 0);
    }
    const sx = maxX - minX || 1;
    const sy = maxY - minY || 1;
    return coords.map((c) => ({
      x: ((c[0] ?? 0) - minX) / sx,
      y: ((c[1] ?? 0) - minY) / sy,
    }));
  }

  function drawMap(px: Uint8Array): void {
    const ctx = map?.getContext("2d");
    if (!map || !ctx) return;
    const { width: w, height: h } = map;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, w, h);
    const pad = 8;
    const r = Math.max(2, Math.min(6, Math.floor(w / Math.sqrt(mapNorm.length) / 4)));
    for (let i = 0; i < mapNorm.length; i++) {
      const p = mapNorm[i];
      if (!p) continue;
      ctx.fillStyle = `rgb(${px[i * 3] ?? 0},${px[i * 3 + 1] ?? 0},${px[i * 3 + 2] ?? 0})`;
      ctx.beginPath();
      ctx.arc(pad + p.x * (w - 2 * pad), pad + p.y * (h - 2 * pad), r, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  function drawStrip(px: Uint8Array): void {
    const n = px.length / 3;
    if (!strip || !waterfall) return;
    const sc = strip.getContext("2d");
    const wc = waterfall.getContext("2d");
    if (!sc || !wc) return;
    if (strip.width !== n) {
      strip.width = n;
      waterfall.width = n;
    }
    // current frame: one row, scaled up by CSS
    const img = sc.createImageData(n, 1);
    for (let i = 0; i < n; i++) {
      img.data[i * 4] = px[i * 3] ?? 0;
      img.data[i * 4 + 1] = px[i * 3 + 1] ?? 0;
      img.data[i * 4 + 2] = px[i * 3 + 2] ?? 0;
      img.data[i * 4 + 3] = 255;
    }
    sc.putImageData(img, 0, 0);
    // waterfall: scroll history down one row (PB preview-strip style)
    wc.drawImage(waterfall, 0, 1);
    wc.putImageData(img, 0, 0);
  }

  function drawGrid(px: Uint8Array, w: number, h: number): void {
    if (!grid) return;
    const gc = grid.getContext("2d");
    if (!gc) return;
    if (grid.width !== w || grid.height !== h) {
      grid.width = w;
      grid.height = h;
    }
    const img = gc.createImageData(w, h);
    const n = Math.min(px.length / 3, w * h);
    for (let i = 0; i < n; i++) {
      img.data[i * 4] = px[i * 3] ?? 0;
      img.data[i * 4 + 1] = px[i * 3 + 1] ?? 0;
      img.data[i * 4 + 2] = px[i * 3 + 2] ?? 0;
      img.data[i * 4 + 3] = 255;
    }
    gc.putImageData(img, 0, 0);
  }
</script>

<div class="preview">
  {#if layout.kind === "strip"}
    <canvas class="strip" bind:this={strip} width={layout.pixels} height="1"></canvas>
    <canvas class="waterfall" bind:this={waterfall} width={layout.pixels} height="160"></canvas>
  {:else if layout.kind === "grid"}
    <canvas class="grid" bind:this={grid} width={layout.w} height={layout.h}></canvas>
  {:else}
    <canvas class="map" bind:this={map} width="320" height="320"></canvas>
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
    border-radius: 6px;
    background: #000;
  }

  .strip {
    height: 26px;
  }

  .waterfall {
    height: 160px;
  }

  .grid {
    aspect-ratio: 1;
    max-height: 320px;
    object-fit: contain;
  }

  .map {
    aspect-ratio: 1;
    max-height: 320px;
    image-rendering: auto; /* smooth dots, unlike the pixelated grid */
  }
</style>
