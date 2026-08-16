<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { Layout } from "../lib/examples";

  export let layout: Layout;

  // Click/drag anywhere on the preview → an `inject` event with the
  // normalized hit position — App feeds it to the engine's event queue
  // (readEvent) and, on a device, forwards it to the strip.
  const dispatch = createEventDispatcher<{ inject: { x: number; y: number } }>();
  let injecting = false;

  function injectAt(e: PointerEvent): void {
    const c = e.currentTarget as HTMLCanvasElement;
    const r = c.getBoundingClientRect();
    const clamp = (v: number) => Math.min(1, Math.max(0, v));
    dispatch("inject", {
      x: clamp((e.clientX - r.left) / r.width),
      // 1D previews (strip + waterfall) only have a meaningful x
      y: layout.kind === "strip" ? 0 : clamp((e.clientY - r.top) / r.height),
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

  // mapped pixel positions, normalized to a centered unit cube, cached per
  // layout. `is3D` is true when the z axis actually varies → a rotating cloud.
  let mapNorm: { x: number; y: number; z: number }[] = [];
  let map3D = false;
  let mapAngle = 0;
  $: if (layout.kind === "map") {
    const r = normalizeMap(layout.coords);
    mapNorm = r.pts;
    map3D = r.is3D;
  }

  function normalizeMap(coords: number[][]): {
    pts: { x: number; y: number; z: number }[];
    is3D: boolean;
  } {
    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (const c of coords) {
      for (let d = 0; d < 3; d++) {
        const v = c[d] ?? 0;
        lo[d] = Math.min(lo[d]!, v);
        hi[d] = Math.max(hi[d]!, v);
      }
    }
    const is3D = hi[2]! - lo[2]! > 1e-6;
    const s = [hi[0]! - lo[0]! || 1, hi[1]! - lo[1]! || 1, hi[2]! - lo[2]! || 1];
    // centered on 0 so rotation is about the middle; y flipped (screen down)
    const pts = coords.map((c) => ({
      x: ((c[0] ?? 0) - lo[0]!) / s[0]! - 0.5,
      y: ((c[1] ?? 0) - lo[1]!) / s[1]! - 0.5,
      z: is3D ? ((c[2] ?? 0) - lo[2]!) / s[2]! - 0.5 : 0,
    }));
    return { pts, is3D };
  }

  function drawMap(px: Uint8Array): void {
    const ctx = map?.getContext("2d");
    if (!map || !ctx) return;
    const { width: w, height: h } = map;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, w, h);
    const baseR = Math.max(2, Math.min(7, Math.floor(w / Math.sqrt(mapNorm.length) / 4)));

    if (!map3D) {
      // flat 2D scatter
      const pad = 8;
      const span = w - 2 * pad;
      for (let i = 0; i < mapNorm.length; i++) {
        const p = mapNorm[i];
        if (!p) continue;
        ctx.fillStyle = `rgb(${px[i * 3] ?? 0},${px[i * 3 + 1] ?? 0},${px[i * 3 + 2] ?? 0})`;
        ctx.beginPath();
        ctx.arc(pad + (p.x + 0.5) * span, pad + (p.y + 0.5) * span, baseR, 0, Math.PI * 2);
        ctx.fill();
      }
      return;
    }

    // 3D: slowly rotate about the vertical axis, fixed tilt, orthographic
    // projection, painter's algorithm + depth cue (farther = smaller/dimmer).
    mapAngle += 0.012;
    const cx = w / 2;
    const cy = h / 2;
    const scale = Math.min(w, h) * 0.72;
    const ca = Math.cos(mapAngle);
    const sa = Math.sin(mapAngle);
    const tilt = 0.45;
    const ct = Math.cos(tilt);
    const st = Math.sin(tilt);
    const proj: { sx: number; sy: number; depth: number; i: number }[] = [];
    for (let i = 0; i < mapNorm.length; i++) {
      const p = mapNorm[i];
      if (!p) continue;
      const x = p.x * ca - p.z * sa; // rotate about Y
      const z = p.x * sa + p.z * ca;
      const y2 = p.y * ct - z * st; // tilt about X
      const z2 = p.y * st + z * ct;
      proj.push({ sx: cx + x * scale, sy: cy + y2 * scale, depth: z2, i });
    }
    proj.sort((a, b) => a.depth - b.depth); // back to front
    for (const q of proj) {
      const cue = 0.55 + 0.45 * (q.depth + 0.6); // ~0.55..1.15
      const bri = Math.max(0.35, Math.min(1, cue));
      const rr = baseR * Math.max(0.6, Math.min(1.3, cue));
      const r = Math.round((px[q.i * 3] ?? 0) * bri);
      const g = Math.round((px[q.i * 3 + 1] ?? 0) * bri);
      const b = Math.round((px[q.i * 3 + 2] ?? 0) * bri);
      ctx.fillStyle = `rgb(${r},${g},${b})`;
      ctx.beginPath();
      ctx.arc(q.sx, q.sy, rr, 0, Math.PI * 2);
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
  {:else if layout.kind === "grid"}
    <canvas
      class="grid"
      bind:this={grid}
      width={layout.w}
      height={layout.h}
      on:pointerdown={onDown}
      on:pointermove={onMove}
      on:pointerup={onUp}
      on:pointercancel={onUp}
    ></canvas>
  {:else}
    <canvas
      class="map"
      class:cube={map3D}
      data-3d={map3D}
      bind:this={map}
      width="320"
      height="320"
      on:pointerdown={onDown}
      on:pointermove={onMove}
      on:pointerup={onUp}
      on:pointercancel={onUp}
    ></canvas>
    {#if map3D}<span class="map-3d-badge" data-role="map-3d">3D · auto-rotating</span>{/if}
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

  .map-3d-badge {
    align-self: center;
    margin-top: 4px;
    font-size: 11px;
    color: var(--text-dim);
  }
</style>
