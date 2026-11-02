<script lang="ts">
  // The layer's box, as four numbers (mockup S7 `.irow.wide > .boxrow`).
  //
  // It is the EXACT fallback for dragging the marquee on the preview, which
  // is the primary way geometry is set (§5.5) — so the two write the same
  // `style.rect` and neither is authoritative.
  //
  // `w`/`h` = 0 means "the whole layout" on that axis (docs/spec/scenes.md
  // §2) — for a pattern, a text or a colour layer. A SPRITE layer never shows
  // a 0 here any more: the editor normalises its box to the sprite's own size
  // the moment a sprite is bound, so the four fields, the marquee and the Fit
  // chooser are all talking about the same explicit rectangle (Jeremy,
  // 2026-09-26: "the stretch fit should force the UI to see the w + h the same
  // always"). The `sizeReadonly` prop that used to grey w/h for a sprite went
  // with it — scaling is real since #740/#741 and nothing here is read-only.
  //
  // ON A STRIP the box is a SEGMENT (Gitea #985, docs/spec/scenes.md §2): the
  // canvas is one row, so `x` and `w` place the layer along it and `y`/`h`
  // have nothing to say (`y` 0, `h` 0 or 1 are both the row). The two fields
  // are absent there rather than disabled; an edit keeps whatever the record
  // carries for them. The canvas is read off the ONE Layout, exactly as the
  // scene editor's own rig is (`sceneCanvas`), so no inspector has to pass it.
  import { createEventDispatcher } from "svelte";
  import type { Rect } from "../../lib/scene";
  import { layout, sceneCanvas } from "../../stores/geometry";

  export let rect: Rect;
  export let dataRole = "scene-box";

  $: segment = sceneCanvas($layout)?.h === 1;

  const dispatch = createEventDispatcher<{ input: Rect }>();

  function set(field: keyof Rect, raw: string): void {
    const v = Number.parseInt(raw, 10);
    if (!Number.isFinite(v)) return;
    dispatch("input", { ...rect, [field]: v });
  }
</script>

<div class="irow wide">
  <div class="ilab">Box</div>
  <div class="boxrow" data-role={dataRole} data-segment={segment ? "" : undefined}>
    <label
      >x <input
        class="inp"
        type="number"
        data-role={`${dataRole}-x`}
        value={rect.x}
        on:change={(e) => set("x", e.currentTarget.value)}
      /></label
    >
    {#if !segment}
      <label
        >y <input
          class="inp"
          type="number"
          data-role={`${dataRole}-y`}
          value={rect.y}
          on:change={(e) => set("y", e.currentTarget.value)}
        /></label
      >
    {/if}
    <label
      >w <input
        class="inp"
        type="number"
        data-role={`${dataRole}-w`}
        value={rect.w}
        on:change={(e) => set("w", e.currentTarget.value)}
      /></label
    >
    {#if !segment}
      <label
        >h <input
          class="inp"
          type="number"
          data-role={`${dataRole}-h`}
          value={rect.h}
          on:change={(e) => set("h", e.currentTarget.value)}
        /></label
      >
    {/if}
  </div>
</div>

<style>
  /* the spinners would not fit a 46px field and the marquee is the real
     control anyway */
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
