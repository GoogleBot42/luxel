<script lang="ts">
  // The quiet Projection row (proposal §5.4d, mockups S2c/S2d).
  //
  // "How a pattern made for another layout is shown on this one" is a status
  // line, not a control: one dim row under a hairline, AFTER the pattern's own
  // values, saying what is in force and offering one word to change it. It is
  // visible only when it can matter — the pattern's dimensionality differs
  // from the Layout's native one AND that Layout offers more than one option.
  // A 2D pattern on a matrix renders nothing at all (S2d), not a disabled
  // control and not an "n/a" line.
  //
  // The option list and every label come from the ENGINE
  // (`Luxel.projectionOptions`), so the editor, a playlist row and Settings
  // can never caption the same projection differently.
  import { createEventDispatcher } from "svelte";
  import type { Layout, PatternDims, ProjectionMode } from "../lib/geometry";
  import type { ProjectionOption } from "../lib/luxel";
  import { isPlayground } from "../stores/device";
  import { luxel } from "../stores/pattern";

  /** What the compiled pattern asks for (`preferredDims()`; 0 reads as 1D). */
  export let patternDims: PatternDims = 0;
  /** The Layout it is being shown on — its `projection` triple is the default. */
  export let layout: Layout;
  /** The working copy's override, or null while it inherits the default. */
  export let override: ProjectionMode | null = null;

  const dispatch = createEventDispatcher<{ set: ProjectionMode | null }>();

  let open = false;

  /** 1/2/3 — `preferredDims()`'s 0 means "no preference", i.e. a 1D pattern. */
  $: pd = (patternDims === 0 ? 1 : patternDims) as 1 | 2 | 3;
  $: options = $luxel ? $luxel.projectionOptions(pd, layout.dims) : ([] as ProjectionOption[]);
  /** The device's (or the playground's) default for a pattern of these dims. */
  $: inherited =
    pd === 3 ? layout.projection.proj3d : pd === 2 ? layout.projection.proj2d : layout.projection.proj1d;
  $: active = override ?? inherited;
  $: activeLabel = options.find((o) => o.mode === active)?.label ?? active;
  // A single-option cell is a note, never a control — and a native pair has no
  // options at all, so the row disappears entirely (§5.4d, S2d).
  $: show = options.length > 1;
  // Choosing the inherited mode explicitly is not an override: it would read
  // in accent and offer "reset" while changing nothing.
  $: pick = (mode: ProjectionMode) => {
    open = false;
    dispatch("set", mode === inherited ? null : mode);
  };
</script>

{#if show}
  <div class="hairline"></div>
  <div class="projrow" data-role="projection-row" data-mode={active}>
    <span class="pl">Projection</span>
    <button
      class="pv"
      class:ovr={override !== null}
      data-role="projection-value"
      title="how this pattern's dimensionality is mapped onto this layout"
      on:click={() => (open = !open)}
    >
      {#if override !== null}
        {activeLabel} · override
      {:else}
        {$isPlayground ? "default" : "device default"} · {activeLabel}
      {/if}
    </button>
    {#if override !== null}
      <button class="pa" data-role="projection-reset" on:click={() => dispatch("set", null)}>
        reset
      </button>
    {:else}
      <button class="pa" data-role="projection-change" on:click={() => (open = !open)}>
        change
      </button>
    {/if}
  </div>
  {#if open}
    <div class="projopts" data-role="projection-options">
      {#each options as o (o.mode)}
        <button
          class="opt"
          class:sel={o.mode === active}
          data-role="projection-opt-{o.mode}"
          on:click={() => pick(o.mode)}
        >
          {o.label}
          {#if o.mode === inherited}<span class="dim">· default</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .hairline {
    height: 1px;
    margin: 10px 0 8px;
    background: var(--border);
  }

  .projrow {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .pl {
    color: var(--text-dim);
  }

  /* the value is the thing you read; it opens the cards, but it is not a
     control — no border, no background (S2c) */
  .pv {
    background: transparent;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--text-dim);
    cursor: pointer;
    text-align: left;
  }

  /* an override reads in accent, so a glance separates "inherited" from
     "this item deviates" (§5.4d) */
  .pv.ovr {
    color: var(--accent);
  }

  .pa {
    margin-left: auto;
    background: transparent;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }

  .projopts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 6px;
  }

  .opt {
    font-size: 12px;
    padding: 4px 8px;
  }

  .opt.sel {
    border-color: var(--accent);
    color: var(--accent);
  }

  .dim {
    color: var(--text-dim);
  }
</style>
