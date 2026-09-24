<script lang="ts">
  // The COLOUR layer's inspector — mockup S7e, the shortest one in the app:
  // name, colour, box, blend, opacity.
  //
  // There is no *Transparent* row and it is absent rather than set to
  // `nothing`: a flat colour has no pixels to key (S7e's note).
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import SceneSwatch from "./SceneSwatch.svelte";
  import StyleTail from "./StyleTail.svelte";
  import { MAX_LAYER_NAME, truncateUtf8, type Layer } from "../../lib/scene";

  export let layer: Layer;

  const dispatch = createEventDispatcher<{ change: Layer }>();

  $: color = layer.body.kind === "color" ? layer.body.color : "000000";

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

<div class="rhead" style="margin-bottom:12px"><div class="slabel">Color layer</div></div>

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
  <div class="ilab">Color</div>
  <div>
    <SceneSwatch
      value={color}
      label="layer colour"
      dataRole="scene-wash-color"
      on:input={(e) => patchLayer({ body: { kind: "color", color: e.detail } })}
    />
  </div>
</div>

<BoxRow
  rect={layer.style.rect}
  on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })}
/>

<div class="irule"></div>

<StyleTail style={layer.style} on:change={(e) => patchLayer({ style: e.detail })} on:delete />
