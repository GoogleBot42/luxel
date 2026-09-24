<script lang="ts">
  // The PATTERN layer's inspector — mockups S7b (Blend = Normal, so the
  // transparency key is offered) and S7g (a non-Normal blend, where the key
  // row is GONE, plus a ramp with stops and the quiet Projection row).
  //
  // Field order is the mocks': the pattern itself, its own controls, the
  // projection line under a hairline, the colour ramp, the box and fit, then
  // the blend tail. Nothing about a layer is edited in two places — the box
  // here is the same `style.rect` the marquee drags.
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import RampEditor from "./RampEditor.svelte";
  import StyleTail from "./StyleTail.svelte";
  import Controls from "../Controls.svelte";
  import PatternThumb from "../PatternThumb.svelte";
  import ProjectionRow from "../ProjectionRow.svelte";
  import { parseControlHints } from "../../lib/hints";
  import type { Control, Engine } from "../../lib/luxel";
  import type { Layout, PatternDims, ProjectionMode } from "../../lib/geometry";
  import { FITS, type Fit, type Layer } from "../../lib/scene";
  import { luxel } from "../../stores/pattern";

  export let layer: Layer;
  /** The engine the stage is already running this layer on — its `controls()`
   *  and `patternDims()` are read from here rather than from a throwaway
   *  compile (`PlaylistRow.svelte` does the latter; a scene already has one). */
  export let engine: Engine | null = null;
  /** The layer's pattern source and name, resolved by the page. */
  export let source: string | undefined = undefined;
  export let patternName = "";
  /** The rig the scene renders on — what the projection row is relative to. */
  export let rig: Layout;

  const dispatch = createEventDispatcher<{ change: Layer; pick: void }>();

  $: pat = layer.body.kind === "pat" ? layer.body.pat : null;
  $: controls = (engine?.controls() ?? []) as Control[];
  $: hints = parseControlHints(source ?? "");
  $: dims = (engine?.patternDims() ?? 0) as PatternDims;
  /**
   * The line under the pattern's name. Its numbers are what the pattern
   * actually renders, not the fixture's: a 2D pattern on a 64×64 panel reads
   * `2D · 4096 px` (S7b), and a 1D one projected along an axis reads
   * `1D · 64 px stretched` (S7g) — the strip the engine gives it, and the
   * word for what the projection then does with it.
   */
  $: shapeLine = shapeOf(engine, dims, rig.pixels);

  function shapeOf(e: Engine | null, d: PatternDims, layoutPixels: number): string {
    const head = d === 0 ? "any" : `${d}D`;
    const eff = e?.effectiveGeometry();
    const px = eff?.pixelCount ?? layoutPixels;
    const stretched = eff !== undefined && px !== layoutPixels;
    return `${head} · ${px} px${stretched ? " stretched" : ""}`;
  }

  const readouts = new Map<string, number>();

  function patchPat(next: Partial<NonNullable<typeof pat>>): void {
    if (!pat) return;
    dispatch("change", { ...layer, body: { kind: "pat", pat: { ...pat, ...next } } });
  }

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
  }

  // The narrowing lives in the script, not in the markup: a TS assertion
  // inside a template expression is not something svelte-check parses.
  function setProj(mode: ProjectionMode | null): void {
    patchPat({ proj: mode });
  }

  function setFit(v: string): void {
    patchLayer({ style: { ...layer.style, fit: v as Fit } });
  }
</script>

{#if pat}
  <div class="rhead" style="margin-bottom:12px"><div class="slabel">Pattern layer</div></div>

  <div class="irow wide">
    <div class="ilab">Pattern</div>
    <div class="patrow" data-role="scene-pattern">
      {#if $luxel}
        <PatternThumb luxel={$luxel} {source} proj={pat.proj} previewRig={rig} />
      {/if}
      <div class="who">
        <div class="pnm" data-role="scene-pattern-name">{patternName || "no pattern"}</div>
        <div class="hint" data-role="scene-pattern-shape">{shapeLine}</div>
      </div>
      <button class="btn sm" data-role="scene-pattern-change" on:click={() => dispatch("pick")}
        >Change…</button
      >
    </div>
  </div>

  <div class="rhead" style="margin:16px 0 6px">
    <div class="slabel" data-role="scene-controls-label">Controls</div>
  </div>
  <div class="controls" data-role="scene-controls">
    <Controls
      {controls}
      values={pat.controls}
      {readouts}
      {hints}
      on:set={(e) => patchPat({ controls: { ...pat.controls, [e.detail.name]: e.detail.values } })}
    />
  </div>

  <!-- §5.4d's third home for the same row (editor · playlist row · layer
       inspector): absent when the pattern is native to this Layout. -->
  <ProjectionRow
    patternDims={dims}
    layout={rig}
    override={pat.proj}
    on:set={(e) => setProj(e.detail)}
  />

  <div class="irule"></div>

  <RampEditor ramp={pat.ramp} on:input={(e) => patchPat({ ramp: e.detail })} />

  <div class="irule"></div>

  <BoxRow
    rect={layer.style.rect}
    on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })}
  />

  <div class="irow">
    <div class="ilab">Fit</div>
    <select
      class="sel wide"
      data-role="scene-fit"
      value={layer.style.fit}
      on:change={(e) => setFit(e.currentTarget.value)}
    >
      {#each FITS as f (f)}<option value={f}>{f}</option>{/each}
    </select>
  </div>

  <div class="irule"></div>

  <StyleTail
    style={layer.style}
    keyable
    on:change={(e) => patchLayer({ style: e.detail })}
    on:delete
  />
{/if}

<style>
  /* mock S7b: a 44px thumbnail, the name and its shape line, then `Change…` */
  .patrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .patrow :global(.thumb .sq) {
    width: 44px;
    height: 44px;
    border-radius: 4px;
  }

  .who {
    flex: 1;
    min-width: 0;
  }

  .pnm {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .who .hint {
    white-space: nowrap;
  }

  /* The pattern's own controls rail, verbatim from the editor
     (`components/Controls.svelte`) on the inspector's rhythm: the mock draws
     a control row as an `.irow` (86px label, 10px gap, 11px below) rather
     than the editor rail's `.ctlrow`. Same three cells, same widget. */
  .controls :global(.control) {
    grid-template-columns: 86px minmax(0, 1fr) auto;
    gap: 10px;
    margin: 0 0 11px;
  }

  .controls :global(.label) {
    font-size: 12.5px;
  }

  .controls :global(.inp.xs.num) {
    width: 58px;
  }
</style>
