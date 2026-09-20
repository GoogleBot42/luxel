<script lang="ts">
  // THE picker (proposal §5.4: "`+ Add` … opens ONE picker used everywhere").
  // A searchable list of what the device can play, each row carrying the same
  // device-shaped live thumbnail the Patterns page and the playlist rows use,
  // so what you pick looks like what you will get.
  //
  // TWO SECTIONS today (Gitea #538 §F): `On device` — patterns the device
  // already holds, which a pick queues directly — and `Library`, the same
  // clean-room `library/` collection the Patterns page browses. A library
  // pattern is source the device has never seen, so picking one is a SAVE
  // followed by an append; the owner does that work (`pages/Playlist.svelte`)
  // and reports it back through `busy`/`error`, because a picker should not
  // know how a device stores things.
  //
  // BOTH sections are filtered by the Layout the same way the Patterns page's
  // grids are (§B, Gitea #562): a fixture is never offered a pattern it cannot
  // show. See `fits` below.
  //
  // Scenes are Phase B (#478/#481) and slot in as a third section, not a
  // second component: `sections` below is the seam — give it a
  // `{ kind: "scene", … }` group and the markup, the search, the keyboard
  // handling and the `pick` event all work unchanged.
  //
  // Exported for reuse: the editor's and the Patterns tile's ⋯ menus
  // ("Add to playlist", "Add to scene ▸") are the same choice made from a
  // different place (§5.4b).
  import { createEventDispatcher, tick } from "svelte";
  import PatternThumb from "./PatternThumb.svelte";
  import { gatedFetch } from "../lib/fetchgate";
  import type { Luxel } from "../lib/luxel";
  import {
    guessPatternDims,
    layout,
    projectionCompatible,
    type PatternDims,
  } from "../stores/geometry";

  /** The local wasm host the thumbnails render on. */
  export let luxel: Luxel | null = null;
  /** Mounted-but-closed is the normal state: the list keeps its engines. */
  export let open = false;
  /** What the picker is picking FOR — the panel's heading. */
  export let title = "Add to the playlist";
  /** The device's stored patterns (`stores/device.ts` `devicePatterns`), as
   *  they arrive: `source` fills in lazily and a row simply spins until it
   *  does. Passed in rather than read here so the playground can offer the
   *  local library through the same component later. */
  export let patterns: { id: string; name: string; source?: string }[] = [];
  /** Name of the library pattern currently being saved to the device — the
   *  owner sets it while its `pick` handler is in flight. */
  export let busy = "";
  /** What went wrong with the last pick; the panel stays open to say it. */
  export let error = "";

  const dispatch = createEventDispatcher<{
    pick: { id: string; kind: "pattern" | "scene" | "library"; name: string; source?: string };
    close: void;
  }>();

  let query = "";
  let searchEl: HTMLInputElement | undefined;

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
   *  list this long is the difference between typing and scrolling. */
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
   * A device pattern whose source has not streamed in yet counts as 1D (it is
   * shown), exactly as a Gallery tile does.
   */
  $: fixture = $layout.source === "device" || $layout.source === "user";
  const fits = (dims: PatternDims, ld: number, fix: boolean): boolean =>
    !fix || projectionCompatible(dims, ld);

  $: deviceMatches = patterns
    .filter(
      (p) =>
        match(p.name || p.id, needle) &&
        fits(p.source === undefined ? 1 : guessPatternDims(p.source), $layout.dims, fixture),
    )
    .map((p): PickItem => ({ id: p.id, name: p.name, source: p.source }));
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
    .map((p): PickItem => ({ id: p.key, name: p.name, source: p.source }));

  /** One group per source. Phase B appends `{ kind: "scene", … }` here. */
  $: sections = [
    {
      kind: "pattern" as const,
      label: "On device",
      items: deviceMatches.slice(0, SECTION_CAP),
      more: Math.max(0, deviceMatches.length - SECTION_CAP),
    },
    {
      kind: "library" as const,
      label: "Library",
      items: libraryMatches.slice(0, SECTION_CAP),
      more: Math.max(0, libraryMatches.length - SECTION_CAP),
    },
  ];
  $: total = deviceMatches.length + libraryMatches.length;

  function choose(
    id: string,
    kind: "pattern" | "scene" | "library",
    name: string,
    source?: string,
  ): void {
    if (busy) return; // one save at a time — the device serves ~2 connections
    dispatch("pick", { id, kind, name, source });
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.stopPropagation();
      dispatch("close");
    }
  }
</script>

<svelte:window on:keydown={open ? onKey : undefined} />

{#if open}
  <!-- the backdrop is a click target, not a control: the panel below owns
       every keyboard affordance (search field, rows, close button) -->
  <div
    class="backdrop"
    data-role="picker-backdrop"
    on:click={() => dispatch("close")}
    on:keydown={onKey}
    role="presentation"
  ></div>
  <div class="panel" data-role="pattern-picker" role="dialog" aria-label={title}>
    <div class="head">
      <span class="title">{title}</span>
      <span class="spacer"></span>
      <button
        class="btn icon quiet"
        data-role="picker-close"
        aria-label="close"
        on:click={() => dispatch("close")}>✕</button
      >
    </div>
    <input
      class="inp search"
      data-role="picker-search"
      type="search"
      placeholder="Search patterns…"
      bind:this={searchEl}
      bind:value={query}
    />
    {#if busy}
      <p class="line dim" data-role="picker-busy">Saving “{busy}” to the device…</p>
    {:else if error}
      <p class="line err" data-role="picker-error">{error}</p>
    {/if}
    <div class="body">
      {#each sections as section (section.kind)}
        {#if section.items.length > 0}
          <div class="slabel section-label" data-role={`picker-section-${section.kind}`}>
            {section.label}
          </div>
          <ul class="list">
            {#each section.items as p (p.id)}
              <li>
                <button
                  class="item"
                  data-role="picker-item"
                  data-kind={section.kind}
                  data-id={p.id}
                  disabled={busy !== ""}
                  data-reason={busy === "" ? undefined : "a pattern is being saved to the device"}
                  on:click={() => choose(p.id, section.kind, p.name || p.id, p.source)}
                >
                  {#if luxel}<PatternThumb {luxel} source={p.source} />{/if}
                  <span class="name">{p.name || p.id}</span>
                  {#if section.kind === "library"}<span class="tag">saves to device</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
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
          {patterns.length === 0 && library.length === 0
            ? "Nothing to add — save a pattern from the editor first."
            : needle === ""
              ? "Nothing here can play on this layout."
              : `Nothing matches “${query}”.`}
        </p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 40;
  }

  .panel {
    position: fixed;
    z-index: 41;
    top: 10vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(520px, calc(100vw - 24px));
    max-height: 76vh;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.55);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .title {
    font-weight: 600;
  }

  .spacer {
    flex: 1;
  }

  .search {
    width: 100%;
  }

  .body {
    overflow-y: auto;
    min-height: 0;
  }

  .section-label {
    margin: 8px 0 6px;
  }

  .section-label:first-child {
    margin-top: 0;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    /* thumb-sized target on a phone (D9) */
    min-height: 48px;
    padding: 6px 8px;
    text-align: left;
    background: transparent;
    border-color: transparent;
  }

  .item:hover {
    background: var(--bg-inset);
    border-color: var(--accent);
  }

  .item[disabled] {
    opacity: 0.5;
    cursor: default;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* says what picking a LIBRARY row costs before you pick it */
  .tag {
    flex: none;
    font: 11px/1 var(--mono);
    color: var(--text-dim);
  }

  .line {
    font-size: 12px;
    margin: 0;
  }

  .dim {
    color: var(--text-dim);
  }

  .err {
    color: var(--error);
  }

  /* the phone is the primary playlist surface (D9): the sheet fills the
     bottom of the screen rather than floating in the middle of it */
  @media (max-width: 600px) {
    .panel {
      top: auto;
      bottom: 0;
      left: 0;
      transform: none;
      width: 100vw;
      max-height: 82vh;
      border-radius: 12px 12px 0 0;
    }
  }
</style>
