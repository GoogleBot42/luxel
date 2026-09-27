<script lang="ts">
  // The per-layer colour ramp — the scene editor's mount of the ONE ramp
  // editor (mockups S8e collapsed, S8f expanded; they replace the row S7b/S7g
  // drew). Gitea #787.
  //
  // It is the engine's output-palette stage applied per layer — brightness → a
  // gradient of stops — so a white-on-black pattern becomes any gradient
  // without touching its code (§5.5).
  //
  // This file is a THIN ADAPTER and nothing more: the control is
  // `components/ColorRamp.svelte`, the same one Settings › Advanced › Output
  // processing › Color ramp mounts. All this adds is the `Ramp | null` shape
  // the layer record stores and the "no ramp yet" teaser that offers to make
  // one.
  import { createEventDispatcher } from "svelte";
  import ColorRamp from "../ColorRamp.svelte";
  import { paintLut, rampLut, type GradientStop } from "../../lib/gradient";
  import { type Ramp } from "../../lib/scene";

  export let ramp: Ramp | null = null;
  /** The layer's pattern source, for the editor's preview pair (S8f draws the
   *  layer and the layer through the ramp, side by side). */
  export let patternSource: string | null | undefined = null;
  /** Live when this scene is the one playing, saved otherwise — #563's split,
   *  which the pre-#787 mount threw away by wiring `on:input` and `on:change`
   *  to one handler (inventory item 20). */
  export let live = false;

  const dispatch = createEventDispatcher<{ input: Ramp | null }>();

  /** Is the editor open? Bound from the component, because the inspector's own
   *  `Color ramp` label is the COLLAPSED row's (S8e) and the expanded card
   *  carries its own header (S8f). */
  let editorOpen = false;

  /** What the default ramp is when one is first added: black → white, the
   *  identity-ish ramp you then drag colour into (it is also the `Mono`
   *  preset, so the editor opens on a chip that is already lit). */
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

  /** The teaser's bar is painted by the SAME table the editor's is — there is
   *  no second gradient anywhere in this file. Reactive rather than `onMount`,
   *  because this branch can mount long after the component does (removing a
   *  ramp switches back to it), and `bind:this` invalidating `seedBar` is what
   *  re-runs it. */
  const SEED_STOPS: GradientStop[] = SEED.stops.map(([pos, hex]) => ({ pos, hex }));
  let seedBar: HTMLCanvasElement | null = null;
  $: paintLut(seedBar, rampLut(SEED_STOPS, SEED.pct));

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
  <!-- S8e / S8f: the one editor, collapsed to a summary row or open in place.
       The inspector's own `Color ramp` label belongs to the COLLAPSED row only
       (S8e draws it, S8f does not): expanded, the card's own `.rehead` says
       `Color ramp`, and keeping both put it on screen twice. -->
  <div class="irow wide">
    {#if !editorOpen}<div class="ilab">Color ramp</div>{/if}
    <div>
      <ColorRamp
        stops={toStops(ramp)}
        amount={ramp.pct}
        role="scene-ramp"
        previewRole="scene-ramp"
        scope="layer"
        mode={live ? "live" : "save"}
        {patternSource}
        bind:expanded={editorOpen}
        on:input={(e) => edit(e.detail.stops, e.detail.amount)}
        on:change={(e) => edit(e.detail.stops, e.detail.amount)}
        on:clear={() => dispatch("input", null)}
      />
    </div>
  </div>
{:else}
  <!-- no ramp yet — the bar, `Edit…`, and what a ramp is for -->
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
    color: var(--accent);
  }

  .lnk:hover {
    border-color: transparent;
    color: var(--text);
  }
</style>
