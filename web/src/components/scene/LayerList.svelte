<script lang="ts">
  // The scene editor's LEFT column (mockup S7 `.lcol`, states in S7d/S7e):
  // what is stacked, in the OBS/Photoshop idiom — drag to reorder, eye to
  // hide, click to select — plus the one primary the column has,
  // `+ Add layer ▾`.
  //
  // TOP = FRONT. The wire lists layers bottom → top (`layers[0]` is the
  // base), so this component renders the reverse and translates indices at
  // its edge; nothing above it ever sees a display index.
  //
  // Visibility lives HERE, on the eye, and never as a checkbox in the
  // inspector (mockups.html :1287-88).
  import { createEventDispatcher } from "svelte";
  import Popover from "../Popover.svelte";
  import { formatClock } from "../../lib/sceneRender";
  import type { Layer, LayerKind, Scene } from "../../lib/scene";

  export let scene: Scene;
  /** Index into `scene.layers` (wire order), or -1 for nothing selected. */
  export let selected = -1;
  /** `caps.layers` — the device's pattern-layer budget. NEVER a constant. */
  export let layerCap = 2;
  /** How many pattern layers the scene already has. */
  export let patternLayers = 0;
  /** Sprite geometry by layer index, for the `sprite · 9×8` metadata line. */
  export let spriteDims: Record<number, string> = {};
  /** The pattern's display name by layer index (only the host knows it). */
  export let patternNames: Record<number, string> = {};

  const dispatch = createEventDispatcher<{
    select: number;
    toggle: number;
    reorder: { from: number; to: number };
    add: LayerKind;
  }>();

  /** Display order: top of the list is the front of the stack. */
  $: rows = scene.layers.map((l, i) => ({ layer: l, at: i })).reverse();

  let addOpen = false;
  let addBtn: HTMLElement | null = null;

  /** The cap's reason, in D4's words — the SAME string goes on `data-reason`
   *  and under the row, because "2 of 2 used" alone does not tell you that
   *  text, sprite and color layers are free (S7d). */
  $: capReason = `this device fits ${layerCap} pattern layer${layerCap === 1 ? "" : "s"}; text and sprite and color layers are free`;
  $: atCap = patternLayers >= layerCap;

  const TYPE_GLYPH: Record<LayerKind, string> = {
    text: "T",
    sprite: "▦",
    pat: "▤",
    color: "▭",
  };

  /** The dim mono line after the name (S7 `.meta2`, per type). */
  function metaOf(l: Layer, at: number, sprites: Record<number, string>): string {
    if (!l.style.visible) return "hidden";
    if (l.body.kind === "text") {
      const s = l.body.text.source;
      if (s.kind === "lit") return s.text;
      if (s.kind === "clock") return formatClock(s.fmt, new Date());
      return `slot ${s.slot}`;
    }
    if (l.body.kind === "sprite") return sprites[at] ?? "sprite";
    if (l.body.kind === "color") return "color";
    const bits: string[] = [];
    if (l.style.opacity !== 100) bits.push(`${l.style.opacity} %`);
    if (l.style.blend !== "normal") bits.push(l.style.blend);
    if (bits.length === 0) return at === 0 ? "base" : "";
    return bits.join(" · ");
  }

  function nameOf(l: Layer, at: number, names: Record<number, string>): string {
    if (l.name !== "") return l.name;
    if (l.body.kind === "pat") return names[at] ?? "no pattern";
    return "Layer";
  }

  // ---- drag to reorder (S7e) ----
  // The lifted row keeps its place and the destination is a 2px accent rule
  // BETWEEN rows, so the list never reflows under the pointer.

  /** Display index of the row being dragged, or -1. */
  let dragRow = -1;
  /** Display slot the drop rule sits above (0 = before the first row). */
  let dropAt = -1;
  let listEl: HTMLElement | null = null;

  function onGrip(e: PointerEvent, row: number): void {
    e.preventDefault();
    e.stopPropagation();
    capture(e);
    dragRow = row;
    dropAt = row;
  }

  /** Pointer capture is best-effort: a SYNTHETIC `pointerdown` (a harness
   *  staging the drag state, `web/tools/mockdiff.map.json` S7e) has no active
   *  pointer, and the throw would take the whole drag down with it. */
  function capture(e: PointerEvent): void {
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* synthetic pointer — the window listeners still drive the drag */
    }
  }

  function onGripMove(e: PointerEvent): void {
    if (dragRow < 0 || !listEl) return;
    const els = [...listEl.querySelectorAll<HTMLElement>("[data-role='scene-layer']")];
    let at = els.length;
    for (let i = 0; i < els.length; i++) {
      const r = (els[i] as HTMLElement).getBoundingClientRect();
      if (e.clientY < r.top + r.height / 2) {
        at = i;
        break;
      }
    }
    dropAt = at;
  }

  function onGripUp(e: PointerEvent): void {
    if (dragRow < 0) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    const from = dragRow;
    const to = dropAt > from ? dropAt - 1 : dropAt;
    dragRow = -1;
    dropAt = -1;
    if (to >= 0 && to !== from) {
      // display indices → wire indices (the list is the reverse of the wire)
      const n = scene.layers.length;
      dispatch("reorder", { from: n - 1 - from, to: n - 1 - to });
    }
  }

  function add(kind: LayerKind): void {
    addOpen = false;
    dispatch("add", kind);
  }
</script>

<div class="lcol" data-role="scene-layers">
  <div class="rhead" style="margin-bottom:8px">
    <div class="slabel">Layers</div>
    <div class="rdim" style="margin-left:auto">top = front</div>
  </div>

  <div bind:this={listEl}>
    {#each rows as r, row (r.at)}
      {#if dragRow >= 0 && dropAt === row}
        <div class="dropline" data-role="scene-drop"></div>
      {/if}
      <div
        class="lrow"
        class:sel={selected === r.at}
        class:off={!r.layer.style.visible}
        class:drag={dragRow === row}
        data-role="scene-layer"
        data-layer={r.at}
      >
        <span
          class="hnd"
          data-role="scene-layer-grip"
          title="drag to reorder"
          on:pointerdown={(e) => onGrip(e, row)}
          on:pointermove={onGripMove}
          on:pointerup={onGripUp}
          on:pointercancel={onGripUp}
          role="presentation">⠿</span
        >
        <button
          class="eye"
          data-role="scene-layer-eye"
          aria-label={r.layer.style.visible ? "hide this layer" : "show this layer"}
          aria-pressed={r.layer.style.visible}
          on:click|stopPropagation={() => dispatch("toggle", r.at)}
        >
          {#if r.layer.style.visible}
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              ><path d="M2 12s3.8-6 10-6 10 6 10 6-3.8 6-10 6S2 12 2 12z" /><circle
                cx="12"
                cy="12"
                r="3"
              /></svg
            >
          {:else}
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              ><path d="M4 4l16 16" /><path
                d="M6.7 6.9C4 8.6 2 12 2 12s3.8 6 10 6c1.9 0 3.5-.5 4.9-1.3"
              /><path d="M9.9 5.2A9.6 9.6 0 0 1 12 6c6.2 0 10 6 10 6a18 18 0 0 1-2.4 3" /></svg
            >
          {/if}
        </button>
        <button
          class="pick"
          data-role="scene-layer-pick"
          on:click={() => dispatch("select", r.at)}
        >
          <span class="ty">{TYPE_GLYPH[r.layer.body.kind]}</span>
          <span class="nm2">{nameOf(r.layer, r.at, patternNames)}</span>
          <span class="meta2">{metaOf(r.layer, r.at, spriteDims)}</span>
        </button>
      </div>
    {/each}
    {#if dragRow >= 0 && dropAt === rows.length}
      <div class="dropline" data-role="scene-drop"></div>
    {/if}
  </div>

  <div class="addwrap">
    <button
      class="btn primary"
      data-role="scene-add-layer"
      bind:this={addBtn}
      aria-haspopup="menu"
      aria-expanded={addOpen}
      on:click|stopPropagation={() => (addOpen = !addOpen)}>+ Add layer ▾</button
    >
    <Popover
      open={addOpen}
      anchor={addBtn}
      align="start"
      dataRole="scene-add-menu"
      on:close={() => (addOpen = false)}
    >
      <div class="full-inner">
        {#if atCap}
          <!-- D4's ONE deliberate exception to "absent, never disabled": the
               layer budget is something the user has to learn, so Pattern
               stays in the menu, greyed, with its count on the row and the
               reason spelled out underneath (S7d). -->
          <button class="mi dis" data-role="scene-add-pat" disabled data-reason={capReason}>
            ▤ Pattern <span class="mdim">{patternLayers} of {layerCap} used</span>
          </button>
          <div class="reason">{capReason}</div>
        {:else}
          <button class="mi" data-role="scene-add-pat" on:click={() => add("pat")}
            >▤ Pattern</button
          >
        {/if}
        <button class="mi" data-role="scene-add-text" on:click={() => add("text")}>T Text</button>
        <button class="mi" data-role="scene-add-sprite" on:click={() => add("sprite")}
          >▦ Sprite</button
        >
        <button class="mi" data-role="scene-add-color" on:click={() => add("color")}
          >▭ Color</button
        >
      </div>
    </Popover>
  </div>
</div>

<style>
  /* The row is three children in the mock (`hnd → eye → ty/nm2/meta2`); the
     last three are one button so clicking the row selects it without the grip
     or the eye being inside a second control. It is transparent and carries
     no metrics of its own — `.lrow`'s flex line is unchanged. */
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
  }

  .pick:hover {
    border-color: transparent;
  }

  /* `.menu.full` spans the column rather than the global 214px (mock
     `.menu.full{left:0;right:0;width:auto}`); the popover is placed `fixed`,
     so the width comes from the button it hangs off. */

  .full-inner {
    display: contents;
  }
</style>
