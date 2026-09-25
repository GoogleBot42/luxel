<script lang="ts">
  // The per-layer colour ramp (mockups S7b collapsed, S7g with stops).
  //
  // It is the engine's OUTPUT-PALETTE stage applied per layer — luma → a
  // gradient of stops — so a white-on-black pattern becomes any gradient
  // without touching its code (§5.5).
  //
  // Since #734 this file is a THIN ADAPTER: the control itself is
  // `components/GradientEditor.svelte`, the one copy that Settings › Output ›
  // Palette also mounts. All this adds is the scene's two states and the
  // `Ramp | null` shape the layer record stores:
  //   * no ramp yet — the bar, `Edit…`, and the sentence that says what a
  //     ramp DOES (S7b);
  //   * a ramp — the editor (S7g).
  import { createEventDispatcher } from "svelte";
  import GradientEditor, {
    paintRamp,
    type GradientStop,
  } from "../GradientEditor.svelte";
  import { type Ramp } from "../../lib/scene";

  export let ramp: Ramp | null = null;

  const dispatch = createEventDispatcher<{ input: Ramp | null }>();

  /** What the default ramp is when one is first added: black → white, the
   *  identity-ish ramp you then drag colour into. */
  const SEED: Ramp = {
    pct: 100,
    stops: [
      [0, "000000"],
      [255, "ffffff"],
    ],
  };

  /** `Ramp.stops` is the wire's `[pos, rrggbb]` pair; the editor speaks the
   *  named form. These two are the whole adapter. */
  const toStops = (r: Ramp): GradientStop[] => r.stops.map(([pos, hex]) => ({ pos, hex }));
  const fromStops = (list: readonly GradientStop[]): [number, string][] =>
    list.map((s) => [s.pos, s.hex]);

  /** The S7b bar is painted by the SAME table the editor's bar is — there is
   *  no second gradient anywhere in this file. Reactive rather than
   *  `onMount`, because the S7b branch can mount long after the component
   *  does (clearing a ramp switches back to it), and `bind:this` invalidating
   *  `seedBar` is what re-runs this. */
  const SEED_STOPS: GradientStop[] = SEED.stops.map(([pos, hex]) => ({ pos, hex }));
  let seedBar: HTMLCanvasElement | null = null;
  $: paintRamp(seedBar, SEED_STOPS, SEED.pct);

  function edit(stops: readonly GradientStop[], pct: number): void {
    dispatch("input", { pct, stops: fromStops(stops) });
  }

  /** A fresh copy every time — `SEED`'s own arrays must never end up inside
   *  the scene record. */
  function open(): void {
    dispatch("input", {
      pct: SEED.pct,
      stops: SEED.stops.map(([p, c]): [number, string] => [p, c]),
    });
  }
</script>

{#if ramp}
  <!-- S7g: the ramp, its draggable stops, and the one editor both homes use -->
  <div class="irow wide">
    <div class="ilab">Color ramp</div>
    <div>
      <GradientEditor
        stops={toStops(ramp)}
        amount={ramp.pct}
        role="scene-ramp"
        previewRole="scene-ramp"
        minStops={2}
        label="layer colour ramp"
        summary="the same editor as Settings › Output › Palette"
        emptyLabel="no ramp"
        on:input={(e) => edit(e.detail.stops, e.detail.amount)}
        on:change={(e) => edit(e.detail.stops, e.detail.amount)}
        on:clear={() => dispatch("input", null)}
      />
    </div>
  </div>
{:else}
  <!-- S7b: no ramp yet — the bar, `Edit…`, and what a ramp is for -->
  <div class="irow start">
    <div class="ilab" style="padding-top:4px">Color ramp</div>
    <div>
      <div class="barrow">
        <canvas class="ramp" bind:this={seedBar} data-role="scene-ramp" aria-hidden="true"></canvas>
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
    display: block;
    flex: 1;
    min-width: 0;
    height: 22px;
    border-radius: 4px;
    border: 1px solid var(--border);
    image-rendering: pixelated;
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
</style>
