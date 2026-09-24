<script lang="ts">
  // The layer's box, as four numbers (mockup S7 `.irow.wide > .boxrow`).
  //
  // It is the EXACT fallback for dragging the marquee on the preview, which
  // is the primary way geometry is set (§5.5) — so the two write the same
  // `style.rect` and neither is authoritative.
  //
  // `w`/`h` = 0 means "the whole layout" on that axis (docs/spec/scenes.md
  // §2). A sprite layer's w/h mirror the sprite and are read-only (S7c).
  import { createEventDispatcher } from "svelte";
  import type { Rect } from "../../lib/scene";

  export let rect: Rect;
  /** A sprite is never scaled, so its size is the sprite's (S7c). */
  export let sizeReadonly = false;
  export let dataRole = "scene-box";

  const dispatch = createEventDispatcher<{ input: Rect }>();

  function set(field: keyof Rect, raw: string): void {
    const v = Number.parseInt(raw, 10);
    if (!Number.isFinite(v)) return;
    dispatch("input", { ...rect, [field]: v });
  }
</script>

<div class="irow wide">
  <div class="ilab">Box</div>
  <div class="boxrow" data-role={dataRole}>
    <label
      >x <input
        class="inp"
        type="number"
        data-role={`${dataRole}-x`}
        value={rect.x}
        on:change={(e) => set("x", e.currentTarget.value)}
      /></label
    >
    <label
      >y <input
        class="inp"
        type="number"
        data-role={`${dataRole}-y`}
        value={rect.y}
        on:change={(e) => set("y", e.currentTarget.value)}
      /></label
    >
    <label
      >w <input
        class="inp"
        type="number"
        data-role={`${dataRole}-w`}
        readonly={sizeReadonly}
        value={rect.w}
        on:change={(e) => set("w", e.currentTarget.value)}
      /></label
    >
    <label
      >h <input
        class="inp"
        type="number"
        data-role={`${dataRole}-h`}
        readonly={sizeReadonly}
        value={rect.h}
        on:change={(e) => set("h", e.currentTarget.value)}
      /></label
    >
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

  /* a sprite's w/h mirror the sprite: greyed, not disabled (S7c) */
  input[readonly] {
    color: #5b6070;
  }
</style>
