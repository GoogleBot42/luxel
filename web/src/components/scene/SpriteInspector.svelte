<script lang="ts">
  // The SPRITE layer's inspector — mockup S7c.
  //
  // First cut (#480): the fields that are the SCENE's — name, the size and
  // frame count read off the sprite tag, the box whose w/h mirror the sprite,
  // the fixed black key stated as a line rather than offered as a select, and
  // the blend tail. WEB-C (#481) adds the ≤16-colour palette, the frame strip
  // and the tool row above the preview; this file is where those go.
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import StyleTail from "./StyleTail.svelte";
  import { MAX_LAYER_NAME, truncateUtf8, type SpriteTag } from "../../lib/scene";
  import type { Layer } from "../../lib/scene";

  export let layer: Layer;
  /** The sprite tag parsed off its pattern source, when the browser has it. */
  export let tag: SpriteTag | null = null;

  const dispatch = createEventDispatcher<{ change: Layer; pick: void }>();

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
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
</script>

<div class="rhead" style="margin-bottom:12px"><div class="slabel">Sprite layer</div></div>

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

<div class="irow">
  <div class="ilab">Sprite</div>
  <div class="patrow">
    <span class="hint" data-role="scene-sprite-size"
      >{tag ? `${tag.w}×${tag.h}` : "none chosen"}</span
    >
    <button class="btn sm" data-role="scene-sprite-change" on:click={() => dispatch("pick")}
      >Change…</button
    >
  </div>
</div>

<div class="irow">
  <div class="ilab">Frames</div>
  <div class="patrow">
    <span class="hint" data-role="scene-sprite-frames">{tag?.frames ?? 1}</span>
    <span class="hint">a frame strip appears at 2+</span>
  </div>
</div>

<div class="irule"></div>

<!-- w/h mirror the sprite: greyed, because a sprite is never scaled (S7c) -->
<BoxRow
  rect={layer.style.rect}
  sizeReadonly
  on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })}
/>

<div class="irow">
  <div class="ilab">Fit</div>
  <div class="hint" data-role="scene-sprite-fit">1:1 · sprites are never scaled</div>
</div>

<div class="irule"></div>

<StyleTail
  style={layer.style}
  keyFixed="black pixels · fixed"
  on:change={(e) => patchLayer({ style: e.detail })}
  on:delete
/>

<style>
  .patrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }
</style>
