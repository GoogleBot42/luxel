<script lang="ts">
  // A small live thumbnail of a single pattern, compiled and animated on the
  // local wasm engine — the same idea as a Gallery tile, but standalone so it
  // can preview device-stored patterns in the Device Patterns list. It renders
  // the pattern locally (device patterns are just source), so it works even
  // while the device streams a different running pattern.
  import { onDestroy, onMount } from "svelte";
  import { Engine, Luxel } from "../lib/luxel";

  export let luxel: Luxel;
  /** Pattern source; `undefined` while the device fetch is still in flight. */
  export let source: string | undefined;
  export let kind: "strip" | "grid" = "strip";

  const STRIP_PX = 64;
  const GRID = 16;
  const FPS_MS = 100; // ~10 fps is plenty for a thumbnail

  let canvas: HTMLCanvasElement | undefined;
  let engine: Engine | undefined;
  let dead = false;
  let ready = false;
  let built = ""; // the source the current engine was built from
  let raf = 0;
  let last = 0;

  function build(src: string): void {
    engine?.free();
    engine = undefined;
    dead = false;
    ready = false;
    const px = kind === "grid" ? GRID * GRID : STRIP_PX;
    const r = luxel.compile(src, px);
    if (r instanceof Engine) {
      if (kind === "grid") r.setMapGrid(GRID, GRID);
      r.setWallClock(Date.now() / 1000);
      engine = r;
    } else {
      dead = true; // won't compile — show a muted placeholder
    }
  }

  function draw(px: Uint8Array): void {
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    if (kind === "grid") {
      const cell = canvas.width / GRID;
      for (let y = 0; y < GRID; y++) {
        for (let x = 0; x < GRID; x++) {
          const i = (y * GRID + x) * 3;
          ctx.fillStyle = `rgb(${px[i]},${px[i + 1]},${px[i + 2]})`;
          ctx.fillRect(x * cell, y * cell, cell + 1, cell + 1);
        }
      }
    } else {
      const w = canvas.width / STRIP_PX;
      for (let x = 0; x < STRIP_PX; x++) {
        const i = x * 3;
        ctx.fillStyle = `rgb(${px[i]},${px[i + 1]},${px[i + 2]})`;
        ctx.fillRect(x * w, 0, w + 1, canvas.height);
      }
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

  // rebuild whenever the source arrives or changes
  $: if (luxel && source !== undefined && source !== built) {
    built = source;
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

<span class="thumb" class:grid={kind === "grid"} class:dead>
  {#if kind === "grid"}
    <canvas bind:this={canvas} width="48" height="48"></canvas>
  {:else}
    <canvas bind:this={canvas} width="72" height="16"></canvas>
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
