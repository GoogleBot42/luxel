<script lang="ts">
  // THE drawing surface (Gitea #741). Jeremy: "the sprite draw tools are sad,
  // have tiny confusing buttons, and incredibly unintuitive. It needs a full
  // redesign." This is the centre of that redesign: one big canvas, zoomed to
  // fit at an INTEGER factor so a texel is a square you can hit, on a
  // checkerboard that says which texels are transparent.
  //
  // It reports CELLS and modifiers; it does not know what a tool is. The page
  // owns the tools, the palette and undo, so "shift is a temporary
  // eyedropper" and "the right button erases" are decided in one place —
  // here, where the pointer is — and applied in one place: the page.
  //
  // Pointer CAPTURE on pointerdown, so a stroke that leaves the canvas keeps
  // painting and, more importantly, its `pointerup` still arrives — an undo
  // step is one stroke, and a stroke that never ends never closes its step.
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import { renderFrame, type Sprite } from "../../lib/sprite";

  export let sprite: Sprite;
  /** The frame being edited. */
  export let frame = 0;
  /** Ghost the previous frame at ~35 % (the strip's `Onion` toggle). */
  export let onion = false;
  /** Smallest texel the canvas will draw, in CSS pixels. */
  export let minZoom = 4;
  /** The tallest the picture may get, in CSS pixels — the height half of the
   *  fit. It is a NUMBER and not a measurement on purpose: the wrapper's own
   *  height is content-driven (the canvas is the content), so measuring it
   *  would be a feedback loop. 460 is the scene stage's size, so the two
   *  screens' pictures come out the same size. */
  export let maxHeight = 460;

  const dispatch = createEventDispatcher<{
    /** A texel was touched. `phase` is `down` for the first one of a stroke. */
    cell: { col: number; row: number; phase: "down" | "move"; erase: boolean; sample: boolean };
    /** The stroke ended — the page closes its undo step here. */
    end: void;
  }>();

  let canvas: HTMLCanvasElement | undefined;
  let wrap: HTMLElement | undefined;
  /** The width the wrapper offers, measured — the integer zoom follows it. */
  let avail = 0;
  let drawing = false;

  /** Zoom to FIT at an integer factor, never below `minZoom`: a 64×64 sprite
   *  in a 460px column is 7 px a texel, a 8×8 one is 57. Recomputed whenever
   *  the column changes width (the ResizeObserver below), because the scene
   *  editor's three columns reflow at 1000px and 600px. */
  $: zoom = Math.max(
    minZoom,
    Math.min(
      Math.floor(Math.max(1, avail) / Math.max(1, sprite.w)),
      Math.floor(maxHeight / Math.max(1, sprite.h)),
    ),
  );
  $: cssW = sprite.w * zoom;
  $: cssH = sprite.h * zoom;

  // Every dependency NAMED and the paint in a FUNCTION: a `$:` that assigned
  // what it reads would be its own dependency (.claude/rules/web.md).
  $: paint(sprite, frame, onion, zoom);

  function paint(..._deps: unknown[]): void {
    const c = canvas;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const dpr = typeof devicePixelRatio === "number" ? devicePixelRatio : 1;
    const want = Math.max(1, Math.round(cssW * dpr));
    const wantH = Math.max(1, Math.round(cssH * dpr));
    if (c.width !== want || c.height !== wantH) {
      c.width = want;
      c.height = wantH;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);
    checker(ctx);
    if (onion && frame > 0) {
      ctx.globalAlpha = 0.35;
      blit(ctx, frame - 1);
      ctx.globalAlpha = 1;
    }
    blit(ctx, frame);
    if (zoom >= 8) grid(ctx);
  }

  /** The transparency ground: 8-texel squares, so it reads as a checker at
   *  every zoom rather than as noise. */
  function checker(ctx: CanvasRenderingContext2D): void {
    ctx.fillStyle = "#15171c";
    ctx.fillRect(0, 0, cssW, cssH);
    ctx.fillStyle = "#1e222a";
    for (let row = 0; row < sprite.h; row++) {
      for (let col = 0; col < sprite.w; col++) {
        if (((col + row) & 1) === 0) continue;
        ctx.fillRect(col * zoom, row * zoom, zoom, zoom);
      }
    }
  }

  let scratch: HTMLCanvasElement | undefined;

  function blit(ctx: CanvasRenderingContext2D, f: number): void {
    if (f < 0 || f >= sprite.frames) return;
    if (!scratch) scratch = document.createElement("canvas");
    if (scratch.width !== sprite.w || scratch.height !== sprite.h) {
      scratch.width = sprite.w;
      scratch.height = sprite.h;
    }
    const sctx = scratch.getContext("2d");
    if (!sctx) return;
    sctx.clearRect(0, 0, sprite.w, sprite.h);
    // `createImageData` + `set` rather than `new ImageData(bytes, w, h)`: the
    // constructor's typing demands a `Uint8ClampedArray<ArrayBuffer>` and
    // `renderFrame`'s is the plain (ArrayBufferLike) one.
    const img = sctx.createImageData(sprite.w, sprite.h);
    img.data.set(renderFrame(sprite, f));
    sctx.putImageData(img, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(scratch, 0, 0, cssW, cssH);
  }

  /** Faint texel rules, only once a texel is big enough for them not to be
   *  most of what you see. */
  function grid(ctx: CanvasRenderingContext2D): void {
    ctx.strokeStyle = "rgba(255,255,255,0.07)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let col = 1; col < sprite.w; col++) {
      ctx.moveTo(col * zoom + 0.5, 0);
      ctx.lineTo(col * zoom + 0.5, cssH);
    }
    for (let row = 1; row < sprite.h; row++) {
      ctx.moveTo(0, row * zoom + 0.5);
      ctx.lineTo(cssW, row * zoom + 0.5);
    }
    ctx.stroke();
  }

  function cellAt(e: PointerEvent): { col: number; row: number } {
    const r = (canvas as HTMLCanvasElement).getBoundingClientRect();
    const col = Math.floor(((e.clientX - r.left) / r.width) * sprite.w);
    const row = Math.floor(((e.clientY - r.top) / r.height) * sprite.h);
    return {
      col: Math.max(0, Math.min(sprite.w - 1, col)),
      row: Math.max(0, Math.min(sprite.h - 1, row)),
    };
  }

  function down(e: PointerEvent): void {
    e.preventDefault();
    // best-effort: a synthetic pointerdown (a harness staging a stroke) has no
    // active pointer to capture, and the throw would kill the stroke
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    drawing = true;
    const c = cellAt(e);
    dispatch("cell", { ...c, phase: "down", erase: e.button === 2, sample: e.shiftKey });
  }

  function move(e: PointerEvent): void {
    if (!drawing) return;
    const c = cellAt(e);
    dispatch("cell", { ...c, phase: "move", erase: (e.buttons & 2) !== 0, sample: e.shiftKey });
  }

  function up(e: PointerEvent): void {
    if (!drawing) return;
    drawing = false;
    // best-effort, and the mirror of `setPointerCapture` above: a synthetic
    // stroke never captured a real pointer, and `releasePointerCapture` THROWS
    // (`NotFoundError`) rather than returning for an id it does not hold — an
    // uncaught one there would fail e2e's "no page errors" while the stroke
    // itself worked.
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    dispatch("end");
  }

  let ro: ResizeObserver | undefined;

  onMount(() => {
    measure();
    if (typeof ResizeObserver !== "undefined" && wrap) {
      ro = new ResizeObserver(() => measure());
      ro.observe(wrap);
    }
    paint();
  });

  onDestroy(() => ro?.disconnect());

  /** `avail` is assigned in a function, never inside the `$:` that reads it. */
  function measure(): void {
    const w = wrap?.clientWidth ?? 0;
    if (w > 0 && w !== avail) avail = w;
  }
</script>

<div class="cwrap" bind:this={wrap} data-role="sprite-canvas-wrap">
  <!-- `touch-action:none` so a drag on a phone paints instead of scrolling the
       page out from under the stroke; `contextmenu` suppressed because the
       right button is the eraser. -->
  <canvas
    bind:this={canvas}
    data-role="sprite-canvas"
    data-zoom={zoom}
    style={`width:${cssW}px;height:${cssH}px`}
    on:pointerdown={down}
    on:pointermove={move}
    on:pointerup={up}
    on:pointercancel={up}
    on:contextmenu={(e) => e.preventDefault()}
  ></canvas>
</div>

<style>
  .cwrap {
    display: flex;
    justify-content: center;
    min-width: 0;
  }

  canvas {
    display: block;
    border: 1px solid var(--border);
    border-radius: 4px;
    image-rendering: pixelated;
    touch-action: none;
    cursor: crosshair;
    max-width: 100%;
  }
</style>
