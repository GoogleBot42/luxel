<script lang="ts">
  // The per-layer colour ramp (mockups S7b collapsed, S7g with stops).
  //
  // It is the engine's OUTPUT-PALETTE stage applied per layer — luma → a
  // gradient of stops — so a white-on-black pattern becomes any gradient
  // without touching its code (§5.5). Same maths, same wire shape and the
  // same stop editor Settings › Output › Palette has; the duplication of that
  // markup is tracked for extraction into one component.
  //
  // Two states, both drawn:
  //   * no ramp yet — the bar, `Edit…`, and the sentence that says what a
  //     ramp DOES (S7b);
  //   * a ramp — the bar, its draggable stop markers, `Edit stops…` and the
  //     count (S7g).
  import { createEventDispatcher } from "svelte";
  import type { Ramp } from "../../lib/scene";

  export let ramp: Ramp | null = null;

  const dispatch = createEventDispatcher<{ input: Ramp | null }>();

  let editing = false;

  /** What the default ramp is when one is first added: black → white, the
   *  identity-ish ramp you then drag colour into. */
  const SEED: Ramp = {
    pct: 100,
    stops: [
      [0, "000000"],
      [255, "ffffff"],
    ],
  };

  /** The CSS gradient the device's `sample_palette` builds: below the first
   *  stop clamps to its colour, past the last stop is BLACK (the asymmetry
   *  `settings/OutputCard.svelte` documents). */
  function rampCss(r: Ramp | null): string {
    const stops = [...(r?.stops ?? SEED.stops)].sort((a, b) => a[0] - b[0]);
    const first = stops[0];
    const last = stops[stops.length - 1];
    if (!first || !last) return "#000";
    const pct = (p: number): string => `${((p / 255) * 100).toFixed(1)}%`;
    const parts = [`#${first[1]} 0%`, ...stops.map((s) => `#${s[1]} ${pct(s[0])}`)];
    if (last[0] < 255) parts.push(`#000000 ${pct(last[0])}`, "#000000 100%");
    return `linear-gradient(90deg, ${parts.join(", ")})`;
  }

  let picked = 0;

  function edit(next: Ramp): void {
    dispatch("input", next);
  }

  function setStop(i: number, pos: number, hex: string): void {
    const r = ramp ?? SEED;
    const stops = r.stops.map((s, n): [number, string] =>
      n === i ? [Math.max(0, Math.min(255, Math.round(pos))), hex.replace(/^#/, "")] : s,
    );
    stops.sort((a, b) => a[0] - b[0]);
    edit({ ...r, stops });
  }

  function addStop(): void {
    const r = ramp ?? SEED;
    if (r.stops.length >= 32) return;
    const last = r.stops[r.stops.length - 1];
    const pos = last ? Math.min(255, last[0] + 64) : 0;
    const stops: [number, string][] = [...r.stops, [pos, "ffffff"]];
    stops.sort((a, b) => a[0] - b[0]);
    edit({ ...r, stops });
  }

  function removeStop(i: number): void {
    const r = ramp ?? SEED;
    if (r.stops.length <= 2) {
      // a ramp needs two stops; removing the second-to-last clears it
      dispatch("input", null);
      editing = false;
      return;
    }
    edit({ ...r, stops: r.stops.filter((_, n) => n !== i) });
  }

  function open(): void {
    editing = true;
    if (!ramp) edit(SEED);
  }
</script>

{#if ramp}
  <!-- S7g: the ramp itself, its stops, and one editor for both places -->
  <div class="irow wide">
    <div class="ilab">Color ramp</div>
    <div>
      <div class="gradbar" data-role="scene-ramp" style={`background:${rampCss(ramp)}`}></div>
      <div class="stoprow" data-role="scene-ramp-stops">
        {#each ramp.stops as s, i (i)}
          <button
            class="stopmk"
            class:on={picked === i}
            data-role="scene-ramp-stop"
            aria-label={`stop ${i + 1}`}
            style={`left:${((s[0] / 255) * 100).toFixed(1)}%;background:#${s[1]}`}
            on:click={() => (picked = i)}
          ></button>
        {/each}
      </div>
      <div class="editrow">
        <button class="btn sm" data-role="scene-ramp-edit" on:click={() => (editing = !editing)}
          >Edit stops…</button
        >
        <span class="hint"
          >{ramp.stops.length} stops · the same editor as Settings › Output › Palette</span
        >
      </div>
      {#if editing}
        <div class="stops" data-role="scene-ramp-editor">
          {#each ramp.stops as s, i (i)}
            <div class="stop">
              <input
                type="color"
                data-role="scene-ramp-color"
                value={`#${s[1]}`}
                on:change={(e) => setStop(i, s[0], e.currentTarget.value)}
              />
              <input
                class="inp num"
                type="number"
                min="0"
                max="255"
                data-role="scene-ramp-pos"
                value={s[0]}
                on:change={(e) => setStop(i, Number(e.currentTarget.value), s[1])}
              />
              <button class="btn sm" data-role="scene-ramp-remove" on:click={() => removeStop(i)}
                >remove</button
              >
            </div>
          {/each}
          <div class="stop">
            <button class="btn sm" data-role="scene-ramp-add" on:click={addStop}>add stop</button>
            <label class="hint">
              amount
              <input
                class="inp num"
                type="number"
                min="0"
                max="100"
                step="5"
                data-role="scene-ramp-amount"
                value={ramp.pct}
                on:change={(e) => edit({ ...ramp, pct: Number(e.currentTarget.value) })}
              />
              %
            </label>
            <button class="btn sm" data-role="scene-ramp-clear" on:click={() => dispatch("input", null)}
              >clear</button
            >
          </div>
        </div>
      {/if}
    </div>
  </div>
{:else}
  <!-- S7b: no ramp yet — the bar, `Edit…`, and what a ramp is for -->
  <div class="irow start">
    <div class="ilab" style="padding-top:4px">Color ramp</div>
    <div>
      <div class="barrow">
        <div class="ramp" data-role="scene-ramp" style={`background:${rampCss(null)}`}></div>
        <button class="lnk" data-role="scene-ramp-edit" on:click={open}>Edit…</button>
      </div>
      <div class="hint" style="margin-top:7px">
        recolors this layer by brightness — a white-on-black pattern becomes any gradient
      </div>
    </div>
  </div>
{/if}

<style>
  /* mockups.html `.ramp` (S7b): the bar shares its row with the link */
  .barrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .ramp {
    flex: 1;
    height: 22px;
    border-radius: 4px;
    border: 1px solid var(--border);
  }

  .lnk {
    padding: 0;
    border: none;
    background: transparent;
  }

  .lnk:hover {
    border-color: transparent;
    color: var(--text);
  }

  .editrow {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }

  .stops {
    margin-top: 10px;
  }

  .stop {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }

  .stop input[type="color"] {
    width: 26px;
    height: 26px;
    padding: 0;
  }
</style>
