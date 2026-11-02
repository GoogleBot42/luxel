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
  import type { PreviewSize } from "../../lib/scenePlace";

  /** The scene's grid. */
  export let w = 64;
  export let h = 64;
  /** The selected layer's box AS THE RECORD HAS IT — design units in a
   *  scene with a `U` line, pixels otherwise — or null when nothing is
   *  selected. A drag starts from these numbers and hands back new ones. */
  export let rect: Rect | null = null;
  /**
   * Where that layer REALLY draws on this canvas, in cells: the page reads it
   * off the compositor (`SceneRenderer.layerRect` — the `U` mapping, the
   * keyframe track, a natural sprite's own size all applied; Gitea #997, and
   * #996 item 1, whose marquee sat at the `L` origin while a keyed sprite was
   * somewhere else). The marquee is drawn HERE, not from `rect`.
   */
  export let box: { x: number; y: number; w: number; h: number } | null = null;
  /** Cells per design unit on this canvas (`lib/scenePlace.ts`
   *  `designScale`; 1 and 1 without a `U` line). A drag measured in cells is
   *  divided by it, so a box moves in whole design units. */
  export let scale: { x: number; y: number } = { x: 1, y: 1 };
  /** `64×64 · 24 fps on device` — the header's dim line, which travels WITH
   *  the canvas rather than with the column (#736 item 25). */
  export let dimsLine = "";
  /** The preview's frame budget, in the pattern editor's own terms: 0 = as
   *  fast as the browser will go (#736 item 26). The page owns the number —
   *  it is the page's render loop — and this column owns the chooser. */
  export let targetFps = 60;
  /** The `Preview at` sizes (`lib/scenePlace.ts` `previewSizes`) and the one
   *  chosen — "" is the device's own canvas. The page re-rigs its PREVIEW
   *  compositor on a change and nothing else: no device write (Gitea #997). */
  export let sizes: PreviewSize[] = [];
  export let previewAt = "";
  /** The one dim line under the picture after a Layout switch (S7j). */
  export let note = "";

  const dispatch = createEventDispatcher<{
    rect: Rect;
    /** A new preview frame budget (#736 item 26). */
    targetfps: number;
    /** A new `Preview at` size id. */
    previewat: string;
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

  /** A one-row canvas — a strip (Gitea #985). The stage is a bar there and
   *  a box is a SEGMENT: whatever `y`/`h` the record holds, it is drawn as
   *  the row, and a drag only ever moves or resizes it along `x`. */
  $: oneRow = h === 1;

  /** On one row the box is the row whatever `y`/`h` say. */
  $: shown = box ? { x: box.x, y: oneRow ? 0 : box.y, w: box.w, h: oneRow ? 1 : box.h } : null;

  $: marq = shown
    ? {
        left: `${(shown.x / w) * 100}%`,
        top: `${(shown.y / h) * 100}%`,
        width: `${(shown.w / w) * 100}%`,
        height: `${(shown.h / h) * 100}%`,
      }
    : null;

  // ---- dragging the marquee ----

  type Corner = "nw" | "ne" | "sw" | "se";
  const CORNERS: Corner[] = ["nw", "ne", "sw", "se"];
  /** A drag in progress: where the pointer went down (in cells), the record's
   *  box then, and its size then in DESIGN units — an unset (0) axis taken
   *  from what is drawn, so a corner drag off a "whole layout" box or a
   *  natural-size sprite starts from the size you can see. */
  type Grab = { mode: "move" | Corner; col: number; row: number; start: Rect; bw: number; bh: number };
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
    if (!shown || !rect) return;
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
    grab = {
      mode,
      col: c.col,
      row: c.row,
      start: { ...rect },
      bw: rect.w !== 0 ? rect.w : Math.max(1, Math.round(shown.w / scale.x)),
      bh: rect.h !== 0 ? rect.h : Math.max(1, Math.round(shown.h / scale.y)),
    };
  }

  /**
   * The pointer's travel since it went down, in DESIGN units: cells divided
   * by the scale and rounded, so under `U 32 32` on a 64×64 panel a box moves
   * one unit per two cells (snapped to design cells), and 1:1 without `U`.
   * Measured as a DELTA because the centring offset of the design area — and
   * a keyframe track, which moves what is drawn away from the `L` origin —
   * never matter to how far a box should move.
   */
  function move(e: PointerEvent): void {
    if (!grab) return;
    const c = cellAt(e);
    const s = grab.start;
    const dx = Math.round((c.col - grab.col) / scale.x);
    const dy = Math.round((c.row - grab.row) / scale.y);
    if (grab.mode === "move") {
      // on a strip the segment slides along the row: `y` lands on it and `h`
      // keeps its word (0 stays 0 — a drag must not invent a height)
      dispatch("rect", { ...s, x: s.x + dx, y: oneRow ? 0 : s.y + dy });
      return;
    }
    // A resize moves one corner and keeps the opposite one put; the box never
    // goes below 1x1, and an explicit size replaces a "whole layout" 0.
    let { x, y } = s;
    let nw = grab.bw;
    let nh = grab.bh;
    if (grab.mode === "nw" || grab.mode === "sw") {
      nw = Math.max(1, grab.bw - dx);
      x = s.x + grab.bw - nw;
    } else {
      nw = Math.max(1, grab.bw + dx);
    }
    if (grab.mode === "nw" || grab.mode === "ne") {
      nh = Math.max(1, grab.bh - dy);
      y = s.y + grab.bh - nh;
    } else {
      nh = Math.max(1, grab.bh + dy);
    }
    if (oneRow) {
      dispatch("rect", { ...s, x, y: 0, w: nw });
      return;
    }
    dispatch("rect", { x, y, w: nw, h: nh });
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
        <!-- `Preview at` (S7j, Gitea #997): the size the preview is RENDERED
             at — how a scene with a design size is checked on a panel you do
             not own. Its own class, appended after the fps picker, so no
             mapped selector of S7/S7f can land on it (.claude/rules/web.md). -->
        {#if sizes.length > 1}
          <select
            class="sel pvat"
            data-role="scene-preview-at"
            value={sizes.some((p) => p.id === previewAt) ? previewAt : ""}
            title="preview at another panel size (the device is not touched)"
            aria-label="preview at"
            on:change={(e) => dispatch("previewat", e.currentTarget.value)}
          >
            {#each sizes as p (p.id)}
              <option value={p.id}>{p.label}</option>
            {/each}
          </select>
        {/if}
      </div>
    </div>

    <div class="stage" bind:this={stage}>
      <canvas
        bind:this={canvas}
        class:bar={oneRow}
        width={w}
        height={h}
        data-role="scene-stage"
        data-shape={oneRow ? "bar" : "grid"}
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
    {#if note !== ""}
      <p class="lythint" data-role="scene-layout-hint">{note}</p>
    {/if}
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
