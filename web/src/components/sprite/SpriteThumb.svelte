<script lang="ts">
  // A sprite's PICTURE — the Sprites tab's tile, the sprite editor's 1:1
  // Preview, and (#741) the frame strip's thumbnails.
  //
  // It is a canvas and nothing else: a sprite is palette-indexed texels, so
  // drawing one is `putImageData` into a `w`×`h` scratch canvas and one
  // `drawImage` with smoothing off. No wasm, no engine — which is the whole
  // point of #740.
  //
  // ONE shared rAF ticker for every mounted thumb (the module block below),
  // for the reason `components/Gallery.svelte` has one: a Sprites tab is
  // twenty of these and twenty independent animation loops is twenty wakeups
  // a frame for pictures that change at 10 fps.
  import { onDestroy, onMount } from "svelte";
  import { frameAt, renderFrame, type Sprite } from "../../lib/sprite";

  export let sprite: Sprite | null = null;
  /** The intrinsic square this draws into, in canvas pixels. */
  export let size = 128;
  /** 0 = zoom to fit at an INTEGER factor; anything else is that factor. */
  export let zoom = 0;
  /** `checker` shows transparency; `black` is what a fixture actually does. */
  export let ground: "checker" | "black" = "checker";
  /** Animate at the sprite's own fps. Off = show `frame`. */
  export let playing = true;
  /** Which frame to show when `playing` is false. */
  export let frame = 0;
  export let dataRole = "sprite-thumb";

  let canvas: HTMLCanvasElement | undefined;
  /** The `w`×`h` scratch the texels land in before they are scaled up. */
  let scratch: HTMLCanvasElement | undefined;
  /** Playback clock, in a plain variable: a thumb must not invalidate its
   *  component sixty times a second just to keep time. */
  let elapsed = 0;
  let lastAt = 0;
  let shown = -1;

  // Every dependency NAMED, so a repaint happens when any of them changes —
  // and the draw is a FUNCTION call, because a `$:` that assigned `shown`
  // while reading it would be its own dependency (.claude/rules/web.md).
  $: repaint(sprite, size, zoom, ground, frame, playing);

  function repaint(..._deps: unknown[]): void {
    shown = -1; // force the next paint even if the frame number is unchanged
    paint();
  }

  /** Called by the shared ticker and by the reactive block above. */
  export function paint(): void {
    const c = canvas;
    const s = sprite;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const f = s ? (playing ? frameAt(s, elapsed) : Math.min(frame, s.frames - 1)) : 0;
    if (s && f === shown) return;
    shown = f;

    const z = !s ? 8 : zoom > 0 ? zoom : Math.max(1, Math.floor(Math.min(c.width / s.w, c.height / s.h)));
    ctx.clearRect(0, 0, c.width, c.height);
    // The checker's squares are TEXELS, not a fixed 8 px: a fine checker under
    // big texels reads as noise rather than as "nothing is here".
    drawGround(ctx, c.width, c.height, z);
    if (!s) return;

    if (!scratch) scratch = document.createElement("canvas");
    if (scratch.width !== s.w || scratch.height !== s.h) {
      scratch.width = s.w;
      scratch.height = s.h;
    }
    const sctx = scratch.getContext("2d");
    if (!sctx) return;
    sctx.clearRect(0, 0, s.w, s.h);
    // `createImageData` + `set` rather than `new ImageData(bytes, w, h)`: the
    // constructor's typing demands a `Uint8ClampedArray<ArrayBuffer>` and
    // `renderFrame`'s is the plain (ArrayBufferLike) one.
    const img = sctx.createImageData(s.w, s.h);
    img.data.set(renderFrame(s, f));
    sctx.putImageData(img, 0, 0);

    const dw = s.w * z;
    const dh = s.h * z;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(
      scratch,
      Math.round((c.width - dw) / 2),
      Math.round((c.height - dh) / 2),
      dw,
      dh,
    );
  }

  /** The transparency ground. A checker says "nothing here"; black says "this
   *  is what the fixture will show", which is what the editor's Preview
   *  wants. */
  function drawGround(ctx: CanvasRenderingContext2D, w: number, h: number, cell: number): void {
    if (ground === "black") {
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, w, h);
      return;
    }
    const step = Math.max(4, cell);
    ctx.fillStyle = "#15171c";
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = "#1e222a";
    for (let row = 0; row * step < h; row++) {
      for (let col = 0; col * step < w; col++) {
        if (((col + row) & 1) === 0) continue;
        ctx.fillRect(col * step, row * step, step, step);
      }
    }
  }

  function tick(now: number): void {
    if (!playing || !sprite) {
      lastAt = now;
      return;
    }
    const dt = lastAt === 0 ? 16 : Math.min(200, now - lastAt);
    lastAt = now;
    elapsed += dt;
    paint();
  }

  onMount(() => {
    join(tick);
    paint();
  });

  onDestroy(() => leave(tick));
</script>

<canvas
  bind:this={canvas}
  width={size}
  height={size}
  data-role={dataRole}
  data-frames={sprite?.frames ?? 0}
></canvas>

<style>
  canvas {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    image-rendering: pixelated;
  }
</style>

<script context="module" lang="ts">
  /** The ONE ticker every mounted thumb shares. */
  type Tick = (now: number) => void;
  const live = new Set<Tick>();
  let raf = 0;

  function join(t: Tick): void {
    live.add(t);
    if (raf === 0 && typeof requestAnimationFrame === "function") {
      raf = requestAnimationFrame(step);
    }
  }

  function leave(t: Tick): void {
    live.delete(t);
    if (live.size === 0 && raf !== 0) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  function step(now: number): void {
    raf = live.size > 0 ? requestAnimationFrame(step) : 0;
    for (const t of live) t(now);
  }
</script>
