<script lang="ts">
  // The Patterns page (proposal §5.1, D3, mockups S1/S1b/S1c) — ONE browser
  // for every source of patterns, replacing the old "Patterns Library",
  // "PixelBlaze Library" and "Device Patterns" tabs, which looked like three
  // apps over the same thing.
  //
  //   console:     On device (N) | Library (N) [| PixelBlaze Library (N)]
  //   playground:  Library (N)   | Mine (N)    [| PixelBlaze Library (N)]
  //
  // One search box, one tile grid per source (all mounted, only the chosen
  // one shown, so switching sources does not throw away compiled engines).
  // Tiles take the Layout's shape and carry the projection caption — that is
  // `components/Gallery.svelte`'s job; this page owns the source control and
  // the per-tile verbs (Play · Edit · ⋯).
  //
  // Since #538 the Layout also FILTERS: a fixture never offers a pattern it
  // cannot show (a strip hides every 2D/3D pattern, a plane every 3D one), so
  // each source's chip counts what is actually on screen. `On device` is the
  // exception — the user's own stored patterns do not silently vanish, they
  // drop into a collapsed "Not for this layout (N)" group under the grid,
  // drawn in their own shape and with no Play verb. The heading is the whole
  // explanation; Jeremy asked for no prose about the rule.
  import { createEventDispatcher } from "svelte";
  import Gallery, { type GalleryItem } from "../components/Gallery.svelte";
  import Popover from "../components/Popover.svelte";
  import {
    addToPlaylist as addPatternToPlaylist,
    device,
    deviceError,
    devicePatterns,
    isPlayground,
    refreshDevicePatterns,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { note } from "../stores/notify";
  import {
    compileToBytecode,
    devicePatternId,
    luxel,
    saved,
    saveToLocalLibrary,
  } from "../stores/pattern";

  /** The tab is the visible one (drives the lazy gallery mount). */
  export let active = false;
  /** The scraped-corpus source exists only when tools/gen-corpus-gallery.mjs
   *  found a populated corpus/ (dev-only, as today — see App.svelte's probe). */
  export let hasPixelblazeLibrary = false;

  const dispatch = createEventDispatcher<{
    /** open a library/corpus pattern in the editor */
    pick: { name: string; source: string };
    /** open one of this browser's saved patterns in the editor (by name) */
    openSaved: string;
    /** open a device pattern in the editor (by id) */
    openDevice: string;
    /** activate a device pattern WITHOUT opening the editor (by id) */
    playDevice: string;
    new: void;
  }>();

  type SourceId = "device" | "library" | "mine" | "pixelblaze";

  let search = "";
  /** Lazy-mount the grids on first visit, then keep them alive so their
   *  compiled tile engines persist across tab switches. */
  let mounted = false;
  $: if (active) mounted = true;

  // counts for the segment chips: the generated sources report their own
  let libraryCount = 0;
  let libraryLoading = true;
  let libraryNote = "";
  let corpusCount = 0;
  let corpusLoading = true;
  let corpusNote = "";
  let deviceNote = "";
  let mineNote = "";
  /** What each source SHOWS on the current Layout — the segment chip's number
   *  is the visible set, so it never promises patterns the filter removed. */
  let deviceCount = 0;
  let mineCount = 0;
  /** The device patterns this Layout cannot show (§B, Jeremy): a second,
   *  collapsed group under the grid rather than a silent disappearance —
   *  they are the user's own, stored on their own hardware. */
  let deviceIncompatible = 0;
  let showIncompatible = false;

  $: sources = [
    {
      id: "device" as const,
      label: "On device",
      show: !$isPlayground,
      count: deviceCount,
    },
    { id: "library" as const, label: "Library", show: true, count: libraryCount },
    { id: "mine" as const, label: "Mine", show: $isPlayground, count: mineCount },
    {
      id: "pixelblaze" as const,
      label: "PixelBlaze Library",
      show: hasPixelblazeLibrary,
      count: corpusCount,
    },
  ].filter((s) => s.show);

  /** Which source the grid is showing. A console opens on `On device` and a
   *  playground on `Library` (§4) — and since the app boots as a playground
   *  and only then discovers a device, the pick is re-defaulted when the mode
   *  itself changes, not just on first render. */
  let sourceId: SourceId = "library";
  let pickedFor: "console" | "playground" | null = null;
  $: {
    const m = $isPlayground ? "playground" : "console";
    if (m !== pickedFor) {
      pickedFor = m;
      sourceId = m === "console" ? "device" : "library";
    }
  }
  /** …and fall back to the first available one if it is not on offer. */
  $: if (!sources.some((s) => s.id === sourceId)) sourceId = sources[0]?.id ?? "library";

  /** The device's stored library, as tiles. `source` streams in behind the
   *  list (stores/device.ts fetches one at a time), and a tile whose source
   *  has not arrived yet just spins. */
  $: deviceItems = $devicePatterns.map(
    (p): GalleryItem => ({ key: p.id, name: p.name, source: p.source }),
  );
  /** This browser's saved patterns (the playground's "Mine"). */
  $: mineItems = $saved.map((s): GalleryItem => ({ key: s.name, name: s.name, source: s.source }));

  /** What the "open this in the editor" verb is called. The console edits
   *  patterns it can also play; the playground only opens them (§5.7). */
  $: openVerb = $isPlayground ? "Open" : "Edit";

  $: loading =
    sourceId === "library" ? libraryLoading : sourceId === "pixelblaze" ? corpusLoading : false;
  /** The active source's "nothing here" line, shown beside its count — but
   *  never on an unreachable device, where the empty list is a symptom and
   *  the `device-offline` hint above says what actually happened. */
  $: sourceNote =
    sourceId === "library"
      ? libraryNote
      : sourceId === "pixelblaze"
        ? corpusNote
        : sourceId === "mine"
          ? mineNote
          : $device
            ? deviceNote
            : "";

  function selectSource(id: SourceId): void {
    sourceId = id;
    if (id === "device") void refreshDevicePatterns();
  }

  // ---- the tile ⋯ menu (§5.4b) ----
  // One popup for the whole page, positioned from the button that opened it:
  // a menu inside the scrolling tile grid would be clipped by it.

  let menu: { item: GalleryItem; anchor: HTMLElement } | null = null;

  function openMenu(e: MouseEvent, item: GalleryItem): void {
    menu = { item, anchor: e.currentTarget as HTMLElement };
  }

  function closeMenu(): void {
    menu = null;
  }

  // ---- tile verbs ----

  /** Append a device pattern to the playlist with the pattern's OWN defaults:
   *  an item with no control overrides is exactly that (the device applies the
   *  stored `//#` defaults). Tuned values come from the editor's ⋯, which has
   *  live sliders to capture — a tile has none. */
  function addToPlaylist(item: GalleryItem): void {
    closeMenu();
    if (!$device) return;
    // the ONE path every "Add to playlist" affordance takes (#470) — no
    // values, so the item runs the pattern's own defaults
    addPatternToPlaylist(item.key);
    note("save", "added to playlist", 2000);
  }

  /** "<name> copy", "<name> copy 2", … — whichever is free in `taken`. */
  function copyName(name: string, taken: string[]): string {
    const base = `${name} copy`;
    if (!taken.includes(base)) return base;
    for (let n = 2; ; n++) if (!taken.includes(`${base} ${n}`)) return `${base} ${n}`;
  }

  async function duplicate(item: GalleryItem): Promise<void> {
    closeMenu();
    if (item.source === undefined) return; // still streaming in
    if (sourceId === "mine") {
      const name = copyName(item.name, $saved.map((s) => s.name));
      saveToLocalLibrary(name, item.source);
      note("save", "duplicated", 2000);
      return;
    }
    const d = $device;
    if (!d) return;
    const bc = compileToBytecode(item.source);
    if (!bc) {
      note("save", "duplicate failed: pattern does not compile", 3000);
      return;
    }
    const name = copyName(item.name, $devicePatterns.map((p) => p.name));
    const r = await d.savePattern(name, item.source, bc);
    if (!r.ok) {
      note("save", "error" in r ? `duplicate failed: ${r.error}` : "duplicate failed", 3000);
      return;
    }
    note("save", "duplicated on the device", 2000);
    await refreshDevicePatterns();
  }

  async function removeFromDevice(item: GalleryItem): Promise<void> {
    closeMenu();
    const d = $device;
    if (!d) return;
    const ok = await confirm({
      title: "Delete pattern from the device?",
      body: `"${item.name}" is removed from the device's stored library. This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
    });
    if (!ok) return;
    await d.deletePattern(item.key);
    if ($devicePatternId === item.key) devicePatternId.set("");
    note("save", "deleted from device", 2000);
    await refreshDevicePatterns();
  }
</script>

<!-- dismissal is the popover's (components/Popover.svelte) -->

<div class="patterns-tab" data-role="patterns-panel" hidden={!active}>
  <div class="pagebar">
    <!-- the source control: one page, N sources (D3) — not N tabs -->
    <div class="seg" data-role="patterns-sources" role="tablist">
      {#each sources as s (s.id)}
        <button
          class="segbtn"
          class:on={sourceId === s.id}
          data-role={`patterns-source-${s.id}`}
          role="tab"
          aria-selected={sourceId === s.id}
          on:click={() => selectSource(s.id)}
        >
          {s.label}
          <span class="ct">{s.count}</span>
        </button>
      {/each}
    </div>
    <input
      class="inp search"
      data-role="gallery-search"
      type="search"
      placeholder="search patterns…"
      bind:value={search}
    />
    <span class="spacer"></span>
    <!-- S1: the ONE primary action of the page, and the only thing right of
         the search. The count lives in the segment chip (`Library 307`). -->
    {#if sourceId !== "pixelblaze"}
      <button class="btn primary new" data-role="new-pattern" on:click={() => dispatch("new")}>
        <span class="newfull">+ New pattern</span>
        <span class="newicon" aria-hidden="true">+</span>
      </button>
    {/if}
  </div>

  {#if !$isPlayground && !$device && sourceId === "device"}
    <p class="dim hint" data-role="device-offline">
      device unreachable — {$deviceError || "reload to retry"}.
    </p>
  {/if}
  <!-- the count moved into the chip, but "still loading" and "nothing here"
       are news, and stay a line of their own under the bar -->
  {#if loading}
    <p class="dim hint" data-role="gallery-loading">
      <span class="spinner" aria-hidden="true"></span> loading patterns…
    </p>
  {:else if sourceNote}
    <p class="dim hint" data-role="gallery-note">{sourceNote}</p>
  {/if}

  <div class="grids">
    {#if mounted && $luxel}
      {#if !$isPlayground}
        <!-- On device: the running pattern wears the ring + pill, and a tile's
             own click plays it (S1). -->
        <div
          class="grid"
          data-role="patterns-grid"
          data-source="device"
          hidden={sourceId !== "device"}
        >
          <Gallery
            luxel={$luxel}
            items={deviceItems}
            {search}
            only="compatible"
            playingKey={$devicePatternId}
            emptyNote="no patterns stored on the device yet — “+ New pattern” makes one"
            bind:count={deviceCount}
            bind:note={deviceNote}
            on:pick={(e) => dispatch("playDevice", e.detail.key)}
          >
            <svelte:fragment slot="actions" let:item let:dead>
              <!-- §5.7: nothing to PLAY when the pattern does not compile, so
                   Play is absent (the tile says why). Edit and ⋯ stay — a
                   broken pattern of your own must still be fixable and
                   deletable (Gitea #529). -->
              {#if !dead}
                <button
                  class="btn sm"
                  data-role="tile-play"
                  on:click|stopPropagation={() => dispatch("playDevice", item.key)}>▶ Play</button
                >
              {/if}
              <button
                class="btn sm"
                data-role="tile-edit"
                on:click|stopPropagation={() => dispatch("openDevice", item.key)}>Edit</button
              >
              <button
                class="btn sm icon"
                data-role="tile-menu"
                title="more actions"
                on:click|stopPropagation={(e) => openMenu(e, item)}>⋯</button
              >
            </svelte:fragment>
            <svelte:fragment slot="meta" let:item>
              <button
                class="elink"
                data-role="tile-edit-link"
                on:click|stopPropagation={() => dispatch("openDevice", item.key)}>Edit</button
              >
            </svelte:fragment>
          </Gallery>

          <!-- Patterns this fixture cannot show (§B): not hidden outright the
               way a library pattern is — these are stored on the user's own
               hardware, so they sit in a collapsed group, drawn in the
               playground's "Auto" style (their own shape, not the device's)
               and with no Play verb, since playing them is the thing the
               rule forbids. The heading is the whole explanation. -->
          <div class="incompat" data-role="patterns-incompatible" hidden={deviceIncompatible === 0}>
            <button
              class="slabel disc"
              data-role="patterns-incompatible-toggle"
              aria-expanded={showIncompatible}
              on:click={() => (showIncompatible = !showIncompatible)}
            >
              <span class="caret" class:open={showIncompatible} aria-hidden="true">▸</span>
              Not for this layout ({deviceIncompatible})
            </button>
            <div hidden={!showIncompatible}>
              <Gallery
                luxel={$luxel}
                items={deviceItems}
                {search}
                only="incompatible"
                autoStyle
                playingKey={$devicePatternId}
                bind:count={deviceIncompatible}
                on:pick={(e) => dispatch("openDevice", e.detail.key)}
              >
                <svelte:fragment slot="actions" let:item let:dead>
                  {#if !dead}
                    <button
                      class="btn sm"
                      data-role="tile-edit"
                      on:click|stopPropagation={() => dispatch("openDevice", item.key)}>Edit</button
                    >
                  {/if}
                  <button
                    class="btn sm icon"
                    data-role="tile-menu"
                    title="more actions"
                    on:click|stopPropagation={(e) => openMenu(e, item)}>⋯</button
                  >
                </svelte:fragment>
                <svelte:fragment slot="meta" let:item>
                  <button
                    class="elink"
                    data-role="tile-edit-link"
                    on:click|stopPropagation={() => dispatch("openDevice", item.key)}>Edit</button
                  >
                </svelte:fragment>
              </Gallery>
            </div>
          </div>
        </div>
      {/if}

      <div
        class="grid"
        data-role="patterns-grid"
        data-source="library"
        hidden={sourceId !== "library"}
      >
        <Gallery
          luxel={$luxel}
          {search}
          only="compatible"
          bind:count={libraryCount}
          bind:loading={libraryLoading}
          bind:note={libraryNote}
          on:pick={(e) => dispatch("pick", { name: e.detail.name, source: e.detail.source ?? "" })}
        >
          <svelte:fragment slot="actions" let:item let:dead>
            <!-- §5.7: absent on a tile that does not compile — the meta link
                 below stays, so the source is still reachable (Gitea #529) -->
            {#if !dead}
              <button
                class="btn sm"
                data-role="tile-edit"
                on:click|stopPropagation={() =>
                  dispatch("pick", { name: item.name, source: item.source ?? "" })}
                >{openVerb}</button
              >
            {/if}
          </svelte:fragment>
          <svelte:fragment slot="meta" let:item>
            <button
              class="elink"
              data-role="tile-edit-link"
              on:click|stopPropagation={() =>
                dispatch("pick", { name: item.name, source: item.source ?? "" })}>{openVerb}</button
            >
          </svelte:fragment>
        </Gallery>
      </div>

      {#if $isPlayground}
        <!-- Mine: what this browser saved (localStorage), which used to be a
             row of chips above the library grid. -->
        <div class="grid" data-role="patterns-grid" data-source="mine" hidden={sourceId !== "mine"}>
          <Gallery
            luxel={$luxel}
            items={mineItems}
            {search}
            only="compatible"
            emptyNote="nothing saved in this browser yet"
            bind:count={mineCount}
            bind:note={mineNote}
            on:pick={(e) => dispatch("openSaved", e.detail.name)}
          >
            <svelte:fragment slot="actions" let:item>
              <button
                class="btn sm"
                data-role="tile-edit"
                on:click|stopPropagation={() => dispatch("openSaved", item.name)}>{openVerb}</button
              >
              <button
                class="btn sm icon"
                data-role="tile-menu"
                title="more actions"
                on:click|stopPropagation={(e) => openMenu(e, item)}>⋯</button
              >
            </svelte:fragment>
            <svelte:fragment slot="meta" let:item>
              <button
                class="elink"
                data-role="tile-edit-link"
                on:click|stopPropagation={() => dispatch("openSaved", item.name)}>{openVerb}</button
              >
            </svelte:fragment>
          </Gallery>
        </div>
      {/if}

      {#if hasPixelblazeLibrary}
        <div
          class="grid"
          data-role="patterns-grid"
          data-source="pixelblaze"
          hidden={sourceId !== "pixelblaze"}
        >
          <Gallery
            luxel={$luxel}
            src="pixelblaze-library.json"
            {search}
            only="compatible"
            emptyNote="corpus unavailable (no pixelblaze-library.json)"
            bind:count={corpusCount}
            bind:loading={corpusLoading}
            bind:note={corpusNote}
            on:pick={(e) =>
              dispatch("pick", { name: e.detail.name, source: e.detail.source ?? "" })}
          >
            <svelte:fragment slot="actions" let:item let:dead>
              <!-- §5.7: absent on a tile that does not compile (Gitea #529) -->
              {#if !dead}
                <button
                  class="btn sm"
                  data-role="tile-edit"
                  on:click|stopPropagation={() =>
                    dispatch("pick", { name: item.name, source: item.source ?? "" })}
                  >{openVerb}</button
                >
              {/if}
            </svelte:fragment>
            <svelte:fragment slot="meta" let:item>
              <button
                class="elink"
                data-role="tile-edit-link"
                on:click|stopPropagation={() =>
                  dispatch("pick", { name: item.name, source: item.source ?? "" })}
                >{openVerb}</button
              >
            </svelte:fragment>
          </Gallery>
        </div>
      {/if}
    {:else}
      <div class="tab-empty dim">loading patterns…</div>
    {/if}
  </div>
</div>

{#if menu}
  <!-- Add to scene ▸ belongs here too (regular 2D only, §5.4b) — Scenes do
       not exist until Phase B, so it is absent rather than disabled: Gitea
       #480 adds the entry. -->
  {@const item = menu.item}
  <Popover open anchor={menu.anchor} dataRole="tile-menu-popup" on:close={closeMenu}>
    {#if sourceId === "device"}
      <button class="mi" data-role="tile-menu-playlist" on:click={() => addToPlaylist(item)}>
        Add to playlist
      </button>
    {/if}
    <button class="mi" data-role="tile-menu-duplicate" on:click={() => void duplicate(item)}>
      Duplicate
    </button>
    {#if sourceId === "device"}
      <button
        class="mi del"
        data-role="tile-menu-delete"
        on:click={() => void removeFromDevice(item)}
      >
        Delete
      </button>
    {/if}
  </Popover>
{/if}

<style>
  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .patterns-tab[hidden] {
    display: none;
  }

  .patterns-tab {
    flex: 1;
    min-height: 0;
    background: var(--bg-panel);
    display: flex;
    flex-direction: column;
  }

  .dim {
    color: var(--text-dim);
  }

  .spacer {
    flex: 1;
  }

  .hint {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    margin: 8px 20px 0;
  }

  /* mockups.html `.pagebar` */
  .pagebar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }

  /* the segmented source control (D3) — mockups.html `.seg` */
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-inset);
  }

  .segbtn {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 14px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    font: 13px/1 var(--sans);
    cursor: pointer;
  }

  .segbtn + .segbtn {
    border-left: 1px solid var(--border);
  }

  .segbtn:hover {
    color: var(--text);
  }

  /* the active segment is BRIGHTER, not amber — an amber-soft ground with an
     inset amber underline, and only the count keeps the accent colour
     (Jeremy, 2026-09-19: "the colors don't match the mocks when activated") */
  .segbtn.on {
    background: var(--accent-soft);
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .ct {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .segbtn.on .ct {
    color: var(--accent);
  }

  .search {
    flex: none;
    width: 240px;
  }

  /* S1c: the primary shrinks to a `+` icon beside the search on a phone */
  .newicon {
    display: none;
  }

  .grids {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* the GRID is the scroll container: the device source stacks two galleries
     (the shown patterns and the collapsed "Not for this layout" group), and
     they scroll as one page, not as two independent panes */
  .grid {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .grid[hidden] {
    display: none;
  }

  .tab-empty {
    padding: 24px;
    font-size: 13px;
  }

  /* the collapsed second category for patterns this fixture cannot show */
  .incompat[hidden] {
    display: none;
  }

  .incompat {
    border-top: 1px solid var(--border);
  }

  .disc {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 14px 20px 0;
    border: none;
    border-radius: 0;
    background: transparent;
    cursor: pointer;
  }

  .disc:hover {
    color: var(--text);
  }

  .caret {
    display: inline-block;
    transition: transform 0.12s;
  }

  .caret.open {
    transform: rotate(90deg);
  }

  /* the mobile stand-in for the hover strip (S1c) — mockups.html `.elink` */
  .elink {
    display: none;
    margin-top: 5px;
    padding: 0;
    border: none;
    background: none;
    font-size: 12px;
    color: var(--accent);
    cursor: pointer;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: pat-spin 0.7s linear infinite;
  }

  @keyframes pat-spin {
    to {
      transform: rotate(1turn);
    }
  }

  /* S1c: the page bar becomes TWO rows — the segment full width, then the
     search and the `+` icon sharing the line below it. */
  @media (max-width: 600px) {
    .pagebar {
      flex-wrap: wrap;
      gap: 10px;
      padding: 12px;
    }

    .seg {
      width: 100%;
    }

    .segbtn {
      flex: 1;
      justify-content: center;
    }

    .search {
      flex: 1;
      width: auto;
    }

    .spacer {
      display: none;
    }

    .new {
      width: 32px;
      padding: 0;
    }

    .newfull {
      display: none;
    }

    .newicon {
      display: inline;
      font-size: 17px;
    }

    .hint {
      margin: 8px 12px 0;
    }

    .disc {
      padding: 14px 12px 0;
    }

    /* the hover strip is gone on a phone — `Edit` lives under the name */
    .elink {
      display: block;
    }
  }
</style>
