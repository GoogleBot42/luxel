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
  import { createEventDispatcher } from "svelte";
  import Gallery, { type GalleryItem } from "../components/Gallery.svelte";
  import {
    device,
    deviceError,
    devicePatterns,
    isPlayground,
    playlist,
    queuePlaylistSave,
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

  $: sources = [
    {
      id: "device" as const,
      label: "On device",
      show: !$isPlayground,
      count: $devicePatterns.length,
    },
    { id: "library" as const, label: "Library", show: true, count: libraryCount },
    { id: "mine" as const, label: "Mine", show: $isPlayground, count: $saved.length },
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

  $: showing = sources.find((s) => s.id === sourceId);
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

  let menu: { item: GalleryItem; x: number; y: number } | null = null;

  function openMenu(e: MouseEvent, item: GalleryItem): void {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { item, x: Math.min(r.right, window.innerWidth - 8), y: r.bottom + 4 };
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
    playlist.update((pl) => ({
      ...pl,
      items: [...pl.items, { id: item.key, name: item.name, sec: null, controls: {} }],
    }));
    queuePlaylistSave();
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

<svelte:window on:click={closeMenu} on:keydown={(e) => e.key === "Escape" && closeMenu()} />

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
      class="search"
      data-role="gallery-search"
      type="search"
      placeholder="search patterns…"
      bind:value={search}
    />
    <span class="spacer"></span>
    {#if loading}
      <span class="spinner" aria-hidden="true"></span>
      <span class="dim" data-role="gallery-loading">loading patterns…</span>
    {:else}
      <span class="dim" data-role="gallery-count">
        {showing?.count ?? 0}
        {(showing?.count ?? 0) === 1 ? "pattern" : "patterns"}{sourceNote ? ` · ${sourceNote}` : ""}
      </span>
    {/if}
    {#if sourceId !== "pixelblaze"}
      <button class="primary" data-role="new-pattern" on:click={() => dispatch("new")}>
        + New pattern
      </button>
    {/if}
  </div>

  {#if !$isPlayground && !$device && sourceId === "device"}
    <p class="dim hint" data-role="device-offline">
      device unreachable — {$deviceError || "reload to retry"}.
    </p>
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
            playingKey={$devicePatternId}
            emptyNote="no patterns stored on the device yet — “+ New pattern” makes one"
            bind:note={deviceNote}
            on:pick={(e) => dispatch("playDevice", e.detail.key)}
          >
            <svelte:fragment slot="actions" let:item let:dead>
              <button
                class="act"
                data-role="tile-play"
                disabled={dead}
                on:click|stopPropagation={() => dispatch("playDevice", item.key)}>▶ Play</button
              >
              <button
                class="act"
                data-role="tile-edit"
                on:click|stopPropagation={() => dispatch("openDevice", item.key)}>Edit</button
              >
              <button
                class="act icon"
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
          bind:count={libraryCount}
          bind:loading={libraryLoading}
          bind:note={libraryNote}
          on:pick={(e) => dispatch("pick", { name: e.detail.name, source: e.detail.source ?? "" })}
        >
          <svelte:fragment slot="actions" let:item let:dead>
            <button
              class="act"
              data-role="tile-edit"
              disabled={dead}
              on:click|stopPropagation={() =>
                dispatch("pick", { name: item.name, source: item.source ?? "" })}>{openVerb}</button
            >
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
            emptyNote="nothing saved in this browser yet"
            bind:note={mineNote}
            on:pick={(e) => dispatch("openSaved", e.detail.name)}
          >
            <svelte:fragment slot="actions" let:item>
              <button
                class="act"
                data-role="tile-edit"
                on:click|stopPropagation={() => dispatch("openSaved", item.name)}>{openVerb}</button
              >
              <button
                class="act icon"
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
            emptyNote="corpus unavailable (no pixelblaze-library.json)"
            bind:count={corpusCount}
            bind:loading={corpusLoading}
            bind:note={corpusNote}
            on:pick={(e) =>
              dispatch("pick", { name: e.detail.name, source: e.detail.source ?? "" })}
          >
            <svelte:fragment slot="actions" let:item let:dead>
              <button
                class="act"
                data-role="tile-edit"
                disabled={dead}
                on:click|stopPropagation={() =>
                  dispatch("pick", { name: item.name, source: item.source ?? "" })}
                >{openVerb}</button
              >
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
  <div class="menu" data-role="tile-menu-popup" style={`left:${menu.x}px;top:${menu.y}px`}>
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
        class="mi danger"
        data-role="tile-menu-delete"
        on:click={() => void removeFromDevice(item)}
      >
        Delete
      </button>
    {/if}
  </div>
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
    font-size: 12px;
    margin: 6px 16px;
  }

  .pagebar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
    flex-wrap: wrap;
  }

  /* the segmented source control (D3) */
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 7px;
    overflow: hidden;
    background: var(--bg-inset);
  }

  .segbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    padding: 5px 12px;
    cursor: pointer;
  }

  .segbtn + .segbtn {
    border-left: 1px solid var(--border);
  }

  .segbtn:hover {
    color: var(--text);
  }

  .segbtn.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }

  .ct {
    font-size: 11px;
    opacity: 0.8;
    font-variant-numeric: tabular-nums;
  }

  .search {
    flex: none;
    width: 240px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-inset);
    color: var(--text);
    font-size: 13px;
  }

  .grids {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .grid {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .grid[hidden] {
    display: none;
  }

  .tab-empty {
    padding: 24px;
    font-size: 13px;
  }

  /* per-tile verbs, rendered into Gallery's hover strip */
  .act {
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    color: var(--text);
    cursor: pointer;
  }

  .act:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .act.icon {
    padding: 3px 6px;
    line-height: 1;
  }

  /* the mobile stand-in for the hover strip (S1c) */
  .elink {
    display: none;
    background: none;
    border: none;
    padding: 0;
    font-size: 11px;
    color: var(--accent);
    cursor: pointer;
  }

  .menu {
    position: fixed;
    z-index: 60;
    transform: translateX(-100%);
    min-width: 168px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-panel);
    box-shadow: 0 8px 24px rgb(0 0 0 / 45%);
    display: flex;
    flex-direction: column;
  }

  .mi {
    text-align: left;
    background: none;
    border: none;
    border-radius: 5px;
    padding: 6px 10px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
  }

  .mi:hover {
    background: var(--bg-inset);
  }

  .mi.danger {
    color: var(--error);
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

  @media (max-width: 600px) {
    .pagebar {
      gap: 8px;
      padding: 10px 12px;
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

    /* the hover strip is gone on a phone — `Edit` lives under the name */
    .elink {
      display: inline;
    }
  }
</style>
