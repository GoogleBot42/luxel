<script lang="ts">
  // One playlist entry (proposal §5.4, mockups S4/S4b):
  //
  //   ⠿ handle · 44px device-shaped thumbnail · name + type ·
  //   `8 s` chip · `N values ▾` chip · ✕
  //
  // REORDERING IS THE HANDLE. S4 has no ↑/↓ movers, so they are gone; the
  // handle is a real focusable control and ↑/↓ on it move the item, which is
  // the keyboard (and screen-reader) path those buttons used to carry.
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

  /** The handle's keyboard reorder — what the ↑/↓ mover buttons used to do. */
  function onHandleKey(e: KeyboardEvent): void {
    const dir = e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
    if (dir === 0) return;
    if ((dir === -1 && first) || (dir === 1 && last)) return;
    e.preventDefault();
    dispatch("move", dir);
  }
</script>

<li
  class="plrow"
  class:playing={active}
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
    <!-- S4 `.hnd` — `⠿` normally, a green `▶` on the playing row. It is the
         ONE reorder affordance: drag with a mouse, ↑/↓ with a keyboard. -->
    <span
      class="hnd"
      class:ok={active}
      data-role="pl-grip"
      title="drag to reorder — or focus and press ↑/↓"
      role="button"
      tabindex="0"
      aria-label="reorder this item"
      draggable="true"
      on:dragstart={() => dispatch("dragstart")}
      on:keydown={onHandleKey}>{active ? "▶" : "⠿"}</span
    >
    {#if luxel && !missing}<PatternThumb {luxel} {source} proj={item.proj ?? null} />{/if}
    <span class="who">
      <span class="n" data-role="pl-name">
        {item.name || item.id}{#if missing}<span class="miss"> (deleted)</span>{/if}
      </span>
      <span class="t">
        {kindLabel}{#if projApplies && item.proj}<span class="ovr-note">· projected</span>{/if}
        <!-- S4b folds the duration into this line; the chip beside it is
             hidden at that width, so this stays the way to open it -->
        <button
          class="durline"
          class:ovr={overridden}
          data-role="pl-duration-inline"
          aria-expanded={durOpen}
          on:click={() => (durOpen = !durOpen)}>· {durationLabel}</button
        >
      </span>
    </span>
    {#if item.invalid}
      <span class="invalid" data-role="pl-invalid" title={item.invalid}>⚠ won't run</span>
    {/if}
    <button
      class="chip dur"
      class:ovr={overridden}
      class:open={durOpen}
      data-role="pl-duration"
      title="how long this item plays"
      aria-expanded={durOpen}
      on:click={() => (durOpen = !durOpen)}>{durationLabel}</button
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
        {#if params.length === 0}
          Projection
        {:else}
          {params.length}<span class="vword">
            {params.length === 1 ? "value" : "values"}</span
          >
        {/if}
        {valuesOpen ? "▴" : "▾"}
      </button>
    {/if}
    <button
      class="btn icon quiet rm"
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
          class="inp num xs"
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
    <div class="expand" class:bare={params.length === 0} data-role="pl-values">
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
  /* S4 `.plrow` */
  .plrow {
    padding: 10px 12px;
    margin-bottom: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-panel);
  }

  /* the playing row is marked with a green edge, not a full-row highlight */
  .plrow.playing {
    border-left: 3px solid var(--ok);
    padding-left: 10px;
  }

  .plrow.missing {
    opacity: 0.6;
    border-style: dashed;
  }

  .plrow.dragover {
    border-color: var(--accent);
    border-style: dashed;
  }

  /* S4 `.hnd` */
  .hnd {
    flex: none;
    width: 14px;
    text-align: center;
    color: #555c6b;
    font-size: 14px;
    line-height: 1;
    cursor: grab;
    user-select: none;
  }

  .hnd.ok {
    color: var(--ok);
    font-size: 11px;
    cursor: default;
  }

  .hnd:active {
    cursor: grabbing;
  }

  .hnd:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
    border-radius: 3px;
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

  /* S4 row: handle · thumb · who · chips · ✕, 12px apart */
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  /* S4 `.plrow canvas{width:44px;height:44px}` — the thumbnail component owns
     its engine and its default sizes, so the ROW states the size it wants */
  .head :global(.thumb canvas.sq) {
    width: 44px;
    height: 44px;
  }

  .head :global(.thumb canvas.bar) {
    width: 66px;
    height: 16px;
  }

  /* S4 `.who` */
  .who {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .n {
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .t {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 3px;
    font: 11px/1 var(--mono);
    color: var(--text-dim);
  }

  .ovr-note {
    color: var(--accent);
  }

  /* the phone's duration affordance: plain text on the subtitle line (S4b) */
  .durline {
    display: none;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
  }

  .durline.ovr {
    color: var(--accent);
  }

  /* S4 `.chip` — 26px, 6px radius, mono; NOT a pill */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    height: 26px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-inset);
    color: var(--text-dim);
    font: 12px/1 var(--mono);
    white-space: nowrap;
  }

  .chip.open {
    color: var(--text);
    background: #1c2029;
    border-color: #3a4150;
  }

  /* an OVERRIDDEN duration reads in accent, so a glance down the list says
     which items deviate from the playlist default (§5.4) */
  .chip.ovr {
    color: var(--accent);
    border-color: rgba(232, 163, 61, 0.45);
  }

  /* S4 `.btn.icon.quiet` — the ✕ has no outline until it is hovered */
  .rm {
    flex: none;
  }

  .rm:hover {
    color: var(--error);
  }

  /* S4 `.plvals`: the opened panel is a darker band filling the bottom of the
     row, not an indented block inside it */
  .expand {
    margin: 10px -12px -10px;
    padding: 2px 12px 12px;
    border-top: 1px solid var(--border);
    border-radius: 0 0 7px 7px;
    background: rgba(0, 0, 0, 0.2);
  }

  /* a pattern with no controls opens straight onto ProjectionRow, which
     draws its own hairline — two of them 10 px apart is the tell */
  .expand.bare {
    padding-top: 0;
    border-top: none;
  }

  .dur-edit {
    padding-top: 10px;
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

  .dim {
    color: var(--text-dim);
  }

  /* ---- phone (S4b: the playlist is the primary phone surface) ---- */
  @media (max-width: 600px) {
    .head {
      gap: 10px;
    }

    /* S4b: no duration chip — the duration moves onto the subtitle line, and
       the values chip keeps only its count (`3 ▾`) */
    .chip.dur {
      display: none;
    }

    .durline {
      display: inline;
    }

    .vword {
      display: none;
    }

    .chip,
    .rm {
      /* thumb-sized targets */
      min-height: 32px;
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
