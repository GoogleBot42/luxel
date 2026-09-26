<script lang="ts">
  // The scene editor's CENTRE column (mockup S7 `.ccol`): the live composite
  // in the device's shape, and the selected layer's box as a dashed marquee
  // you can drag and resize.
  //
  // It no longer draws the frame-cost line or the budget meter under the
  // canvas (Gitea #736 item 24). Jeremy: "that loading bar below the pattern
  // preview needs to completely go away. That and this text 'Frame cost: 1
  // pattern layer · text, sprite and color layers are free'". They were a
  // budget nobody was near, stated on every scene, under the one thing on
  // the screen you are actually looking at — and the meter read as progress.
  // The sentence they were carrying has one honest home, which is the Add
  // layer menu AT the cap, where it explains a row that will not click
  // (`LayerList.svelte`, item 18). Mockups S7/S7c/S7d/S7e still draw both:
  // the mockups are wrong now, not the app.
  //
  // "The one direct-manipulation affordance in the whole app, and it earns its
  // place" (§5.5): geometry is set by dragging, with the inspector's four
  // numbers as the exact fallback. Both write the same `style.rect`.
  //
  // It NO LONGER paints (Gitea #741). A selected sprite layer used to mount
  // `SpriteTools` into a slot here and take the canvas's pointer events over
  // for drawing, which made the marquee unreachable — Jeremy: "I expect to be
  // able to move around the sprite via moving like the other layers but that
  // doesn't work. Just that alone is asking for trouble." Drawing moved to its
  // own screen (`pages/SpriteEditor.svelte`), so EVERY layer kind moves and
  // resizes here the same way and the canvas has no pointer handlers at all.
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
  /**
   * What an UNSET axis of the box really covers, when the layer knows.
   *
   * `w`/`h` = 0 means "the whole layout on that axis" for a pattern or a text
   * layer — but for a SPRITE it means the sprite's own size at (x, y)
   * (Gitea #740's fit rule). Drawing the marquee across the whole grid for a
   * 12×12 sprite makes the one direct-manipulation affordance in the app read
   * as "you are dragging the canvas", which is the opposite of what #741 asked
   * for. The page passes the sprite's dimensions here and the marquee wraps
   * what is actually drawn.
   */
  export let natural: { w: number; h: number } | null = null;
  /** `64×64 · 24 fps on device` — the header's dim line, which travels WITH
   *  the canvas rather than with the column (#736 item 25). */
  export let dimsLine = "";
  /** The preview's frame budget, in the pattern editor's own terms: 0 = as
   *  fast as the browser will go (#736 item 26). The page owns the number —
   *  it is the page's render loop — and this column owns the chooser. */
  export let targetFps = 60;

  const dispatch = createEventDispatcher<{
    rect: Rect;
    /** A new preview frame budget (#736 item 26). */
    targetfps: number;
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
        w: rect.w === 0 ? (natural?.w ?? w) : rect.w,
        h: rect.h === 0 ? (natural?.h ?? h) : rect.h,
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
    // best-effort: a synthetic pointerdown (a harness staging the state) has
    // no active pointer to capture, and the throw would kill the drag
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    const c = cellAt(e);
    grab = { mode, ox: c.col - box.x, oy: c.row - box.y, start: { ...rect } };
  }

  function move(e: PointerEvent): void {
    if (!grab || !box) return;
    const c = cellAt(e);
    const s = grab.start;
    // The same "what does 0 cover" rule the marquee is drawn with, so a
    // corner drag off a natural-size sprite starts from the size you can see.
    const bw = s.w === 0 ? (natural?.w ?? w) : s.w;
    const bh = s.h === 0 ? (natural?.h ?? h) : s.h;
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
    // best-effort, and the mirror of the `setPointerCapture` in `start`: a
    // synthetic drag (a harness moving a layer) never captured a real pointer,
    // and `releasePointerCapture` THROWS `NotFoundError` for an id it does not
    // hold — the optional-call `?.` never guarded that, because the method
    // exists; it is the pointer that does not.
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    grab = null;
  }

  onMount(() => clear());
</script>

<!-- The column is a centring frame and nothing else (#736 item 25). Jeremy:
     "the generated scene preview should be centered vertically and
     horizontally in the space available. Keep the 'Preview 64×64 · 60 fps'
     text in same place right next to the generated scene preview." So the
     header, the tool row and the canvas are ONE stack of the stage's width,
     centred in the column as a unit — the dim line stays directly above the
     canvas's left edge instead of drifting off to the column's. -->
<div class="ccol" data-role="scene-preview">
  <div class="cstack">
    <div class="rhead">
      <div class="slabel">Preview</div>
      <div class="rdim" data-role="scene-preview-dims">{dimsLine}</div>
      <div class="grp">
        <!-- The preview's frame budget, the pattern editor's control and the
             pattern editor's option set exactly (#736 item 26, `target-fps`
             in pages/Editor.svelte). It lives in the PREVIEW section's own
             header there too — the transport is the one that moved to the top
             bar, and this one stayed with the picture it throttles. -->
        <select
          class="sel"
          data-role="scene-target-fps"
          value={targetFps}
          title="preview frame rate"
          aria-label="preview frame rate"
          on:change={(e) => dispatch("targetfps", Number(e.currentTarget.value))}
        >
          <option value={0}>max fps</option>
          <option value={60}>60 fps</option>
          <option value={30}>30 fps</option>
          <option value={15}>15 fps</option>
          <option value={5}>5 fps</option>
        </select>
      </div>
    </div>

    <div class="stage" bind:this={stage}>
      <canvas bind:this={canvas} width={w} height={h} data-role="scene-stage"></canvas>
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
  </div>
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
