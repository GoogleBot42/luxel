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

  /** Which fields this device advertises (`lib/settingsCaps.ts`). */
  export let showPowerCap = true;
  export let showBlurGlow = true;
  /** How to word blur/glow: along the strip, or across the grid. */
  export let scope: "strip" | "grid" = "strip";

  /** One editable stop: byte position along the luma ramp + an #rrggbb color. */
  type PaletteStop = { pos: number; hex: string };
  const MAX_PALETTE_STOPS = 32;

  /** A local, editable mirror of the device's output settings — the fields are
   *  two-way bound, and every change pushes and then re-reads. */
  let out: OutputStatus | null = null;
  $: out = $outputStatus;

  let stops: PaletteStop[] = [];
  $: stops = stopsFromFlat($paletteFlat);

  const byteToHexPair = (n: number): string =>
    Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");

  /** Flat [pos,r,g,b,…] (the wire form) → editable stops. */
  function stopsFromFlat(flat: readonly number[]): PaletteStop[] {
    const list: PaletteStop[] = [];
    for (let i = 0; i + 3 < flat.length; i += 4) {
      list.push({
        pos: flat[i] ?? 0,
        hex: `#${byteToHexPair(flat[i + 1] ?? 0)}${byteToHexPair(flat[i + 2] ?? 0)}${byteToHexPair(flat[i + 3] ?? 0)}`,
      });
    }
    return list;
  }

  /** Editable stops → the flat wire form, sorted (the device requires it). */
  function flatFromStops(list: readonly PaletteStop[]): number[] {
    return [...list]
      .sort((a, b) => a.pos - b.pos)
      .flatMap((s) => {
        const v = Number.parseInt(s.hex.slice(1), 16);
        return [
          Math.max(0, Math.min(255, Math.round(s.pos))),
          (v >> 16) & 0xff,
          (v >> 8) & 0xff,
          v & 0xff,
        ];
      });
  }

  /**
   * CSS preview of the ramp the device will build from these stops —
   * including the asymmetric ends the engine's `sample_palette` has:
   * below the first stop clamps to its color, past the last stop is BLACK.
   */
  function paletteCss(list: readonly PaletteStop[]): string {
    if (list.length === 0) return "transparent";
    const sorted = [...list].sort((a, b) => a.pos - b.pos);
    const first = sorted[0];
    const last = sorted[sorted.length - 1];
    if (!first || !last) return "transparent";
    const pct = (s: PaletteStop): string => `${((s.pos / 255) * 100).toFixed(1)}%`;
    const parts = [`${first.hex} 0%`, ...sorted.map((s) => `${s.hex} ${pct(s)}`)];
    if (last.pos < 255) parts.push(`#000000 ${pct(last)}`, "#000000 100%");
    return `linear-gradient(90deg, ${parts.join(", ")})`;
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

  /** Push the edited palette (or clear it when there are no stops left). */
  function onPaletteChange(): void {
    void (async () => {
      const d = $device;
      if (!d) return;
      const flat = flatFromStops(stops);
      const res =
        flat.length === 0 ? await d.clearPalette() : await d.setPalette(flat, $paletteAmount);
      note("palette", "");
      if (!res.ok) reportApiError(res.error ?? "rejected", { scope: "output" });
      void refreshOutput();
    })();
  }

  function addPaletteStop(): void {
    if (stops.length >= MAX_PALETTE_STOPS) return;
    // seed a new stop past the last one so the list stays ascending
    const last = stops[stops.length - 1];
    stops = [
      ...stops,
      { pos: last ? Math.min(255, last.pos + 64) : 0, hex: last ? "#ffffff" : "#000000" },
    ];
    onPaletteChange();
  }

  function removePaletteStop(i: number): void {
    stops = stops.filter((_, n) => n !== i);
    onPaletteChange();
  }

  function clearPalette(): void {
    stops = [];
    onPaletteChange();
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
        <!-- the mockup's `.gradbar`: the ramp itself, with nothing written on
             it — what it holds is said in the line under the controls -->
        <div
          class="palette-preview"
          data-role="out-palette-preview"
          style="background: {paletteCss(stops)}"
        ></div>
        {#each stops as stop, i (i)}
          <div class="palette-stop">
            <input
              type="color"
              data-role="out-palette-color"
              bind:value={stop.hex}
              on:change={onPaletteChange}
            />
            <input
              class="inp num"
              type="number"
              data-role="out-palette-pos"
              min="0"
              max="255"
              bind:value={stop.pos}
              on:change={onPaletteChange}
            />
            <button
              data-role="out-palette-remove"
              title="remove this stop"
              on:click={() => removePaletteStop(i)}>remove</button
            >
          </div>
        {/each}
        <div class="palette-stop">
          <!-- The ONE deliberate §5.7 exception: a budget the user has to
               learn. It stays disabled AT the cap and carries `data-reason`,
               with the same words on screen under the row (Gitea #529). -->
          {#if stops.length >= MAX_PALETTE_STOPS}
            <button
              data-role="out-palette-add"
              disabled
              data-reason="all {MAX_PALETTE_STOPS} palette stops are used"
              >add stop</button
            >
          {:else}
            <button data-role="out-palette-add" on:click={addPaletteStop}>add stop</button>
          {/if}
          <!-- nothing to clear with no stops → absent, never disabled -->
          {#if stops.length > 0}
            <button data-role="out-palette-clear" on:click={clearPalette}>clear</button>
          {/if}
          <label class="dim">
            amount
            <input
              class="inp num"
              type="number"
              data-role="out-palette-amount"
              min="0"
              max="100"
              step="5"
              bind:value={$paletteAmount}
              on:change={onPaletteChange}
            />
            %
          </label>
        </div>
        {#if stops.length >= MAX_PALETTE_STOPS}
          <span class="dim" data-role="out-palette-cap">
            all {MAX_PALETTE_STOPS} stops are used — remove one to add another
          </span>
        {/if}
        {#if $notes.palette}
          <span class="dim hint" data-role="out-palette-note">{$notes.palette}</span>
        {/if}
        <span class="dim hint" data-role="out-palette-summary">
          {stops.length === 0
            ? "no device palette"
            : `${stops.length} stop${stops.length === 1 ? "" : "s"}`} · applied on top of the
          pattern's own palette
        </span>
      </div>
    </div>
  {/if}
{:else}
  <p class="dim hint">not available on this firmware</p>
{/if}
