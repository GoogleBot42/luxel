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
    layout,
    layoutKey,
    layoutSignature,
    THUMB_MAX_CELLS,
    tileShape,
    type Layout,
    type PatternDims,
    type ProjectionMode,
    type TileShape,
  } from "../stores/geometry";

  export let luxel: Luxel;
  /**
   * The pattern's source, in three states (Gitea #731):
   *  - a string — compile it and animate it;
   *  - `null` — there is NOTHING to preview (no pattern chosen, an id that
   *    resolves to nothing). Black, immediately, and never a spinner;
   *  - `undefined` — UNKNOWN: a fetch may genuinely be in flight, because a
   *    device row's source streams in after its name (`stores/device.ts`).
   *    That is the only state a spinner means, and it is bounded by
   *    `PENDING_MS` — an owner that passes `undefined` for "nothing" (rather
   *    than `null`) therefore gets a black thumbnail, not one that spins for
   *    ever, which is exactly what #731 was.
   */
  export let source: string | null | undefined;
  /** Per-item projection override (a playlist row's, §5.4d) — null = the
   *  device default. The thumbnail has to render through it or it shows a
   *  different picture from the one the device will play. */
  export let proj: ProjectionMode | null = null;
  /** `row` is the thumbnail a playlist row or a list entry carries; `summary`
   *  is the larger one the Settings page's LED layout headline sits beside
   *  (Gitea #469). Same engine, same shape — only the CSS size differs. */
  export let size: "row" | "summary" = "row";
  /** Render on a Layout that is NOT the app's — the Settings page's preview
   *  of the lattice you are about to install (Gitea #538). Null (the usual
   *  case) follows `stores/geometry.ts` like everything else. */
  export let previewRig: Layout | null = null;

  const FPS_MS = 100; // ~10 fps is plenty for a thumbnail

  /** How long an UNKNOWN source may keep the spinner up before the thumbnail
   *  gives up and goes black. A spinner that outlives the fetch it stands for
   *  is a lie, and an unresolvable id used to spin for ever (#731). The
   *  give-up is not final: a source that arrives later still lights the
   *  thumbnail up. */
  const PENDING_MS = 2500;

  let canvas: HTMLCanvasElement | undefined;
  let engine: Engine | undefined;
  let rig: Layout | undefined;
  let dims: PatternDims = 0;
  let dead = false;
  /** Why it is dead — the compiler's own message, so a layer whose pattern
   *  does not compile SAYS so instead of just drawing nothing (#731). */
  let error = "";
  let ready = false;
  /** The source the current engine was built from — `null` is "no engine",
   *  which an empty-string source is not. */
  let built: string | null = null;
  let builtRig = ""; // the Layout it was built for
  let builtProj: ProjectionMode | null = null; // the override it was built with
  let raf = 0;
  let last = 0;
  let angle = 0;

  /** The give-up timer for an UNKNOWN source (above). `armedFor` keeps a
   *  parent that re-renders with the SAME source from restarting it, which
   *  would make the bound unreachable. */
  let pendingTimer = 0;
  let armed = false;
  let armedFor: string | null | undefined;
  let gaveUp = false;

  /** Everything derived from the rig is assigned WITH it, in `build()` — a
   *  `$:` here would be ordered before the reactive block that calls `build`
   *  and could render one frame behind the Layout it drew. */
  let shape: TileShape = "bar";
  let points: PointRig = { pts: [], is3D: false };
  /** `1D · by index` when the pattern is not native to this Layout. */
  let caption: string | null = null;
  /** Rebuild when the Layout actually moves, not on every store tick. An
   *  explicit `previewRig` is its own signature — the store's would never change. */
  $: rigKey = previewRig ? layoutKey(previewRig) : $layoutSignature;

  function adopt(l: Layout | undefined, d: PatternDims): void {
    rig = l;
    dims = d;
    // a Layout whose coordinates we don't have has nothing to scatter
    shape = l === undefined ? "bar" : l.coords === undefined && !l.regular ? "bar" : tileShape(l);
    points = l?.coords ? normalizePoints(l.coords) : { pts: [], is3D: false };
    caption = l ? captionFor(d, l) : null;
  }

  /** The Layout a thumbnail with nothing to draw still takes its SHAPE from:
   *  an empty or dead preview must be the same box as a live one, or a row
   *  jumps the moment a pattern is chosen. */
  function idleRig(): Layout {
    return previewRig ?? $layout;
  }

  function build(src: string): void {
    engine?.free();
    engine = undefined;
    dead = false;
    error = "";
    ready = false;
    restartGiveUp(); // a rebuild earns a fresh spinner, and a fresh bound
    const r = compileForLayout(luxel, src, THUMB_MAX_CELLS, proj, previewRig);
    if ("engine" in r) {
      r.engine.setWallClock(Date.now() / 1000);
      engine = r.engine;
      adopt(r.layout, r.dims);
    } else {
      dead = true; // won't compile — show a muted placeholder and SAY why
      error = r.line > 0 ? `line ${r.line}: ${r.message}` : r.message;
      adopt(idleRig(), 0);
      clearCanvas();
    }
  }

  /** Nothing to draw any more: drop the engine and leave the canvas black —
   *  "black is probably more accurate" (#731). A thumbnail that kept the last
   *  pattern's picture would be claiming a layer still has one. */
  function release(rk: string): void {
    engine?.free();
    engine = undefined;
    built = null;
    builtRig = rk;
    builtProj = proj;
    dead = false;
    error = "";
    ready = false;
    adopt(idleRig(), 0);
    clearCanvas();
  }

  function clearCanvas(): void {
    if (!canvas) return;
    // the canvas's CSS background is black, so clearing IS the black frame
    canvas.getContext("2d")?.clearRect(0, 0, canvas.width, canvas.height);
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

  // Rebuild whenever the source arrives or changes, the Layout moves, or the
  // per-item projection override changes — and RELEASE when there is no
  // source any more. Everything the engine decides is assigned inside these
  // functions, so the markup (not a `$:`) reads them: a `$:` derived from a
  // value assigned in a function a reactive block calls never re-runs
  // (.claude/rules/web.md).
  $: syncEngine(source, rigKey, proj, luxel);

  function syncEngine(
    src: string | null | undefined,
    rk: string,
    pj: ProjectionMode | null,
    lx: Luxel | undefined,
  ): void {
    if (!lx) return;
    if (typeof src !== "string") {
      // `null` and `undefined` both mean "no picture": the difference is only
      // whether a spinner is honest, which `thumbState` decides.
      if (built !== null || builtRig !== rk) release(rk);
      return;
    }
    if (src === built && rk === builtRig && pj === builtProj) return;
    built = src;
    builtRig = rk;
    builtProj = pj;
    build(src);
  }

  /** Start (once per distinct source) the bound on how long UNKNOWN may show
   *  a spinner. */
  $: armGiveUp(source);

  function armGiveUp(src: string | null | undefined): void {
    if (armed && src === armedFor) return;
    armed = true;
    armedFor = src;
    restartGiveUp();
  }

  function restartGiveUp(): void {
    window.clearTimeout(pendingTimer);
    gaveUp = false;
    pendingTimer = window.setTimeout(() => {
      pendingTimer = 0;
      gaveUp = true;
    }, PENDING_MS);
  }

  /**
   * THE spinner policy (#731), as one function the markup calls with every
   * dependency named — so it is re-evaluated on each patch instead of
   * freezing at its first value:
   *   `dead`    — it does not compile; the badge says why;
   *   `empty`   — nothing to preview (no pattern, or UNKNOWN for too long);
   *   `loading` — a fetch is genuinely in flight, and not for ever;
   *   `live`    — a frame has been drawn.
   */
  function thumbState(
    src: string | null | undefined,
    rdy: boolean,
    dd: boolean,
    gave: boolean,
  ): "dead" | "empty" | "loading" | "live" {
    if (dd) return "dead";
    if (src === null) return "empty";
    if (rdy && typeof src === "string") return "live";
    return gave ? "empty" : "loading";
  }

  function thumbTitle(
    src: string | null | undefined,
    cap: string | null,
    err: string,
    rdy: boolean,
    dd: boolean,
    gave: boolean,
  ): string | undefined {
    if (dd) return `does not compile — ${err}`;
    if (thumbState(src, rdy, dd, gave) === "empty") return "no pattern";
    return cap ?? undefined;
  }

  onMount(() => {
    raf = requestAnimationFrame(loop);
  });
  onDestroy(() => {
    cancelAnimationFrame(raf);
    window.clearTimeout(pendingTimer);
    engine?.free();
  });
</script>

<span
  class="thumb"
  class:dead
  class:summary={size === "summary"}
  data-shape={shape}
  data-state={thumbState(source, ready, dead, gaveUp)}
  title={thumbTitle(source, caption, error, ready, dead, gaveUp)}
>
  {#if shape === "bar"}
    <canvas class="bar" bind:this={canvas} width="64" height="1"></canvas>
  {:else}
    <canvas class="sq" bind:this={canvas} width="48" height="48"></canvas>
  {/if}
  {#if thumbState(source, ready, dead, gaveUp) === "loading"}
    <span class="spinner overlay" data-role="thumb-spinner" aria-label="loading preview"></span>
  {/if}
  {#if dead}
    <!-- a compile failure the user can SEE, not just an empty box (#731) -->
    <span class="errmark" data-role="thumb-error" aria-label={`does not compile — ${error}`}>!</span>
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

  /* the Settings LED-layout summary (#469; mockups S3/S3b `.summary canvas`) */
  .thumb.summary canvas {
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .thumb.summary .bar {
    width: 120px;
    height: 20px;
  }

  .thumb.summary .sq {
    width: 56px;
    height: 56px;
  }

  .thumb.dead canvas {
    opacity: 0.3;
  }

  /* the dead badge: small enough for a 26px picker row, legible on a 44px
     inspector thumbnail — the title carries the compiler's message */
  .errmark {
    position: absolute;
    right: -2px;
    bottom: -2px;
    display: block;
    min-width: 11px;
    height: 11px;
    border-radius: 6px;
    background: var(--error);
    color: #fff;
    font: 9px/11px var(--mono);
    text-align: center;
  }

  /* `.spinner` / `.spinner.overlay` are the shared ones in app.css (Gitea
   * #738). This file used to carry the sixth verbatim copy of that block,
   * each with its own `@keyframes` name; `.thumb` is already
   * `position: relative`, which is all the overlay variant needs. */
</style>
