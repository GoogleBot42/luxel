<script lang="ts">
  // The SPRITE layer's inspector — mockup S7c, row for row: Name · Size ·
  // Frames · Palette · Transparent · Box · Fit · Blend · Opacity · Delete.
  //
  // Half of it is the SCENE's (the box, the blend tail) and half is the
  // SPRITE's (size, frames, the palette read off its texels). The second half
  // writes the sprite's PATTERN back to the store it came from, which is why
  // those rows dispatch `resize` rather than `change`: the editor owns the
  // save, this file only says what the user asked for (#481/#700).
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import StyleTail from "./StyleTail.svelte";
  import RichSelect from "../RichSelect.svelte";
  import { FIT_OPTIONS, KEY_OPTIONS, fitValue, labelOf } from "../../lib/blendMeta";
  import { MAX_LAYER_NAME, truncateUtf8, type Fit, type SpriteTag } from "../../lib/scene";
  import type { Layer } from "../../lib/scene";
  import {
    paletteCss,
    SPRITE_MAX_COLORS,
    SPRITE_MAX_EDGE,
    spritePalette,
    type Sprite,
  } from "../../lib/sprite";

  export let layer: Layer;
  /** The sprite tag parsed off its pattern source, when the browser has it. */
  export let tag: SpriteTag | null = null;
  /** Its texels, when the source is one this editor can draw on (#481). */
  export let sprite: Sprite | null = null;
  /** True while the rewritten pattern is on its way to the store. */
  export let saving = false;

  const dispatch = createEventDispatcher<{
    change: Layer;
    pick: void;
    fresh: void;
    resize: SpriteTag;
  }>();

  /** The 16 cells S7c draws: the colours in use, then empties to the cap. */
  $: palette = sprite ? spritePalette(sprite) : [];
  $: cells = Array.from({ length: SPRITE_MAX_COLORS }, (_, i) => palette[i] ?? null);

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
  }

  /** Fit is the ONE place `style.fit` means anything — `compose::blit_sprite`
   *  reads it and nothing else does (see `lib/blendMeta.ts`). The narrowing
   *  lives here rather than in the markup: svelte-check does not parse a TS
   *  assertion inside a template expression. */
  function setFit(v: string): void {
    patchLayer({ style: { ...layer.style, fit: v as Fit } });
  }

  /** The device refuses a layer name over 32 BYTES, and a refusal the console
   *  could have prevented is the console's bug — clamp on the way in and put
   *  the clamped value back in the field so what you see is what is stored
   *  (`MAX_LAYER_NAME`, docs/spec/scenes.md §1). */
  function setName(el: HTMLInputElement): void {
    const name = truncateUtf8(el.value, MAX_LAYER_NAME);
    if (name !== el.value) el.value = name;
    patchLayer({ name });
  }

  /** Size and Frames rewrite the SPRITE. Out-of-range is clamped in the field
   *  rather than refused, for the same reason the name is. */
  function resize(field: "w" | "h" | "frames", el: HTMLInputElement): void {
    if (!tag) return;
    const hi = field === "frames" ? 64 : SPRITE_MAX_EDGE;
    const v = Math.max(1, Math.min(hi, Math.round(Number(el.value))));
    if (!Number.isFinite(v)) {
      el.value = String(tag[field]);
      return;
    }
    if (String(v) !== el.value) el.value = String(v);
    if (v === tag[field]) return;
    dispatch("resize", { ...tag, [field]: v });
  }
</script>

<div class="rhead" style="margin-bottom:12px">
  <div class="slabel">Sprite layer</div>
  {#if saving}<div class="rdim" style="margin-left:auto" data-role="scene-sprite-saving">saving…</div>{/if}
</div>

<div class="irow">
  <div class="ilab">Name</div>
  <input
    class="inp"
    style="width:100%"
    data-role="scene-layer-name"
    value={layer.name}
    on:change={(e) => setName(e.currentTarget)}
  />
</div>

<!-- The EMPTY state only. S7c draws a layer that already has a sprite and
     has no row for picking one — and there is no `New sprite…` entry
     anywhere in S1 or S7 either — so this is where a layer bound to nothing
     gets one, and it disappears the moment it has. #700 asks for the picker
     to offer sprites only, which the editor does by filtering the list. -->
{#if !tag}
  <div class="irow">
    <div class="ilab">Sprite</div>
    <div class="patrow">
      <span class="hint" data-role="scene-sprite-state">none chosen</span>
      <button class="btn sm" data-role="scene-sprite-new" on:click={() => dispatch("fresh")}
        >New…</button
      >
      <button class="btn sm" data-role="scene-sprite-change" on:click={() => dispatch("pick")}
        >Change…</button
      >
    </div>
  </div>
{/if}

<div class="irow">
  <div class="ilab">Size</div>
  <div class="boxrow" data-role="scene-sprite-size">
    <label
      >w <input
        class="inp"
        type="number"
        min="1"
        max={SPRITE_MAX_EDGE}
        data-role="scene-sprite-w"
        value={tag?.w ?? 0}
        on:change={(e) => resize("w", e.currentTarget)}
      /></label
    >
    <label
      >h <input
        class="inp"
        type="number"
        min="1"
        max={SPRITE_MAX_EDGE}
        data-role="scene-sprite-h"
        value={tag?.h ?? 0}
        on:change={(e) => resize("h", e.currentTarget)}
      /></label
    >
    <span class="un">px</span>
  </div>
</div>

<div class="irow start">
  <div class="ilab" style="padding-top:5px">Frames</div>
  <div>
    <input
      class="inp num xs"
      style="width:58px"
      type="number"
      min="1"
      data-role="scene-sprite-frames"
      value={tag?.frames ?? 1}
      on:change={(e) => resize("frames", e.currentTarget)}
    />
    <div class="hint" style="margin-top:6px" data-role="scene-sprite-frames-hint">
      animation frames — a frame strip appears at 2+
    </div>
  </div>
</div>

<div class="irow start">
  <div class="ilab" style="padding-top:2px">Palette</div>
  <div>
    <div class="pal" data-role="scene-sprite-palette" data-used={palette.length}>
      {#each cells as c, i (i)}
        {#if c}
          <i style={`background:${paletteCss(c)}`}></i>
        {:else}
          <i class="empty"></i>
        {/if}
      {/each}
    </div>
    <div class="hint" style="margin-top:6px" data-role="scene-sprite-palette-hint">
      {SPRITE_MAX_COLORS} colours
    </div>
  </div>
</div>

<div class="irow">
  <div class="ilab">Transparent</div>
  <div class="hint" data-role="scene-sprite-key">
    {labelOf(KEY_OPTIONS, "black")} · sprites are always keyed
  </div>
</div>

<div class="irule"></div>

<!-- w/h mirror the sprite: greyed, because a sprite is never scaled (S7c) -->
<BoxRow
  rect={layer.style.rect}
  sizeReadonly
  on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })}
/>

<!-- The sprite layer is the ONLY place `fit` does anything (`blit_sprite`:
     `let tile = style.fit == Fit::Tile`), so this is where the chooser lives
     — and it offers the two behaviours that exist rather than the three wire
     words, because `contain` is a synonym of `fill` on every code path
     (lib/blendMeta.ts). It used to be a dead line saying "1:1 · sprites are
     never scaled", which was true of the size and silent about tiling. -->
<div class="irow">
  <div class="ilab">Fit</div>
  <RichSelect
    value={fitValue(layer.style.fit)}
    options={FIT_OPTIONS}
    dataRole="scene-sprite-fit"
    menuRole="scene-fit-menu"
    ariaLabel="how the sprite fills its box"
    on:input={(e) => setFit(e.detail)}
  />
</div>

<div class="irule"></div>

<StyleTail style={layer.style} on:change={(e) => patchLayer({ style: e.detail })} on:delete />

<style>
  .patrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* the mock's inline `.un` on the Size row */
  .un {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
  }

  /* the spinners would not fit a 46px field (BoxRow keeps the same rule) */
  input[type="number"] {
    appearance: textfield;
    -moz-appearance: textfield;
  }

  input[type="number"]::-webkit-outer-spin-button,
  input[type="number"]::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
  }
</style>
