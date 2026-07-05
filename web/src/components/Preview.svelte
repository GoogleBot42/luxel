<script lang="ts">
  export let layout: { kind: "strip"; pixels: number } | { kind: "grid"; w: number; h: number };

  let strip: HTMLCanvasElement;
  let waterfall: HTMLCanvasElement;
  let grid: HTMLCanvasElement;

  /** Draw one frame of RGB bytes. */
  export function draw(px: Uint8Array): void {
    if (layout.kind === "strip") {
      drawStrip(px);
    } else {
      drawGrid(px, layout.w, layout.h);
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
  {:else}
    <canvas class="grid" bind:this={grid} width={layout.w} height={layout.h}></canvas>
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
</style>
