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
  import { createEventDispatcher } from "svelte";
  import { BLENDS, KEYS, type Blend, type LayerKey, type LayerStyle } from "../../lib/scene";

  export let style: LayerStyle;
  /** Pattern and text layers key their pixels; colour and sprite do not. */
  export let keyable = false;
  /** A sprite's fixed key, stated rather than offered (S7c). */
  export let keyFixed = "";

  const dispatch = createEventDispatcher<{ change: LayerStyle; delete: void }>();

  /** The mocks capitalise a blend in the pattern and colour inspectors. */
  const BLEND_LABEL: Record<Blend, string> = {
    normal: "Normal",
    add: "Add",
    lighten: "Lighten",
    multiply: "Multiply",
    mask: "Mask",
  };

  /** §5.5's words, which are also the hint under the select. */
  const KEY_LABEL: Record<LayerKey, string> = {
    none: "nothing",
    black: "black pixels",
    luma: "by brightness",
  };

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
  <select
    class="sel wide"
    data-role="scene-blend"
    value={style.blend}
    on:change={(e) => setBlend(e.currentTarget.value)}
  >
    {#each BLENDS as b (b)}<option value={b}>{BLEND_LABEL[b]}</option>{/each}
  </select>
</div>

{#if keyable && style.blend === "normal"}
  <div class="irow start">
    <div class="ilab" style="padding-top:7px">Transparent</div>
    <div>
      <select
        class="sel wide"
        data-role="scene-key"
        value={style.key}
        on:change={(e) => setKey(e.currentTarget.value)}
      >
        {#each KEYS as k (k)}<option value={k}>{KEY_LABEL[k]}</option>{/each}
      </select>
      <div class="hint" style="margin-top:6px">nothing · black pixels · by brightness</div>
    </div>
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
