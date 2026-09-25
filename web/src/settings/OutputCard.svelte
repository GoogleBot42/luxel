<script lang="ts">
  // Advanced › Output processing: gamma, brightness curve, power cap, blur,
  // glow — and the output palette (Gitea #139).
  //
  // Colour order left for LED layout (#469): it describes the STRIP's wiring,
  // not the processing chain, and §5.7 files it with LED type and data pin.
  // Power cap and blur/glow are caps-gated — a HUB75 panel is a fixed load on
  // a supply sized for it, and the two spatial stages need neighbours the
  // board can afford (`caps.power_cap` / `caps.blur_glow`, docs/api.md).
  import {
    device,
    outputStatus,
    paletteAmount,
    paletteFlat,
    paletteSupported,
    refreshOutput,
    type OutputStatus,
  } from "../stores/device";
  import { note, notes, reportApiError } from "../stores/notify";
  import GradientEditor, { type GradientStop } from "../components/GradientEditor.svelte";

  /** Which fields this device advertises (`lib/settingsCaps.ts`). */
  export let showPowerCap = true;
  export let showBlurGlow = true;
  /** How to word blur/glow: along the strip, or across the grid. */
  export let scope: "strip" | "grid" = "strip";

  /** A local, editable mirror of the device's output settings — the fields are
   *  two-way bound, and every change pushes and then re-reads. */
  let out: OutputStatus | null = null;
  $: out = $outputStatus;

  /** The stops the editor is showing. Re-derived from the device's own
   *  reading, so a refresh is the source of truth; the editor holds its own
   *  draft while a drag is in flight, so a poll landing mid-gesture cannot
   *  snap a handle back (see `GradientEditor.svelte`). */
  let stops: GradientStop[] = [];
  $: stops = stopsFromFlat($paletteFlat);

  const byteToHexPair = (n: number): string =>
    Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");

  /** Flat [pos,r,g,b,…] (the wire form) → editable stops. */
  function stopsFromFlat(flat: readonly number[]): GradientStop[] {
    const list: GradientStop[] = [];
    for (let i = 0; i + 3 < flat.length; i += 4) {
      list.push({
        pos: flat[i] ?? 0,
        hex: `${byteToHexPair(flat[i + 1] ?? 0)}${byteToHexPair(flat[i + 2] ?? 0)}${byteToHexPair(flat[i + 3] ?? 0)}`,
      });
    }
    return list;
  }

  /** Editable stops → the flat wire form, sorted (the device requires it). */
  function flatFromStops(list: readonly GradientStop[]): number[] {
    return [...list]
      .sort((a, b) => a.pos - b.pos)
      .flatMap((s) => {
        const parsed = Number.parseInt(s.hex.replace(/^#/, ""), 16);
        const v = Number.isFinite(parsed) ? parsed : 0;
        return [
          Math.max(0, Math.min(255, Math.round(s.pos))),
          (v >> 16) & 0xff,
          (v >> 8) & 0xff,
          v & 0xff,
        ];
      });
  }

  function onOutputChange(): void {
    void (async () => {
      const o = out;
      if (!o) return;
      outputStatus.set(o);
      await $device?.setOutput(o.order, o.gamma, o.capMa, o.brightCurve, o.blur, o.glow);
      void refreshOutput();
    })();
  }

  /** Push the edited palette (or clear it when there are no stops left).
   *  Every value is passed IN: the editor commits and the store round-trip
   *  lands later, so reading `stops` here would push the pre-edit list. */
  function pushPalette(list: readonly GradientStop[], amountPct: number): void {
    void (async () => {
      const d = $device;
      if (!d) return;
      const flat = flatFromStops(list);
      const res = flat.length === 0 ? await d.clearPalette() : await d.setPalette(flat, amountPct);
      note("palette", "");
      if (!res.ok) reportApiError(res.error ?? "rejected", { scope: "output" });
      void refreshOutput();
    })();
  }

  /** A committed edit: show it at once, then push it. */
  function onPaletteChange(list: GradientStop[], amountPct: number): void {
    stops = list;
    paletteAmount.set(amountPct);
    pushPalette(list, amountPct);
  }

  function clearPalette(): void {
    stops = [];
    paletteAmount.set(0);
    pushPalette([], 0);
  }
</script>

{#if out}
  <!-- Every row is the mockup's `.srow`: the field, then what the number DOES
       in the mockup's own words (S3/S3i). -->
  <div class="field">
    <span class="flabel">Gamma</span>
    <div class="fctl row g10">
      <input
        class="inp num"
        data-role="out-gamma"
        type="number"
        min="0"
        max="5"
        step="0.1"
        value={out.gamma / 10}
        on:change={(e) => {
          if (out) out.gamma = Math.round(Number(e.currentTarget.value) * 10);
          onOutputChange();
        }}
      />
      <span class="dim hint">0 = off; 2.2 gives smoother dark fades</span>
    </div>
  </div>
  <div class="field">
    <span class="flabel">Brightness curve</span>
    <div class="fctl row g10">
      <input
        class="inp num"
        data-role="out-brightcurve"
        type="number"
        min="0"
        max="5"
        step="0.1"
        value={out.brightCurve / 10}
        on:change={(e) => {
          if (out) out.brightCurve = Math.round(Number(e.currentTarget.value) * 10);
          onOutputChange();
        }}
      />
      <span class="dim hint">makes the dimmer end feel linear</span>
    </div>
  </div>
  {#if showPowerCap}
    <div class="field">
      <span class="flabel">Power cap</span>
      <div class="fctl row g10">
        <input
          class="inp num"
          data-role="out-cap"
          type="number"
          min="0"
          max="20000"
          step="100"
          bind:value={out.capMa}
          on:change={onOutputChange}
        />
        <span class="dim hint">mA — frames estimated above this get scaled down; 0 = off</span>
      </div>
    </div>
  {/if}
  {#if showBlurGlow}
    <div class="field">
      <span class="flabel">Blur</span>
      <div class="fctl row g10">
        <input
          class="inp num"
          data-role="out-blur"
          type="number"
          min="0"
          max="100"
          step="5"
          bind:value={out.blur}
          on:change={onOutputChange}
        />
        <span class="dim hint">
          % — softens {scope === "grid" ? "across the panel" : "along the strip"}
        </span>
      </div>
    </div>
    <div class="field">
      <span class="flabel">Glow</span>
      <div class="fctl row g10">
        <input
          class="inp num"
          data-role="out-glow"
          type="number"
          min="0"
          max="100"
          step="5"
          bind:value={out.glow}
          on:change={onOutputChange}
        />
        <span class="dim hint">% — bright pixels bleed into their neighbours</span>
      </div>
    </div>
  {/if}
  {#if $paletteSupported}
    <div class="field top">
      <span class="flabel">Palette</span>
      <div class="palette-edit">
        <!-- ONE gradient editor, shared with the scene layer's colour ramp
             (Gitea #734). The bar is the engine's own 256-entry table, the
             stops drag, and the colour opens the app's `ColorPicker` — this
             card's hand-rolled copy, its CSS gradient and its native
             `<input type="color">` are all gone. -->
        <GradientEditor
          {stops}
          amount={$paletteAmount}
          role="out-palette"
          previewRole="out-palette-preview"
          minStops={0}
          label="device output palette"
          summary="applied on top of the pattern's own palette"
          emptyLabel="no device palette"
          on:input={(e) => {
            stops = e.detail.stops;
            paletteAmount.set(e.detail.amount);
          }}
          on:change={(e) => onPaletteChange(e.detail.stops, e.detail.amount)}
          on:clear={clearPalette}
        />
        {#if $notes.palette}
          <span class="dim hint" data-role="out-palette-note">{$notes.palette}</span>
        {/if}
      </div>
    </div>
  {/if}
{:else}
  <p class="dim hint">not available on this firmware</p>
{/if}
