<script lang="ts">
  // How the console's 2D preview is DRAWN (Gitea #786) — squares, or the
  // HUB75-like dot look.
  //
  // A browser preference, like the clock card's zone name: it never reaches
  // the device, and it changes nothing about what is rendered or pushed. The
  // same chooser is in the "Preview as" popover, which is the playground's
  // only copy of it (the playground has no Settings tab).
  import { parsePreviewStyle } from "../lib/draw";
  import { previewStyle } from "../stores/prefs";

  function onChange(e: Event): void {
    const want = parsePreviewStyle((e.target as HTMLSelectElement).value);
    if (want) previewStyle.set(want);
  }
</script>

<div class="field">
  <span class="flabel">Preview style</span>
  <div class="fctl row g10">
    <select class="w114" data-role="preview-style-setting" value={$previewStyle} on:change={onChange}>
      <option value="squares">Squares</option>
      <option value="panel">LED panel</option>
    </select>
    <span class="dim hint" data-role="preview-style-hint">
      LED panel draws one round emitter per pixel on a black substrate, with a
      soft bloom — the fixture, rather than hard tiles. The 2D preview only;
      tiles, thumbnails, the strip and the 3D view keep their own looks. Stored
      in this browser.
    </span>
  </div>
</div>
