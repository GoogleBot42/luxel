<script lang="ts">
  // The tail every layer inspector ends with: Blend, the transparency key
  // where it applies, Opacity, and `Delete layer` — mockups S7 (text),
  // S7b/S7g (pattern), S7e (color), S7c (sprite).
  //
  // The rule the mocks encode (§5.5, S7b's note): *Transparent* is a KEY on
  // which of the layer's pixels count, and it only means anything under
  // Normal blending — Add, Lighten, Multiply and Mask carry their own (adding
  // black or multiplying by white changes nothing). So under any other blend
  // the row is GONE, not greyed (S7g). A colour layer has no pixels to key
  // and never shows it; a sprite's key is fixed and is a LINE, not a select.
  //
  // Both choosers are `RichSelect`, not `<select>` (Gitea #735): "Most users
  // won't know what those mean. So there should be descriptions and probably
  // even little svg graphics for each option" (Jeremy, 2026-09-24). The words
  // and the drawings live in `lib/blendMeta.ts`; the hint line that used to
  // recite the three key names under the select is gone with them, because
  // the menu now says all three in full.
  import { createEventDispatcher } from "svelte";
  import RichSelect from "../RichSelect.svelte";
  import { BLEND_OPTIONS, KEY_OPTIONS } from "../../lib/blendMeta";
  import type { Blend, LayerKey, LayerStyle } from "../../lib/scene";

  export let style: LayerStyle;
  /** Pattern and text layers key their pixels; colour and sprite do not. */
  export let keyable = false;
  /** A sprite's fixed key, stated rather than offered (S7c). */
  export let keyFixed = "";

  const dispatch = createEventDispatcher<{ change: LayerStyle; delete: void }>();

  // The narrowing lives in the script, not in the markup: a TS assertion
  // inside a template expression is not something svelte-check parses.
  function setBlend(v: string): void {
    dispatch("change", { ...style, blend: v as Blend });
  }

  function setKey(v: string): void {
    dispatch("change", { ...style, key: v as LayerKey });
  }
</script>

<div class="irow">
  <div class="ilab">Blend</div>
  <RichSelect
    value={style.blend}
    options={BLEND_OPTIONS}
    dataRole="scene-blend"
    menuRole="scene-blend-menu"
    ariaLabel="blend mode"
    on:input={(e) => setBlend(e.detail)}
  />
</div>

{#if keyable && style.blend === "normal"}
  <div class="irow">
    <div class="ilab">Transparent</div>
    <RichSelect
      value={style.key}
      options={KEY_OPTIONS}
      dataRole="scene-key"
      menuRole="scene-key-menu"
      ariaLabel="which pixels count"
      on:input={(e) => setKey(e.detail)}
    />
  </div>
{:else if keyFixed}
  <div class="irow">
    <div class="ilab">Transparent</div>
    <div class="hint" data-role="scene-key-fixed">{keyFixed}</div>
  </div>
{/if}

<div class="irow">
  <div class="ilab">Opacity</div>
  <div class="oprow">
    <input
      type="range"
      min="0"
      max="100"
      data-role="scene-opacity"
      value={style.opacity}
      on:input={(e) => dispatch("change", { ...style, opacity: Number(e.currentTarget.value) })}
    />
    <span class="mono tiny dim" data-role="scene-opacity-value">{style.opacity} %</span>
  </div>
</div>

<div class="delwrap">
  <button class="btn del" data-role="scene-delete-layer" on:click={() => dispatch("delete")}
    >Delete layer</button
  >
</div>

<style>
  .oprow {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* `Delete layer` sits alone at the bottom, error-tinted and quiet, far from
     Save (S7's note) */
  .delwrap {
    margin-top: 26px;
  }
</style>
