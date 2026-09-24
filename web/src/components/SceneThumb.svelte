<script lang="ts" context="module">
  // ONE ticker for every scene thumbnail on the page — the playlist's rows and
  // the picker's (Gitea #482). The Scenes page's grid keeps its own for its
  // tiles (`components/scene/SceneGrid.svelte`); the discipline is the same one
  // `Gallery.svelte` uses, and it is the whole design here: a scene thumbnail
  // is a compositor plus one engine PER PATTERN LAYER at the device's size, so
  // a handful of rows is already more engine than the 400-cell `PatternThumb`
  // it sits beside. A budget per animation frame and a cap on live composites
  // is what keeps a playlist of scenes from starving the editor's preview.
  import { SceneRenderer, type SourceLookup } from "../lib/sceneRender";

  /** Live composites at once, over every row on the page. */
  const RENDER_CAP = 8;
  /** Composites stepped per animation frame. */
  const STEP_BUDGET = 2;
  /** How often one thumbnail may advance (~8 fps). */
  const FPS_MS = 125;

  interface Slot {
    build(): void;
    step(now: number): void;
    wants(): boolean;
    due(now: number): boolean;
  }

  const slots: Slot[] = [];
  let live = 0;
  let raf = 0;
  let cursor = 0;

  function admit(): void {
    for (const s of slots) {
      if (live >= RENDER_CAP) return;
      if (s.wants()) {
        live++;
        s.build();
      }
    }
  }

  function tick(now: number): void {
    raf = slots.length > 0 ? requestAnimationFrame(tick) : 0;
    let stepped = 0;
    for (let n = 0; n < slots.length && stepped < STEP_BUDGET; n++) {
      const s = slots[(cursor + n) % slots.length];
      if (!s || !s.due(now)) continue;
      s.step(now);
      stepped++;
    }
    if (slots.length > 0) cursor = (cursor + stepped + 1) % slots.length;
  }

  function join(s: Slot): void {
    slots.push(s);
    if (raf === 0) raf = requestAnimationFrame(tick);
    admit();
  }

  /** Give a build slot back (a failed build, or a rebuild). */
  function release(): void {
    live = Math.max(0, live - 1);
    admit();
  }

  function leave(s: Slot, held: boolean): void {
    const i = slots.indexOf(s);
    if (i >= 0) slots.splice(i, 1);
    if (held) live = Math.max(0, live - 1);
    if (slots.length === 0 && raf !== 0) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
    admit();
  }

  export type { SourceLookup };
  export { SceneRenderer, release, admit };
</script>

<script lang="ts">
  // A live COMPOSITE thumbnail of a scene — the playlist scene row's picture
  // and the picker's (mock S4c).
  //
  // Deliberately the same markup as `PatternThumb.svelte` (`.thumb` wrapping
  // one `canvas.sq`), so every surface that already sizes a thumbnail — the
  // playlist row's `:global(.thumb canvas.sq)` rules, mock S4's 44px and S4b's
  // 40px — sizes this one identically without knowing which kind it is.
  import { onDestroy } from "svelte";
  import { paintGrid } from "../lib/draw";
  import type { Luxel } from "../lib/luxel";
  import type { Scene } from "../lib/scene";
  import { layout as layoutStore, thumbLayout, type Layout } from "../stores/geometry";

  export let luxel: Luxel | null = null;
  /** The scene to composite. */
  export let scene: Scene;
  /** Resolves a layer's stored pattern id to its source (the console's device
   *  library, or the playground's local one) — `lib/sceneRender.ts`. */
  export let lookup: SourceLookup = () => null;
  /** Render on a Layout that is not the app's. */
  export let rig: Layout | null = null;

  let canvas: HTMLCanvasElement | undefined;
  let renderer: SceneRenderer | null = null;
  let ready = false;
  let dead = false;
  let held = false;
  let last = 0;
  let built = "";

  /** A scene is composited on a regular 2D grid; the thumbnail uses the same
   *  tile-sized copy of it the Scenes page's tiles do. */
  $: thumbRig = thumbLayout(rig ?? $layoutStore, 1024);
  /** Rebuild when the record or the rig actually changes — not on every tick. */
  $: key = `${scene.id}:${scene.layers.length}:${thumbRig.w}x${thumbRig.h}:${JSON.stringify(scene.layers)}`;
  $: if (canvas && luxel && key !== built) {
    built = key;
    reset();
  }

  function reset(): void {
    renderer?.free();
    renderer = null;
    ready = false;
    dead = false;
    last = 0;
    if (held) {
      held = false;
      release(); // hands the slot back, and admits whoever was waiting
      return;
    }
    // first paint: the canvas has only just bound, so ask for a slot now
    admit();
  }

  const slot = {
    wants: () => !held && renderer === null && !dead && canvas !== undefined && luxel !== null,
    due: (now: number) => renderer !== null && now - last >= FPS_MS,
    build(): void {
      held = true;
      if (!luxel) {
        held = false;
        release();
        return;
      }
      const r = new SceneRenderer(luxel, thumbRig);
      const err = r.setScene(scene, lookup);
      if (err) {
        r.free();
        dead = true;
        held = false;
        release(); // a scene that will not composite holds no slot
        return;
      }
      renderer = r;
    },
    step(now: number): void {
      const px = renderer?.frame(last === 0 ? 16 : Math.min(now - last, 250));
      last = now;
      if (!px || !canvas) return;
      paintGrid(canvas, px, thumbRig.w, thumbRig.h);
      ready = true;
    },
  };

  join(slot);

  onDestroy(() => {
    leave(slot, held);
    renderer?.free();
    renderer = null;
  });
</script>

<span class="thumb" class:dead data-shape="grid" data-role="scene-thumb">
  <canvas class="sq" bind:this={canvas} width="48" height="48"></canvas>
  {#if !ready && !dead}
    <span class="spinner" data-role="thumb-spinner" aria-label="loading preview"></span>
  {/if}
</span>

<style>
  /* identical to PatternThumb's chrome — a row must not be able to tell the
     two apart (mock S4c draws one canvas for both kinds of row) */
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

  .sq {
    width: 48px;
    height: 48px;
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
    animation: scene-thumb-spin 0.7s linear infinite;
  }

  @keyframes scene-thumb-spin {
    to {
      transform: rotate(1turn);
    }
  }
</style>
