<script lang="ts">
  // The quiet Projection row (proposal §5.4d, mockups S2c/S2d).
  //
  // "How a pattern made for another layout is shown on this one" is a status
  // line, not a control: one dim row under a hairline, AFTER the pattern's own
  // values, saying what is in force and offering one word to change it. A
  // native pair renders nothing at all (S2d) — not a disabled control and not
  // an "n/a" line.
  //
  // CHANGING it opens the same picture cards Settings uses (#538: "it should
  // be using the same widget as from settings in a popup" — the collapsed row
  // itself was already right). One widget, one set of labels, one place to fix
  // them: `settings/ProjectionCard.svelte` is imported as-is.
  //
  // The option list and every label come from the ENGINE
  // (`Luxel.projectionOptions`), which is also where the rule "a 1D fixture
  // shows no 2D/3D patterns, a 2D fixture no 3D ones" lives (#545). An empty
  // list therefore means "this pair is impossible or native" and the row is
  // simply absent — the UI never restates the table.
  import { createEventDispatcher } from "svelte";
  import type { Layout, PatternDims, ProjectionMode } from "../lib/geometry";
  import type { ProjectionOption } from "../lib/luxel";
  import ProjectionCard from "../settings/ProjectionCard.svelte";
  import { isPlayground } from "../stores/device";
  import { luxel } from "../stores/pattern";
  import Popover from "./Popover.svelte";

  /** What the compiled pattern asks for (`preferredDims()`; 0 reads as 1D). */
  export let patternDims: PatternDims = 0;
  /** The Layout it is being shown on — its `projection` triple is the default. */
  export let layout: Layout;
  /** The working copy's override, or null while it inherits the default. */
  export let override: ProjectionMode | null = null;

  const dispatch = createEventDispatcher<{ set: ProjectionMode | null }>();

  let open = false;
  let anchor: HTMLElement | null = null;

  /** 1/2/3 — `preferredDims()`'s 0 means "no preference", i.e. a 1D pattern. */
  $: pd = (patternDims === 0 ? 1 : patternDims) as 1 | 2 | 3;
  $: options = $luxel ? $luxel.projectionOptions(pd, layout.dims) : ([] as ProjectionOption[]);
  /** The device's (or the playground's) default for a pattern of these dims. */
  $: inherited =
    pd === 3 ? layout.projection.proj3d : pd === 2 ? layout.projection.proj2d : layout.projection.proj1d;
  $: active = override ?? inherited;
  $: activeLabel = options.find((o) => o.mode === active)?.label ?? active;
  /** Nothing to choose between → no row (§5.4d, S2d, and the #545 rule). */
  $: show = options.length > 0;
  // Choosing the inherited mode explicitly is not an override: it would read
  // in accent and offer "reset" while changing nothing.
  $: pick = (mode: ProjectionMode) => {
    open = false;
    dispatch("set", mode === inherited ? null : mode);
  };

  function useDefault(): void {
    open = false;
    dispatch("set", null);
  }
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
      on:click|stopPropagation={() => (open = !open)}
    >
      {#if override !== null}
        {activeLabel} · override
      {:else}
        {$isPlayground ? "default" : "device default"} · {activeLabel}
      {/if}
    </button>
    {#if override !== null}
      <button
        class="pa"
        bind:this={anchor}
        data-role="projection-reset"
        on:click={() => dispatch("set", null)}
      >
        reset
      </button>
    {:else}
      <button
        class="pa"
        bind:this={anchor}
        data-role="projection-change"
        on:click={() => (open = !open)}
      >
        change
      </button>
    {/if}
  </div>

  <!-- the Settings widget, in a popup. `active={open}` because each card
       compiles and animates its own preview engine — a rail full of them
       must not run while the popup is shut. -->
  <Popover
    {open}
    {anchor}
    kind="pop"
    ariaRole="dialog"
    dataRole="projection-options"
    on:close={() => (open = false)}
  >
    <div class="popcards">
      {#each options as o (o.mode)}
        <span class="cardslot" data-role={`projection-opt-${o.mode}`}>
          <ProjectionCard
            luxel={$luxel ?? null}
            {layout}
            active={open}
            patternDims={pd}
            mode={o.mode}
            label={o.label}
            selected={o.mode === active}
            on:click={() => pick(o.mode)}
          />
        </span>
      {/each}
    </div>
    <div class="sepr"></div>
    <button class="mi" data-role="projection-use-default" on:click={useDefault}>
      Use {$isPlayground ? "the" : "device"} default · {options.find((o) => o.mode === inherited)
        ?.label ?? inherited}
    </button>
  </Popover>
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
    gap: 10px;
    font-size: 12.5px;
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
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }

  /* an override reads in accent, so a glance separates "inherited" from
     "this item deviates" (§5.4d) */
  .pv.ovr {
    color: var(--accent);
  }

  /* one accent word, no rule under it (mockup S2c `.projrow .pa`) */
  .pa {
    margin-left: auto;
    background: transparent;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    cursor: pointer;
  }

  .pa:hover {
    text-decoration: underline;
  }

  /* two columns at the `.pop`'s 296px — three would give each card a 92px
     preview, which is not a picture of anything */
  .popcards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    padding: 2px;
  }

  .cardslot {
    display: block;
    min-width: 0;
  }

  /* the reset verb reads as the menu item it is, full width under the rule */
  .mi {
    display: block;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font: 13px/1.3 var(--sans);
    text-align: left;
    cursor: pointer;
  }

  .mi:hover {
    border-color: transparent;
    background: rgba(255, 255, 255, 0.05);
  }
</style>
