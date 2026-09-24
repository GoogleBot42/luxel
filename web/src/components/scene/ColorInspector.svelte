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
  import type { Layer } from "../../lib/scene";

  export let layer: Layer;

  const dispatch = createEventDispatcher<{ change: Layer }>();

  $: color = layer.body.kind === "color" ? layer.body.color : "000000";

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
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
    on:change={(e) => patchLayer({ name: e.currentTarget.value })}
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
