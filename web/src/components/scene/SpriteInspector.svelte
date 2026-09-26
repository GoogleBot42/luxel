<script lang="ts">
  // The SPRITE layer's inspector, after the #740/#741 redesign:
  // Name · Sprite · Box · Fit · the blend tail. And nothing else.
  //
  // What LEFT, and why:
  //   * Size / Frames / Palette — those describe the SPRITE, not the layer, and
  //     a sprite is its own record with its own editor now. The Frames field in
  //     particular was the one Jeremy caught doing nothing ("I set frames to 2
  //     and 3 and saw nothing change"); it lives in the sprite editor's frame
  //     strip, which is a real control.
  //   * "Black pixels · sprites are always keyed" — a dead line stating a fact
  //     about the format. Transparency is index 0 in the record now; there is
  //     nothing to choose.
  //
  // What ARRIVED: a real sprite PICKER over the sprite store (the old empty
  // state offered `Change…` only while the layer was unbound, so a layer bound
  // to the wrong sprite had no way back), `Edit ↗` into the sprite editor with
  // a return route, and an editable Box with a `natural` escape — because
  // scaling is real now (#741 item 29: "'1:1 · sprites are never scaled'
  // should that be an option?" — yes, it is `Fit`).
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import StyleTail from "./StyleTail.svelte";
  import RichSelect from "../RichSelect.svelte";
  import { FIT_OPTIONS, fitValue, type RichOption } from "../../lib/blendMeta";
  import { MAX_LAYER_NAME, truncateUtf8, type Fit, type Layer } from "../../lib/scene";
  import { spriteMetaLine } from "../../lib/sprite";
  import type { SpriteMeta } from "../../stores/sprites";

  export let layer: Layer;
  /** The sprite library — the picker's rows. */
  export let library: SpriteMeta[] = [];
  /** True while the sprite store has a write in flight. */
  export let saving = false;

  const dispatch = createEventDispatcher<{
    change: Layer;
    /** Open the sprite editor on this layer's sprite, returning here. */
    edit: void;
    /** Make a blank sprite, bind it, and open it. */
    fresh: void;
  }>();

  $: id = layer.body.kind === "sprite" ? layer.body.id : "";
  $: current = library.find((s) => s.id === id) ?? null;
  /** The box, in the two states it has: unset on either axis = the sprite's
   *  NATURAL size at (x, y) — the contract's rule, and the reason Fit only
   *  appears once the box has a size to fit into. */
  $: natural = layer.style.rect.w === 0 || layer.style.rect.h === 0;

  /** One row per stored sprite, with its size and frame count as the row's
   *  sentence — the picker explains itself like every other `RichSelect`. */
  $: options = library.map(
    (s): RichOption => ({
      value: s.id,
      label: s.name,
      desc: spriteMetaLine(s.w, s.h, s.frames),
      icon: "sprite",
    }),
  );

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
  }

  function setSprite(nextId: string): void {
    if (layer.body.kind !== "sprite" || nextId === layer.body.id) return;
    dispatch("change", { ...layer, body: { kind: "sprite", id: nextId } });
  }

  /** Fit is the ONE place `style.fit` means anything (`compose::blit_sprite`).
   *  The narrowing lives here rather than in the markup: svelte-check does not
   *  parse a TS assertion inside a template expression. */
  function setFit(v: string): void {
    patchLayer({ style: { ...layer.style, fit: v as Fit } });
  }

  /** The device refuses a layer name over 32 BYTES, and a refusal the console
   *  could have prevented is the console's bug — clamp on the way in and put
   *  the clamped value back in the field (`MAX_LAYER_NAME`). */
  function setName(el: HTMLInputElement): void {
    const name = truncateUtf8(el.value, MAX_LAYER_NAME);
    if (name !== el.value) el.value = name;
    patchLayer({ name });
  }

  /** Back to the sprite's own size: zero the box on both axes. The inverse is
   *  typing a size into w/h, or dragging a corner on the stage. */
  function goNatural(): void {
    patchLayer({ style: { ...layer.style, rect: { ...layer.style.rect, w: 0, h: 0 } } });
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

<!-- The picker is ALWAYS here, not only while the layer is unbound: changing
     which sprite a layer draws is an ordinary edit, and the old inspector had
     no row for it once one was chosen. -->
<div class="irow start">
  <div class="ilab" style="padding-top:7px">Sprite</div>
  <div class="spritepick">
    {#if library.length === 0}
      <div class="hint" data-role="scene-sprite-state">no sprites yet</div>
    {:else}
      <RichSelect
        value={id}
        {options}
        dataRole="scene-sprite-pick"
        menuRole="scene-sprite-menu"
        ariaLabel="which sprite this layer draws"
        on:input={(e) => setSprite(e.detail)}
      />
    {/if}
    <div class="pickrow">
      <button
        class="btn sm"
        data-role="scene-sprite-edit"
        disabled={id === ""}
        data-reason={id === "" ? "choose a sprite first, or make a new one" : null}
        title="open this sprite in the sprite editor"
        on:click={() => dispatch("edit")}>Edit ↗</button
      >
      <button class="btn sm" data-role="scene-sprite-new" on:click={() => dispatch("fresh")}
        >New…</button
      >
    </div>
  </div>
</div>

<div class="irule"></div>

<!-- w/h are EDITABLE now: with a box set, `Fit` decides how the sprite fills
     it (stretch / fit / tile). `natural` puts the box back to "the sprite's
     own size at (x, y)", which is what w/h = 0 means on the wire. -->
<BoxRow
  rect={layer.style.rect}
  on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })}
/>

<div class="irow">
  <div class="ilab">Size</div>
  <div class="pickrow">
    {#if natural}
      <span class="hint" data-role="scene-sprite-natural"
        >{current ? `${current.w}×${current.h} · ` : ""}natural size</span
      >
    {:else}
      <button
        class="btn sm"
        data-role="scene-sprite-natural-btn"
        title="put the box back to the sprite's own size"
        on:click={goNatural}>natural</button
      >
    {/if}
  </div>
</div>

{#if !natural}
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
{/if}

<div class="irule"></div>

<StyleTail style={layer.style} on:change={(e) => patchLayer({ style: e.detail })} on:delete />

<style>
  .spritepick {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .pickrow {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
