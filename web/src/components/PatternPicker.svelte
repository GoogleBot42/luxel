<script lang="ts">
  // THE picker (proposal §5.4: "`+ Add` … opens ONE picker used everywhere").
  // A searchable list of what the device can play, each row carrying the same
  // device-shaped live thumbnail the Patterns page and the playlist rows use,
  // so what you pick looks like what you will get.
  //
  // Today it lists patterns. Scenes are Phase B (#478/#481) and slot in as a
  // second SECTION, not a second component: `sections` below is the seam —
  // give it a `{ kind: "scene", … }` group and the markup, the search, the
  // keyboard handling and the `pick` event all work unchanged. That is why
  // `pick` carries a `kind` nothing reads yet.
  //
  // Exported for reuse: the editor's and the Patterns tile's ⋯ menus
  // ("Add to playlist", "Add to scene ▸") are the same choice made from a
  // different place (§5.4b).
  import { createEventDispatcher, tick } from "svelte";
  import PatternThumb from "./PatternThumb.svelte";
  import type { Luxel } from "../lib/luxel";

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

  const dispatch = createEventDispatcher<{
    pick: { id: string; kind: "pattern" | "scene" };
    close: void;
  }>();

  let query = "";
  let searchEl: HTMLInputElement | undefined;

  /** Focus the search as the panel appears — a phone keyboard opening on a
   *  list this long is the difference between typing and scrolling. */
  $: if (open) void tick().then(() => searchEl?.focus());
  // a fresh open starts from the whole list
  $: if (!open) query = "";

  $: needle = query.trim().toLowerCase();
  $: matches = patterns.filter(
    (p) => needle === "" || (p.name || p.id).toLowerCase().includes(needle),
  );

  /** One group per item kind. Phase B appends `{ kind: "scene", … }` here. */
  $: sections = [{ kind: "pattern" as const, label: "Patterns", items: matches }];

  function choose(id: string, kind: "pattern" | "scene"): void {
    dispatch("pick", { id, kind });
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
        class="x"
        data-role="picker-close"
        aria-label="close"
        on:click={() => dispatch("close")}>✕</button
      >
    </div>
    <input
      class="search"
      data-role="picker-search"
      type="search"
      placeholder="Search patterns…"
      bind:this={searchEl}
      bind:value={query}
    />
    <div class="body">
      {#each sections as section (section.kind)}
        {#if section.items.length > 0}
          <div class="section-label">{section.label}</div>
          <ul class="list">
            {#each section.items as p (p.id)}
              <li>
                <button
                  class="item"
                  data-role="picker-item"
                  data-id={p.id}
                  on:click={() => choose(p.id, section.kind)}
                >
                  {#if luxel}<PatternThumb {luxel} source={p.source} />{/if}
                  <span class="name">{p.name || p.id}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
      {#if matches.length === 0}
        <p class="empty dim" data-role="picker-empty">
          {patterns.length === 0
            ? "This device has no saved patterns yet — save one from the editor first."
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

  .x {
    padding: 2px 8px;
    line-height: 1;
  }

  .search {
    width: 100%;
  }

  .body {
    overflow-y: auto;
    min-height: 0;
  }

  .section-label {
    color: var(--text-dim);
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    margin: 2px 0 6px;
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

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    font-size: 12px;
    margin: 6px 2px;
  }

  .dim {
    color: var(--text-dim);
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
