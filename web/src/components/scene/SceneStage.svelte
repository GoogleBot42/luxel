<script lang="ts">
  // The scene editor's CENTRE column (mockup S7 `.ccol`): the live composite
  // in the device's shape, the selected layer's box as a dashed marquee you
  // can drag and resize, and the frame-cost line with its meter.
  //
  // "The one direct-manipulation affordance in the whole app, and it earns its
  // place" (§5.5): geometry is set by dragging, with the inspector's four
  // numbers as the exact fallback. Both write the same `style.rect`.
  //
  // It does NOT mount `components/Preview.svelte`: that component is the
  // pattern editor's and already appears twice in the DOM (editor + map
  // editor), a third `[data-role="preview"]` would break every unscoped
  // harness selector (.claude/rules/web.md), and a scene stage needs an
  // overlay the preview has no business carrying. The pointer path is the
  // same one — `getBoundingClientRect`, normalized, then multiplied by the
  // canvas's intrinsic size, which IS the cell grid.
  import { createEventDispatcher, onMount } from "svelte";
  import { paintGrid } from "../../lib/draw";
  import type { Rect } from "../../lib/scene";

  /** The scene's grid. */
  export let w = 64;
  export let h = 64;
  /** The selected layer's box, or null when nothing is selected. */
  export let rect: Rect | null = null;
  /** `Frame cost: …` and the meter under it. */
  export let patternLayers = 0;
  export let layerCap = 2;
  export let fps = 0;
  /** `64×64 · 24 fps on device` — the column header's dim line. */
  export let dimsLine = "";
  export let paused = false;

  const dispatch = createEventDispatcher<{
    rect: Rect;
    pause: boolean;
    /** A pointer landed on a cell — the sprite tools' hook (#481). */
    cell: { col: number; row: number; down: boolean };
  }>();

  let canvas: HTMLCanvasElement | undefined;
  let stage: HTMLElement | undefined;

  /** Draw one composite frame. Called by the page's ticker. */
  export function draw(px: Uint8Array): void {
    if (canvas) paintGrid(canvas, px, w, h);
  }

  export function clear(): void {
    const ctx = canvas?.getContext("2d");
    if (canvas && ctx) {
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
    }
  }

  /** The box in CSS percentages of the stage — `w`/`h` = 0 is "the whole
   *  layout on that axis" (docs/spec/scenes.md §2). */
  $: box = rect
    ? {
        x: rect.x,
        y: rect.y,
        w: rect.w === 0 ? w : rect.w,
        h: rect.h === 0 ? h : rect.h,
      }
    : null;

  $: marq = box
    ? {
        left: `${(box.x / w) * 100}%`,
        top: `${(box.y / h) * 100}%`,
        width: `${(box.w / w) * 100}%`,
        height: `${(box.h / h) * 100}%`,
      }
    : null;

  /** The cost line. At the cap it counts both numbers and adds the measured
   *  frame rate (S7d/S7e); under it, it says what is free (S7). */
  $: costLine =
    patternLayers >= layerCap
      ? `Frame cost: ${patternLayers} of ${layerCap} pattern layers · ${Math.round(fps)} fps`
      : `Frame cost: ${patternLayers} pattern layer${patternLayers === 1 ? "" : "s"} · text, sprite and color layers are free`;

  $: fill = Math.max(0, Math.min(100, layerCap > 0 ? (patternLayers / layerCap) * 100 : 0));

  // ---- dragging the marquee ----

  type Corner = "nw" | "ne" | "sw" | "se";
  const CORNERS: Corner[] = ["nw", "ne", "sw", "se"];
  type Grab = { mode: "move" | Corner; ox: number; oy: number; start: Rect };
  let grab: Grab | null = null;

  /** Pointer → cell, pixel-snapped. The canvas's intrinsic size IS the grid,
   *  so this is one multiply (`Preview.svelte`'s note). */
  function cellAt(e: PointerEvent): { col: number; row: number } {
    const r = (canvas as HTMLCanvasElement).getBoundingClientRect();
    const col = Math.floor(((e.clientX - r.left) / r.width) * w);
    const row = Math.floor(((e.clientY - r.top) / r.height) * h);
    return { col: Math.max(0, Math.min(w - 1, col)), row: Math.max(0, Math.min(h - 1, row)) };
  }

  function start(e: PointerEvent, mode: Grab["mode"]): void {
    if (!box || !rect) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const c = cellAt(e);
    grab = { mode, ox: c.col - box.x, oy: c.row - box.y, start: { ...rect } };
  }

  function move(e: PointerEvent): void {
    if (!grab || !box) return;
    const c = cellAt(e);
    const s = grab.start;
    const bw = s.w === 0 ? w : s.w;
    const bh = s.h === 0 ? h : s.h;
    if (grab.mode === "move") {
      dispatch("rect", { ...s, x: c.col - grab.ox, y: c.row - grab.oy });
      return;
    }
    // A resize moves one corner and keeps the opposite one put; the box never
    // goes below 1x1, and an explicit size replaces a "whole layout" 0.
    let { x, y } = s;
    let nw = bw;
    let nh = bh;
    if (grab.mode === "nw" || grab.mode === "sw") {
      nw = x + bw - c.col;
      x = c.col;
    } else {
      nw = c.col - x + 1;
    }
    if (grab.mode === "nw" || grab.mode === "ne") {
      nh = y + bh - c.row;
      y = c.row;
    } else {
      nh = c.row - y + 1;
    }
    dispatch("rect", { x, y, w: Math.max(1, nw), h: Math.max(1, nh) });
  }

  function end(e: PointerEvent): void {
    if (!grab) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    grab = null;
  }

  function onCanvasDown(e: PointerEvent): void {
    const c = cellAt(e);
    dispatch("cell", { ...c, down: true });
  }

  function onCanvasMove(e: PointerEvent): void {
    if (e.buttons === 0) return;
    const c = cellAt(e);
    dispatch("cell", { ...c, down: false });
  }

  onMount(() => clear());
</script>

<div class="ccol" data-role="scene-preview">
  <div class="rhead">
    <div class="slabel">Preview</div>
    <div class="rdim" data-role="scene-preview-dims">{dimsLine}</div>
    <div class="grp">
      <button
        class="btn sm icon"
        data-role="scene-pause"
        aria-pressed={paused}
        title={paused ? "resume the preview" : "pause the preview"}
        on:click={() => dispatch("pause", !paused)}>{paused ? "▶" : "‖"}</button
      >
    </div>
  </div>

  <div class="stage" bind:this={stage}>
    <canvas
      bind:this={canvas}
      width={w}
      height={h}
      data-role="scene-stage"
      on:pointerdown={onCanvasDown}
      on:pointermove={onCanvasMove}
    ></canvas>
    {#if marq}
      <div
        class="marq"
        data-role="scene-marquee"
        style={`left:${marq.left};top:${marq.top};width:${marq.width};height:${marq.height}`}
        on:pointerdown={(e) => start(e, "move")}
        on:pointermove={move}
        on:pointerup={end}
        on:pointercancel={end}
        role="presentation"
      ></div>
      {#each CORNERS as corner (corner)}
        <button
          class="hdl"
          data-role={`scene-handle-${corner}`}
          aria-label={`resize ${corner}`}
          style={handleStyle(corner, marq)}
          on:pointerdown={(e) => start(e, corner)}
          on:pointermove={move}
          on:pointerup={end}
          on:pointercancel={end}
        ></button>
      {/each}
    {/if}
  </div>

  <div class="hint" style="margin-top:12px" data-role="scene-cost">{costLine}</div>
  <div class="budget"><i style={`width:${fill}%`}></i></div>
</div>

<script context="module" lang="ts">
  /** The four 7px corner grabs, centred on the marquee's corners (mock
   *  `.hdl` at -3px / +456px against a 460px stage). */
  function handleStyle(
    corner: string,
    m: { left: string; top: string; width: string; height: string },
  ): string {
    const x = corner === "nw" || corner === "sw" ? `calc(${m.left} - 3px)` : `calc(${m.left} + ${m.width} - 4px)`;
    const y = corner === "nw" || corner === "ne" ? `calc(${m.top} - 3px)` : `calc(${m.top} + ${m.height} - 4px)`;
    return `left:${x};top:${y}`;
  }
</script>
