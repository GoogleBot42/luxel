<script lang="ts">
  // The device's output chain: color order, gamma, power cap, brightness
  // curve, blur, glow — and the output palette (Gitea #139).
  import {
    device,
    outputStatus,
    paletteAmount,
    paletteFlat,
    paletteSupported,
    refreshOutput,
    type OutputStatus,
  } from "../stores/device";
  import { note, notes } from "../stores/notify";

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
      note("palette", res.ok ? "" : (res.error ?? "rejected"));
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

<section class="card">
  <h2>Output</h2>
  {#if out}
    <div class="field">
      <span class="flabel">Color order</span>
      <select data-role="out-order" bind:value={out.order} on:change={onOutputChange}>
        {#each ["rgb", "rbg", "grb", "gbr", "brg", "bgr"] as o}
          <option value={o}>{o.toUpperCase()}</option>
        {/each}
      </select>
      <span class="dim">match your strip's wiring (colors swapped? try GRB/BGR)</span>
    </div>
    <div class="field">
      <span class="flabel">Gamma</span>
      <input
        class="num"
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
      <span class="dim">0 = off; 2.2 gives smoother dark fades on the strip</span>
    </div>
    <div class="field">
      <span class="flabel">Power cap</span>
      <input
        class="num"
        data-role="out-cap"
        type="number"
        min="0"
        max="20000"
        step="100"
        bind:value={out.capMa}
        on:change={onOutputChange}
      />
      <span class="dim">mA — frames estimated above this get scaled down; 0 = off</span>
    </div>
    <div class="field">
      <span class="flabel">Brightness curve</span>
      <input
        class="num"
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
      <span class="dim">0 = off; 2.2 makes the dimmer feel linear</span>
    </div>
    <div class="field">
      <span class="flabel">Blur</span>
      <input
        class="num"
        data-role="out-blur"
        type="number"
        min="0"
        max="100"
        step="5"
        bind:value={out.blur}
        on:change={onOutputChange}
      />
      <span class="dim">% — softens the frame along the pixel index</span>
    </div>
    <div class="field">
      <span class="flabel">Glow</span>
      <input
        class="num"
        data-role="out-glow"
        type="number"
        min="0"
        max="100"
        step="5"
        bind:value={out.glow}
        on:change={onOutputChange}
      />
      <span class="dim">% — bright pixels bleed into their neighbours</span>
    </div>
    {#if $paletteSupported}
      <div class="field">
        <span class="flabel">Palette</span>
        <div class="palette-edit">
          <div
            class="palette-preview"
            data-role="out-palette-preview"
            style="background: {paletteCss(stops)}"
          >
            {#if stops.length === 0}<span class="dim">no device palette</span>{/if}
          </div>
          {#each stops as stop, i (i)}
            <div class="palette-stop">
              <input
                type="color"
                data-role="out-palette-color"
                bind:value={stop.hex}
                on:change={onPaletteChange}
              />
              <input
                class="num"
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
            <button
              data-role="out-palette-add"
              disabled={stops.length >= MAX_PALETTE_STOPS}
              on:click={addPaletteStop}>add stop</button
            >
            <button
              data-role="out-palette-clear"
              disabled={stops.length === 0}
              on:click={clearPalette}>clear</button
            >
            <label class="dim">
              amount
              <input
                class="num"
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
          {#if $notes.palette}
            <span class="dim" data-role="out-palette-note">{$notes.palette}</span>
          {/if}
        </div>
        <span class="dim">
          recolors every frame by brightness through these stops — persisted on the device
          and stacked on top of whatever the pattern's own setOutputPalette did (max 32
          stops)
        </span>
      </div>
    {/if}
  {:else}
    <p class="dim hint">not available on this firmware</p>
  {/if}
</section>
