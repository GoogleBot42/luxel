<!-- The exported type lives in a MODULE script: an `export interface` in the
     instance script is a prop declaration as far as svelte2tsx is concerned
     ("Modifiers cannot appear here"). Same shape `components/Gallery.svelte`
     uses for `GalleryItem`. -->
<script lang="ts" context="module">
  /** One source chip. */
  export interface Source {
    id: string;
    label: string;
    /** What the source SHOWS — the chip's number, never a promise. */
    count: number;
  }
</script>

<script lang="ts">
  // The SOURCE control (proposal §5.1, D3, mockups S1/S1b/S1c): one page, N
  // sources, not N tabs. `pages/Patterns.svelte` drew the first one inline;
  // `pages/Sprites.svelte` and `pages/Scenes.svelte` grew the same control in
  // #785, so it is a component rather than a third copy of the markup.
  //
  // The class names are `srcseg`/`srcbtn`, NOT the mock's `.seg`/`.segbtn`:
  // `components/scene/scene.css` defines a GLOBAL `.scenes .seg` as a
  // full-width grid (the inspector's align control), and both new pages wear
  // the `scenes` class to inherit that design system. A `.seg` here would be
  // stretched by it — scoped component CSS does not shield an element from a
  // global selector that also matches.
  import { createEventDispatcher } from "svelte";

  export let sources: Source[] = [];
  export let value = "";
  /** `sprites-sources` → `sprites-source-library` per chip, Patterns' shape. */
  export let dataRole = "sources";

  const dispatch = createEventDispatcher<{ select: string }>();
</script>

<div class="srcseg" data-role={dataRole} role="tablist">
  {#each sources as s (s.id)}
    <button
      class="srcbtn"
      class:on={value === s.id}
      data-role={`${dataRole.replace(/s$/, "")}-${s.id}`}
      role="tab"
      aria-selected={value === s.id}
      on:click={() => dispatch("select", s.id)}
    >
      {s.label}
      <span class="ct">{s.count}</span>
    </button>
  {/each}
</div>

<style>
  /* mockups.html `.seg`, as `pages/Patterns.svelte` renders it */
  .srcseg {
    display: inline-flex;
    flex: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-inset);
  }

  .srcbtn {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 14px;
    border: none;
    /* the divider is a RIGHT border, dropped on the last segment */
    border-right: 1px solid var(--border);
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    font: 13px/1.45 var(--sans);
    cursor: pointer;
    white-space: nowrap;
  }

  .srcbtn:last-child {
    border-right: 0;
  }

  .srcbtn:hover {
    color: var(--text);
  }

  .srcbtn.on {
    background: var(--accent-soft);
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .ct {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .srcbtn.on .ct {
    color: var(--accent);
  }

  /* the phone keeps the chips and drops their padding, not the other way
     round: the source control is how you reach the library at all (S6d) */
  @media (max-width: 600px) {
    .srcbtn {
      padding: 0 9px;
      font-size: 12px;
    }
  }
</style>
