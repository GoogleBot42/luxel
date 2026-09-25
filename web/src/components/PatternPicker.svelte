<script lang="ts">
  // THE picker (proposal §5.4, mockups S4c/S4d: "`+ Add` … opens ONE picker
  // used everywhere"). A dropdown under the button that opened it — the mock's
  // `.addwrap > .menu.full.pick` — not a modal: adding to a playlist is a list
  // operation, and the list must stay visible behind it.
  //
  // THREE SECTIONS, in this order: `Patterns` — what the device already holds,
  // which a pick queues directly — `Scenes` (Gitea #478), the layer stacks the
  // same device holds, and `Library`, the clean-room `library/` collection the
  // Patterns page browses (#538 §F). The mock draws the first two; the third
  // is the same row with the one fact a library pick needs, that it SAVES to
  // the device first. That save is the owner's work (`pages/Playlist.svelte`),
  // reported back through `busy`/`error`, because a picker should not know how
  // a device stores things.
  //
  // Every row is the same row (S4c): device-shaped 26 px thumbnail, name, and
  // ONE dim fact — a pattern's dimensionality and its projection, a scene's
  // layer count. A scene's thumbnail is its COMPOSITE (#482), so what you pick
  // looks like what you will get.
  //
  // Pattern sections are filtered by the Layout the same way the Patterns
  // page's grids are (§B, Gitea #562): a fixture is never offered a pattern it
  // cannot show. See `fits` below. Scenes need a regular 2D grid to exist at
  // all, so the section is simply empty off one.
  import { createEventDispatcher, tick } from "svelte";
  import PatternThumb from "./PatternThumb.svelte";
  import SceneThumb from "./SceneThumb.svelte";
  import { gatedFetch } from "../lib/fetchgate";
  import type { Luxel } from "../lib/luxel";
  import type { SourceLookup } from "../lib/sceneRender";
  import type { Scene } from "../lib/scene";
  import {
    captionFor,
    guessPatternDims,
    layout,
    projectionCompatible,
    type PatternDims,
  } from "../stores/geometry";

  /** The local wasm host the thumbnails render on. */
  export let luxel: Luxel | null = null;
  /** Mounted-but-closed is the normal state: the list keeps its engines. */
  export let open = false;
  /** What the picker is picking FOR — the panel's accessible name. */
  export let title = "Add to the playlist";
  /** The button the panel hangs under: exempt from the outside-click that
   *  closes it, or the click that opened it would close it again. */
  export let anchor: HTMLElement | null = null;
  /** The device's stored patterns (`stores/device.ts` `devicePatterns`), as
   *  they arrive: `source` fills in lazily and a row simply spins until it
   *  does. Passed in rather than read here so the playground can offer the
   *  local library through the same component later. */
  export let patterns: { id: string; name: string; source?: string }[] = [];
  /** The scene library (`stores/scenes.ts`), newest last — the mock's second
   *  section. Empty off a regular 2D Layout, where scenes cannot exist. */
  export let scenes: Scene[] = [];
  /** Resolves a scene layer's stored pattern to its source, for the composite
   *  thumbnails — the same lookup the Scenes page hands its tiles. */
  export let sceneLookup: SourceLookup = () => null;
  /** Name of the library pattern currently being saved to the device — the
   *  owner sets it while its `pick` handler is in flight. */
  export let busy = "";
  /** What went wrong with the last pick; the panel stays open to say it. */
  export let error = "";

  const dispatch = createEventDispatcher<{
    pick: {
      id: string;
      kind: "pattern" | "scene" | "library";
      name: string;
      source?: string;
      /** Layers, on a scene pick — the row's type line has it before the
       *  device's next read comes back. */
      layers?: number;
    };
    close: void;
  }>();

  let query = "";
  let searchEl: HTMLInputElement | undefined;
  let panelEl: HTMLElement | undefined;

  /** The generated clean-room library (`tools/gen-gallery.mjs` → gallery.json,
   *  the same file the Patterns page's Library source reads). Fetched once,
   *  on the first open, so a picker that is never opened costs nothing. */
  let library: { key: string; name: string; source: string; dims: PatternDims }[] = [];
  let libraryLoading = false;
  let libraryLoaded = false;

  async function loadLibrary(): Promise<void> {
    if (libraryLoaded || libraryLoading) return;
    libraryLoading = true;
    try {
      const r = await gatedFetch(`${import.meta.env.BASE_URL}gallery.json`);
      if (r.ok) {
        const list = (await r.json()) as { name: string; kind?: string; source: string }[];
        const seen = new Set<string>();
        library = list
          .filter((p) => {
            const k = p.name.toLowerCase();
            if (seen.has(k)) return false;
            seen.add(k);
            return true;
          })
          .map((p) => ({
            key: p.name,
            name: p.name,
            source: p.source,
            // gen-gallery's `kind` is the same advisory hint the Patterns
            // page's tiles start from (components/Gallery.svelte)
            dims: (p.kind === "grid" ? 2 : p.kind === "cloud" ? 3 : 1) as PatternDims,
          }));
      }
    } catch {
      /* gallery.json missing (a device build without it) — the section is
         simply empty, and the device section still works */
    }
    libraryLoading = false;
    libraryLoaded = true;
  }

  /** Focus the search as the panel appears — a phone keyboard opening on a
   *  list this long is the difference between typing and scrolling. It is
   *  FIRST in the panel for the same reason (S4d). */
  $: if (open) {
    void tick().then(() => searchEl?.focus());
    void loadLibrary();
  }
  // a fresh open starts from the whole list
  $: if (!open) query = "";

  /** Every row carries a LIVE thumbnail, which is a compiled wasm engine and
   *  a rAF of its own — the library is ~300 patterns, so a section shows at
   *  most this many and the search is how you reach the rest. */
  const SECTION_CAP = 40;

  interface PickItem {
    id: string;
    name: string;
    source?: string;
    /** The one dim fact the mock's row carries (`2D`, `1D · along x`, `3 layers`). */
    fact: string;
    /** Scene rows composite their record instead of compiling a source. */
    scene?: Scene;
    layers?: number;
  }

  $: needle = query.trim().toLowerCase();
  const match = (label: string, n: string): boolean =>
    n === "" || label.toLowerCase().includes(n);

  /**
   * The Layout FILTER (§B, Gitea #562), the same rule the Patterns page's
   * grids use (`components/Gallery.svelte`): a fixture is never offered a
   * pattern it cannot show, so a 1D strip sees no 2D/3D entry and a plane no
   * 3D one. Picking one would write it to the device's store and queue an
   * item the fixture cannot play.
   *
   * It bites only when the Layout is a real FIXTURE — the picker is a console
   * surface today, so that is always true here, but the gate is the Gallery's
   * so the two cannot drift.
   *
   * Dimensionality is the same advisory each grid starts from: gen-gallery's
   * `kind` for a library row, a regex guess at the source for a device one.
   */
  $: fixture = $layout.source === "device" || $layout.source === "user";
  const fits = (dims: PatternDims, ld: number, fix: boolean): boolean =>
    !fix || projectionCompatible(dims, ld);

  /** A row whose `source` has not streamed in yet is UNKNOWN, and unknown is
   *  never a "no" (.claude/rules/web.md, Gitea #730): it counts as
   *  dimensionless — 0 is "any", which every Layout accepts — so a list that
   *  is still filling offers everything rather than nothing, and each row's
   *  dim fact sharpens the moment its source lands. */
  const dimsOf = (src: string | undefined): PatternDims =>
    src === undefined ? 0 : guessPatternDims(src);

  /** S4c's dim column: the projection caption when the pattern is not native
   *  to this Layout (`1D · along x`), else its plain dimensionality (`2D`). */
  const patternFact = (dims: PatternDims): string =>
    captionFor(dims, $layout) ?? (dims > 0 ? `${dims}D` : "");

  $: deviceMatches = patterns
    .filter((p) => match(p.name || p.id, needle) && fits(dimsOf(p.source), $layout.dims, fixture))
    .map((p): PickItem => ({
      id: p.id,
      name: p.name,
      source: p.source,
      fact: patternFact(dimsOf(p.source)),
    }));
  $: sceneMatches = scenes
    // A scene with no id cannot be queued (`I S<id>`) or opened, so it is not
    // something to offer — a mirror started with `--scenes` currently reports
    // exactly that for an `S -` block (Gitea #701).
    .filter((s) => /^[0-9a-f]{8}$/.test(s.id))
    .filter((s) => match(s.name || s.id, needle))
    .map((s): PickItem => ({
      id: s.id,
      name: s.name,
      fact: `${s.layers.length} layer${s.layers.length === 1 ? "" : "s"}`,
      scene: s,
      layers: s.layers.length,
    }));
  /** A library pattern already on the device would be a duplicate row in the
   *  picker AND an overwrite on pick, so the device's copy wins. */
  $: onDevice = new Set(patterns.map((p) => (p.name || p.id).toLowerCase()));
  $: libraryMatches = library
    .filter(
      (p) =>
        !onDevice.has(p.name.toLowerCase()) &&
        match(p.name, needle) &&
        fits(p.dims, $layout.dims, fixture),
    )
    .map((p): PickItem => ({
      id: p.key,
      name: p.name,
      source: p.source,
      fact: patternFact(p.dims),
    }));

  /** One group per source, in the mock's order. */
  $: sections = [
    {
      kind: "pattern" as const,
      label: "Patterns",
      items: deviceMatches.slice(0, SECTION_CAP),
      more: Math.max(0, deviceMatches.length - SECTION_CAP),
    },
    {
      kind: "scene" as const,
      label: "Scenes",
      items: sceneMatches.slice(0, SECTION_CAP),
      more: Math.max(0, sceneMatches.length - SECTION_CAP),
    },
    {
      kind: "library" as const,
      label: "Library",
      items: libraryMatches.slice(0, SECTION_CAP),
      more: Math.max(0, libraryMatches.length - SECTION_CAP),
    },
  ];
  /** Only the groups with something in them get a label — the mock spaces
   *  the FIRST one differently from the rest. */
  $: visible = sections.filter((s) => s.items.length > 0);
  $: total = deviceMatches.length + sceneMatches.length + libraryMatches.length;

  function choose(p: PickItem, kind: "pattern" | "scene" | "library"): void {
    if (busy) return; // one save at a time — the device serves ~2 connections
    const detail: {
      id: string;
      kind: "pattern" | "scene" | "library";
      name: string;
      source?: string;
      layers?: number;
    } = { id: p.id, kind, name: p.name || p.id };
    if (p.source !== undefined) detail.source = p.source;
    if (p.layers !== undefined) detail.layers = p.layers;
    dispatch("pick", detail);
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.stopPropagation();
      dispatch("close");
    }
  }

  /** Dismissal: the panel is a dropdown, so anything outside it (except the
   *  button that opened it) closes it — `components/Popover.svelte`'s rule,
   *  hand-rolled because this panel is anchored IN the list rather than
   *  positioned off the viewport. */
  function onWindowClick(e: MouseEvent): void {
    if (!open) return;
    const t = e.target as Node;
    if (panelEl?.contains(t) || anchor?.contains(t)) return;
    dispatch("close");
  }
</script>

<svelte:window on:keydown={open ? onKey : undefined} on:click={onWindowClick} />

{#if open}
  <!-- S4c `.menu.full.pick`: the ⋯ menu's chrome, the width of the button's
       wrapper, with the search field first and one `.slabel` per section. -->
  <div
    class="menu full pick"
    bind:this={panelEl}
    data-role="pattern-picker"
    role="dialog"
    aria-label={title}
  >
    <input
      class="inp xs search"
      data-role="picker-search"
      type="search"
      placeholder="search patterns and scenes…"
      bind:this={searchEl}
      bind:value={query}
    />
    {#if busy}
      <p class="line dim" data-role="picker-busy">Saving “{busy}” to the device…</p>
    {:else if error}
      <p class="line err" data-role="picker-error">{error}</p>
    {/if}
    {#each visible as section, i (section.kind)}
      {#if section.items.length > 0}
        <div
          class="slabel sect"
          class:first={i === 0}
          data-role={`picker-section-${section.kind}`}
        >
          {section.label}
        </div>
        {#each section.items as p (p.id)}
          <button
            class="mi pk"
            data-role="picker-item"
            data-kind={section.kind}
            data-id={p.id}
            disabled={busy !== ""}
            data-reason={busy === "" ? undefined : "a pattern is being saved to the device"}
            on:click={() => choose(p, section.kind)}
          >
            {#if luxel && p.scene}
              <SceneThumb {luxel} scene={p.scene} lookup={sceneLookup} />
            {:else if luxel}
              <PatternThumb {luxel} source={p.source} />
            {/if}
            <span class="pknm">{p.name || p.id}</span>
            <span class="mdim"
              >{section.kind === "library" ? "saves to device" : p.fact}</span
            >
          </button>
        {/each}
        {#if section.more > 0}
          <p class="line dim" data-role={`picker-more-${section.kind}`}>
            + {section.more} more — narrow the search to see them
          </p>
        {/if}
      {/if}
    {/each}
    {#if libraryLoading && library.length === 0}
      <p class="line dim" data-role="picker-loading">loading the library…</p>
    {/if}
    {#if total === 0 && !libraryLoading}
      <p class="line dim" data-role="picker-empty">
        {patterns.length === 0 && library.length === 0 && scenes.length === 0
          ? "Nothing to add — save a pattern from the editor first."
          : needle === ""
            ? "Nothing here can play on this layout."
            : `Nothing matches “${query}”.`}
      </p>
    {/if}
  </div>
{/if}

<style>
  /* S4c: `.menu.full{left:0;right:0;width:auto}` inside the `+ Add` button's
     relative wrapper — the app's global `.menu` is `position:fixed` because
     `Popover` places it off the viewport; this one is placed by the list. */
  .menu.full {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 7px);
    width: auto;
    /* a 300-pattern library still has to be reachable with a thumb */
    max-height: 60vh;
    overflow-y: auto;
  }

  .search {
    width: 100%;
    margin-bottom: 2px;
  }

  /* the mock's section labels: `9px 10px 5px` for the first, `11px 10px 5px`
     for every one after it (the extra 2px is the gap between groups) */
  .sect {
    padding: 11px 10px 5px;
  }

  .sect.first {
    padding: 9px 10px 5px;
  }

  /* S4c `.mi.pk` — the menu item that carries a picture */
  .mi.pk {
    display: flex;
    align-items: center;
    gap: 9px;
    text-align: left;
  }

  /* S4c `.mi.pk canvas{flex:none}` — the picture never shrinks */
  .mi.pk :global(.thumb canvas) {
    flex: none;
  }

  .mi.pk :global(.thumb canvas.sq) {
    width: 26px;
    height: 26px;
  }

  .mi.pk :global(.thumb canvas.bar) {
    width: 26px;
    height: 12px;
  }

  .pknm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* S4c `.mdim` — the one dim fact per row */
  .mdim {
    margin-left: auto;
    font: 10px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .mi.pk[disabled] {
    opacity: 0.5;
    cursor: default;
  }

  .line {
    font-size: 12px;
    margin: 10px 0 0;
    padding: 0 10px;
  }

  .dim {
    color: var(--text-dim);
  }

  .err {
    color: var(--error);
  }
</style>
