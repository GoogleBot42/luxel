<script lang="ts">
  // One playlist entry (proposal §5.4, mockups S4/S4b):
  //
  //   handle · device-shaped thumbnail · name + type · duration chip ·
  //   `N values ▾` chip · ✕
  //
  // Both chips expand IN PLACE. Values live on the item and nowhere else
  // (D6 — named presets were dropped), so the same pattern can sit in the
  // playlist twice with two different looks, and the sliders you see here are
  // that item's, edited where they are used. Under them sits
  // `ProjectionRow` — the SAME component the editor's Controls rail uses
  // (#468), so a projection is captioned identically wherever it is edited,
  // and it renders nothing at all unless it can matter (§5.4d).
  //
  // The pattern's source is compiled locally once, purely to read its control
  // metadata and its own dimensionality; the thumbnail compiles its own
  // (smaller) engine through `stores/geometry.ts`.
  import { createEventDispatcher } from "svelte";
  import Controls from "./Controls.svelte";
  import PatternThumb from "./PatternThumb.svelte";
  import ProjectionRow from "./ProjectionRow.svelte";
  import { Engine, Luxel } from "../lib/luxel";
  import type { Control } from "../lib/luxel";
  import { parseControlHints } from "../lib/hints";
  import type { ControlHint } from "../lib/hints";
  import type { PlaylistItem } from "../lib/device";
  import {
    layout,
    projectionOptions,
    type PatternDims,
    type ProjectionMode,
  } from "../stores/geometry";

  export let luxel: Luxel;
  export let source: string | undefined;
  export let item: PlaylistItem;
  export let defaultSec: number;
  export let missing = false;
  export let active = false;
  export let first = false;
  export let last = false;

  const dispatch = createEventDispatcher<{
    change: void;
    /** A single control the user just moved — the page pushes it live when
     *  this row is the one playing, so the fixture follows the slider. */
    control: { name: string; values: number[] };
    remove: void;
    move: number;
    dragstart: void;
    drop: void;
  }>();

  let dragover = false;
  /** The two in-place disclosures (the chips). Values open by default on the
   *  playing row would fight the list; both start closed. */
  let valuesOpen = false;
  let durOpen = false;

  let controls: Control[] = [];
  let hints = new Map<string, ControlHint>();
  let dims: PatternDims = 0;
  let built = "";
  const readouts = new Map<string, number>();

  // compile once (per source) to read the control list, the //# hints and the
  // pattern's own dimensionality (which decides whether a projection applies)
  $: if (luxel && source !== undefined && source !== built) {
    built = source;
    const e = luxel.compile(source, 64);
    if (e instanceof Engine) {
      controls = e.controls();
      dims = e.preferredDims();
      e.free();
    } else {
      controls = [];
      dims = 0;
    }
    hints = parseControlHints(source);
  }

  // settable controls only (showNumber/gauge are read-only readouts)
  $: params = controls.filter((c) => c.kind !== "showNumber" && c.kind !== "gauge");

  $: effective = item.sec ?? defaultSec;
  $: durationLabel = effective > 0 ? `${effective} s` : "manual";
  $: overridden = item.sec !== null;
  $: kindLabel = (item.kind ?? "pattern") === "scene" ? "Scene" : "Pattern";

  // ---- projection (§5.4d) ----
  // `ProjectionRow` decides for itself whether to render; this predicate is
  // the same test, needed one level up so the chip that OPENS the panel is
  // itself absent when there is nothing inside it. (The pure table here and
  // the engine's list `ProjectionRow` reads are parity-tested — #463.)
  $: projApplies = projectionOptions(dims, $layout.dims).length > 1;

  function onSet(e: CustomEvent<{ name: string; values: number[] }>): void {
    item.controls = { ...item.controls, [e.detail.name]: e.detail.values };
    item = item; // trigger local reactivity (in-place prop mutation)
    dispatch("control", e.detail);
    dispatch("change");
  }

  /** `null` = follow the device default, which is the ABSENCE of the field,
   *  not a value equal to it: the item then tracks a later default change. */
  function onProj(e: CustomEvent<ProjectionMode | null>): void {
    if (e.detail === null) delete item.proj;
    else item.proj = e.detail;
    item = item;
    dispatch("change");
  }

  function toggleOverride(e: Event): void {
    item.sec = (e.target as HTMLInputElement).checked ? Math.max(1, defaultSec || 30) : null;
    item = item;
    dispatch("change");
  }

  function onSec(e: Event): void {
    const v = (e.target as HTMLInputElement).value.trim();
    item.sec = v === "" ? 0 : Math.max(0, Math.round(Number(v) || 0));
    item = item;
    dispatch("change");
  }
</script>

<li
  class="row"
  class:active
  class:missing
  class:dragover
  data-role="playlist-item"
  on:dragover|preventDefault={() => (dragover = true)}
  on:dragleave={() => (dragover = false)}
  on:drop|preventDefault={() => {
    dragover = false;
    dispatch("drop");
  }}
>
  <div class="head">
    <span class="handle">
      <span
        class="grip"
        data-role="pl-grip"
        title="drag to reorder"
        role="button"
        tabindex="-1"
        aria-label="drag to reorder"
        draggable="true"
        on:dragstart={() => dispatch("dragstart")}>{active ? "▶" : "⠿"}</span
      >
      <!-- drag is a mouse gesture; these are the phone's (and the keyboard's)
           way to reorder, so they are always rendered, never hover-only -->
      <span class="movers">
        <button class="mv" title="move up" aria-label="move up" disabled={first}
          on:click={() => dispatch("move", -1)}>↑</button
        >
        <button class="mv" title="move down" aria-label="move down" disabled={last}
          on:click={() => dispatch("move", 1)}>↓</button
        >
      </span>
    </span>
    {#if luxel && !missing}<PatternThumb {luxel} {source} proj={item.proj ?? null} />{/if}
    <span class="who">
      <span class="name" data-role="pl-name">
        {item.name || item.id}{#if missing}<span class="miss"> (deleted)</span>{/if}
      </span>
      <span class="type dim">
        {kindLabel}{#if projApplies && item.proj}<span class="ovr-note">· projected</span>{/if}
      </span>
    </span>
    {#if item.invalid}
      <span class="invalid" data-role="pl-invalid" title={item.invalid}>⚠ won't run</span>
    {/if}
    <span class="chips">
      <button
        class="chip"
        class:ovr={overridden}
        class:open={durOpen}
        data-role="pl-duration"
        title="how long this item plays"
        aria-expanded={durOpen}
        on:click={() => (durOpen = !durOpen)}>{durationLabel} {durOpen ? "▴" : "▾"}</button
      >
      {#if params.length > 0 || projApplies}
        <button
          class="chip"
          class:open={valuesOpen}
          class:ovr={item.proj !== undefined}
          data-role="pl-values-toggle"
          aria-expanded={valuesOpen}
          on:click={() => (valuesOpen = !valuesOpen)}
        >
          <!-- a pattern with no controls still gets the chip when it has a
               projection to choose; "0 values" would be a lie about why -->
          {params.length === 0
            ? "Projection"
            : `${params.length} ${params.length === 1 ? "value" : "values"}`}
          {valuesOpen ? "▴" : "▾"}
        </button>
      {/if}
    </span>
    <button
      class="rm"
      data-role="pl-remove"
      title="remove"
      aria-label="remove"
      on:click={() => dispatch("remove")}>✕</button
    >
  </div>

  {#if durOpen}
    <div class="expand dur-edit" data-role="pl-duration-edit">
      <label class="ovr-toggle" title="override the playlist default for this item">
        <input
          type="checkbox"
          data-role="pl-override"
          checked={overridden}
          on:change={toggleOverride}
        />
        <span class="dim">custom duration</span>
      </label>
      {#if overridden}
        <input
          class="num"
          data-role="pl-sec"
          type="number"
          min="0"
          title="seconds (0 = manual)"
          value={item.sec}
          on:change={onSec}
        />
        <span class="dim">seconds (0 = wait for next)</span>
      {:else}
        <span class="dim">inherits the playlist default ({defaultSec || 0} s)</span>
      {/if}
    </div>
  {/if}

  {#if valuesOpen}
    <div class="expand" data-role="pl-values">
      {#if params.length > 0}
        <Controls controls={params} values={item.controls} {readouts} {hints} on:set={onSet} />
      {/if}
      <!-- quiet and advanced BY WEIGHT (§5.4d): after the pattern's own
           controls, under its own hairline, one line — not a card -->
      <ProjectionRow
        patternDims={dims}
        layout={$layout}
        override={item.proj ?? null}
        on:set={onProj}
      />
    </div>
  {/if}
</li>

<style>
  .row {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    padding: 8px 10px;
    margin-bottom: 8px;
  }

  /* the playing row is marked with a green edge, not a full-row highlight */
  .row.active {
    border-left: 3px solid #4bbd7a;
    padding-left: 8px;
  }

  .row.missing {
    opacity: 0.6;
    border-style: dashed;
  }

  .row.dragover {
    border-color: var(--accent);
    border-style: dashed;
  }

  .handle {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }

  .grip {
    cursor: grab;
    color: var(--text-dim);
    user-select: none;
    font-size: 14px;
    line-height: 1;
    width: 14px;
    text-align: center;
  }

  .row.active .grip {
    color: #4bbd7a;
    cursor: default;
  }

  .grip:active {
    cursor: grabbing;
  }

  .movers {
    display: inline-flex;
    flex-direction: column;
    gap: 1px;
  }

  .mv {
    padding: 0 3px;
    font-size: 9px;
    line-height: 1.2;
    color: var(--text-dim);
    background: transparent;
    border-color: transparent;
  }

  .mv:disabled {
    opacity: 0.25;
    cursor: default;
  }

  .miss {
    color: var(--error);
    font-weight: 400;
    font-size: 12px;
  }

  .invalid {
    color: var(--error);
    border: 1px solid var(--error);
    border-radius: 6px;
    padding: 1px 6px;
    font-size: 11px;
    white-space: nowrap;
    cursor: help;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .who {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .name {
    color: var(--text);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .type {
    font-size: 11px;
  }

  .ovr-note {
    color: var(--accent);
  }

  .chips {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .chip {
    font-size: 12px;
    padding: 3px 9px;
    border-radius: 999px;
    color: var(--text-dim);
    white-space: nowrap;
  }

  .chip.open {
    border-color: var(--accent);
    color: var(--text);
  }

  /* an OVERRIDDEN duration reads in accent, so a glance down the list says
     which items deviate from the playlist default (§5.4) */
  .chip.ovr {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  }

  .rm {
    padding: 2px 8px;
    line-height: 1;
    flex: none;
    color: var(--text-dim);
    background: transparent;
    border-color: transparent;
  }

  .rm:hover {
    color: var(--error);
    border-color: var(--error);
  }

  .expand {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .dur-edit {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 12px;
  }

  .ovr-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
  }

  .num {
    width: 64px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .dim {
    color: var(--text-dim);
  }

  /* ---- phone (D9: the playlist is the primary phone surface) ---- */
  @media (max-width: 600px) {
    .head {
      flex-wrap: wrap;
      gap: 8px;
    }

    /* the chips drop to their own line rather than squeezing the name */
    .chips {
      flex-basis: 100%;
      order: 1;
    }

    .chip,
    .rm,
    .mv {
      /* thumb-sized targets */
      min-height: 32px;
    }

    .chip {
      padding: 6px 12px;
    }

    .rm {
      padding: 6px 10px;
    }

    /* smaller thumbnails (S4b): the row is the same, the art is not */
    .head :global(.thumb canvas.sq) {
      width: 40px;
      height: 40px;
    }

    .head :global(.thumb canvas.bar) {
      width: 56px;
      height: 14px;
    }

  }
</style>
