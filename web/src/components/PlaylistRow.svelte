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
  // The DRAG is pointer-driven and LIVE (Gitea #538 round 2): the grabbed row
  // lifts and follows the pointer, the rows it passes slide out of its way,
  // and only the release changes the order. The row owns none of that
  // arithmetic — `pages/Playlist.svelte` measures the list and hands each row
  // its `shift` — because a row cannot see its neighbours. All it does is
  // paint the two transient states (`lifted`, `shift`) on the two elements it
  // emits, and hand the page the elements to measure (`els()`).
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
  /** THIS row is the one being dragged: it lifts out of the list and follows
   *  the pointer. Set by the page for exactly one row at a time. */
  export let lifted = false;
  /** Vertical offset in px. On the lifted row it is the pointer's travel; on
   *  every other row it is the hole the page is opening for the drop. */
  export let shift = 0;
  /** Animate `shift` changes (~120 ms). Off while the lifted row tracks the
   *  pointer, and off for the one flush in which the release re-renders the
   *  list in its new order — a transition there would slide every row back
   *  from the offset it no longer has. */
  export let anim = false;

  const dispatch = createEventDispatcher<{
    change: void;
    /** A single control the user just moved — the page pushes it live when
     *  this row is the one playing, so the fixture follows the slider. */
    control: { name: string; values: number[] };
    remove: void;
    move: number;
    /** The handle was grabbed — the page takes it from here (it owns the
     *  window listeners, the measurements and the drop). */
    grab: PointerEvent;
  }>();

  let rowEl: HTMLLIElement;
  let bandEl: HTMLLIElement | undefined;

  /** The element(s) this row occupies, top to bottom — the row and, when a
   *  chip is open, the `.plvals` band that is its SIBLING. The page measures
   *  these to size the hole a drag opens. */
  export function els(): HTMLElement[] {
    return bandEl ? [rowEl, bandEl] : [rowEl];
  }

  // One transform for both elements. `null` removes the property entirely, so
  // a resting row carries no inline style at all and the mock-verified
  // computed styles are untouched (mockdiff S4/S4b).
  $: xform = lifted
    ? `translateY(${shift}px) scale(1.012)`
    : shift
      ? `translateY(${shift}px)`
      : null;
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
      dims = e.patternDims(); // DECLARED: 0 = dimensionless, no projection
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

  // ---- the values chip's label (S4 `3 values ▴` / S4b `3 ▴`) ----
  //
  // Built as ONE string per width rather than assembled from spans: the mock's
  // `.chip` is a 5px-gap flex box whose content is a single text run, so every
  // extra child element would add a gap the mock does not have and make the
  // chip wider than it draws.
  $: chipArrow = valuesOpen ? "▴" : "▾";
  // a pattern with no controls still gets the chip when it has a projection to
  // choose; "0 values" would be a lie about why
  $: chipLabel =
    params.length === 0
      ? `Projection ${chipArrow}`
      : `${params.length} ${params.length === 1 ? "value" : "values"} ${chipArrow}`;
  $: chipLabelShort = params.length === 0 ? `Proj ${chipArrow}` : `${params.length} ${chipArrow}`;

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
  class:lifted
  class:anim
  bind:this={rowEl}
  style:transform={xform}
  data-role="playlist-item"
  data-lifted={lifted ? "1" : null}
>
  <!-- S4 `.hnd` — `⠿` normally, a green `▶` on the playing row. It is the
       ONE reorder affordance: drag with a pointer (mouse OR touch — the
       handle is the only element that opts out of touch scrolling), ↑/↓ with
       a keyboard. -->
  <span
    class="hnd"
    class:ok={active}
    data-role="pl-grip"
    title="drag to reorder — or focus and press ↑/↓"
    role="button"
    tabindex="0"
    aria-label="reorder this item"
    on:pointerdown={(e) => dispatch("grab", e)}
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
           hidden at that width, so this stays the way to open it. It PAINTS
           as the mock's 11px type and TAPS at 24px: the height is real and
           the negative block margin keeps it out of the line's own height. -->
      <button
        class="durline"
        class:ovr={overridden}
        data-role="pl-duration-inline"
        aria-expanded={durOpen}
        aria-label="duration — {durationLabel}"
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
      <!-- ONE text run per width: S4's chip is `3 values ▴`, S4b's is `3 ▴`.
           Two spans toggled by the media query rather than one span nested
           inside the label, because `.chip` is a 5px-gap flex box and every
           extra child would widen it past the mock's `.chip`. -->
      <span class="lbl wide">{chipLabel}</span>
      <span class="lbl narrow">{chipLabelShort}</span>
    </button>
  {/if}
  <button
    class="btn icon quiet rm"
    data-role="pl-remove"
    title="remove"
    aria-label="remove"
    on:click={() => dispatch("remove")}>✕</button
  >
</li>

<!-- S4 `.plvals`: a band of its own UNDER the row, not a block inside it —
     the row is the mock's flex line and nothing else lives in it. It stays an
     `<li>` so the list holds only list items; `display:block` is what the mock
     computes. ONE band for both chips (the mock only draws the values one), so
     `pl-values` is the band's role whichever chip opened it. -->
{#if durOpen || valuesOpen}
  <li
    class="plvals"
    class:lifted
    class:anim
    bind:this={bandEl}
    style:transform={xform}
    data-role="pl-values"
  >
    {#if durOpen}
      <div class="dur-edit" data-role="pl-duration-edit">
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
    {/if}
  </li>
{/if}

<style>
  /* S4 `.plrow` — the ROW IS the flex line (handle · thumb · who · chips · ✕,
     12px apart). Nothing else lives inside it: the opened values band is the
     `.plvals` sibling below, exactly as the mock draws it. */
  .plrow {
    display: flex;
    align-items: center;
    gap: 12px;
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

  /* ---- the drag's two TRANSIENT states (#538 round 2) ----
     Neither exists at rest, so the mock-verified resting row is untouched:
     `.anim` only adds a transition, and `.lifted` only paints while a pointer
     is down on a handle. The mocks have no dragging frame to match. */

  /* the rows sliding out of the way — the FLIP half. `transform` alone, so
     nothing reflows while the pointer moves. */
  .plrow.anim,
  .plvals.anim {
    transition: transform 120ms ease;
  }

  /* the grabbed row: out of the list's plane, following the pointer */
  .plrow.lifted {
    position: relative;
    z-index: 5;
    border-color: #3a4150;
    box-shadow: 0 10px 22px rgba(0, 0, 0, 0.5);
    cursor: grabbing;
  }

  /* an open values band travels with the row it belongs to */
  .plvals.lifted {
    position: relative;
    z-index: 5;
  }

  /* the lifted row's own transform tracks the pointer with no easing; the
     RETURN (Escape, or a drop back where it started) animates like the rest */
  .plrow.lifted.anim,
  .plvals.lifted.anim {
    transition: transform 120ms ease;
  }

  @media (prefers-reduced-motion: reduce) {
    .plrow.anim,
    .plvals.anim {
      transition: none;
    }
  }

  /* S4 `.hnd` — no line-height of its own: the mock's handle is one line of
     the inherited 1.45, which is what makes it 20px tall beside a 44px thumb */
  .hnd {
    flex: none;
    width: 14px;
    text-align: center;
    color: #555c6b;
    font-size: 14px;
    cursor: grab;
    user-select: none;
    /* the ONE element that opts out of touch scrolling, so a drag from the
       handle reorders and a drag anywhere else still scrolls the list */
    touch-action: none;
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

  /* S4 `.plrow canvas{flex:none}` — the thumbnail component owns its engine
     and its default sizes, so the ROW states the size it wants */
  .plrow :global(.thumb canvas) {
    flex: none;
  }

  .plrow :global(.thumb canvas.sq) {
    width: 44px;
    height: 44px;
  }

  .plrow :global(.thumb canvas.bar) {
    width: 66px;
    height: 16px;
  }

  /* S4 `.who` — a plain block, not a flex column: the name and the type line
     are two block children of it */
  .who {
    flex: 1;
    min-width: 0;
  }

  .n {
    display: block;
    font-size: 13px;
    color: var(--text);
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

  /* the phone's duration affordance: plain 11px type on the subtitle line
     (S4b). 24px tall so it is a real tap target, with the extra height taken
     back in margin so the subtitle line stays the 11px the mock draws. */
  .durline {
    display: none;
    align-items: center;
    height: 24px;
    margin-block: -6.5px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
  }

  .durline.ovr {
    color: var(--accent);
  }

  .durline:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
  }

  /* S4 `.chip` — 26px, 6px radius, mono; NOT a pill. No `flex:none`: the
     mock's chips are ordinary flex items that may shrink. */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-inset);
    color: var(--text-dim);
    font: 12px/1 var(--mono);
    white-space: nowrap;
  }

  /* one label per width — see the markup */
  .lbl.narrow {
    display: none;
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

  /* S4 `.btn.icon.quiet` — transparent fill, the shared border kept */
  .rm:hover {
    color: var(--error);
  }

  /* S4 `.plvals`: a band UNDER the row — same width, tucked up under its
     bottom edge, no top border, bottom corners only */
  .plvals {
    display: block;
    padding: 2px 12px 12px;
    margin: -4px 0 8px;
    border: 1px solid var(--border);
    border-top: 0;
    border-radius: 0 0 8px 8px;
    background: rgba(0, 0, 0, 0.2);
  }

  /* mockups `.plvals .ctlrow`: the band is narrower than the editor rail, so
     the playlist states its own columns (70/1fr/74) and rhythm (10px) rather
     than inheriting the rail's 82px/auto/12px */
  .plvals :global(.control) {
    grid-template-columns: 70px minmax(0, 1fr) 74px;
    margin-top: 10px;
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
    .plrow {
      gap: 10px;
    }

    /* S4b: no duration chip — the duration moves onto the subtitle line, and
       the values chip keeps only its count (`3 ▾`) */
    .chip.dur {
      display: none;
    }

    .durline {
      display: inline-flex;
    }

    .lbl.wide {
      display: none;
    }

    .lbl.narrow {
      display: inline;
    }

    /* smaller thumbnails (S4b): the row is the same, the art is not */
    .plrow :global(.thumb canvas.sq) {
      width: 40px;
      height: 40px;
    }

    .plrow :global(.thumb canvas.bar) {
      width: 56px;
      height: 14px;
    }
  }
</style>
