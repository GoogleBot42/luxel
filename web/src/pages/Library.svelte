<script lang="ts">
  // Pattern browsing. Two variants over the same shape: the built-in
  // "Patterns Library" (gallery.json, plus this browser's saved patterns) and
  // the local-corpus "PixelBlaze Library" (pixelblaze-library.json) — the tab
  // the audit already noted is one component with two data sources.
  import { createEventDispatcher } from "svelte";
  import Gallery from "../components/Gallery.svelte";
  import { luxel, saved } from "../stores/pattern";

  /** Which library this instance browses. */
  export let variant: "library" | "pixelblaze" = "library";
  /** The tab is the visible one (drives the lazy gallery mount). */
  export let active = false;

  const dispatch = createEventDispatcher<{
    pick: { name: string; kind: "strip" | "grid" | "cloud"; source: string };
    open: string;
    new: void;
  }>();

  /** Lazy-mount the gallery on first visit, then keep it alive so its compiled
   *  tile engines persist. */
  let mounted = false;
  $: if (active) mounted = true;
</script>

<div class="library-tab" data-role={`${variant === "library" ? "library" : "pixelblaze"}-panel`} hidden={!active}>
  <div class="lib-head">
    {#if variant === "library"}
      <span class="lib-title">Patterns Library</span>
      <span class="dim">examples &amp; community patterns{$saved.length ? " · your saved" : ""}</span>
      <span class="spacer"></span>
      <button class="primary" data-role="new-pattern" on:click={() => dispatch("new")}>
        + New pattern
      </button>
    {:else}
      <span class="lib-title">PixelBlaze Library</span>
      <span class="dim">original Pixelblaze community patterns (local corpus)</span>
      <span class="spacer"></span>
    {/if}
  </div>
  {#if variant === "library" && $saved.length > 0}
    <div class="saved-row">
      <span class="dim">your patterns:</span>
      {#each $saved as s (s.name)}
        <button class="chip" data-role="saved-pattern" on:click={() => dispatch("open", s.name)}>
          {s.name}
        </button>
      {/each}
    </div>
  {/if}
  <div class="lib-gallery">
    {#if mounted && $luxel}
      {#if variant === "library"}
        <Gallery luxel={$luxel} on:pick={(e) => dispatch("pick", e.detail)} />
      {:else}
        <Gallery
          luxel={$luxel}
          src="pixelblaze-library.json"
          emptyNote="corpus unavailable (no pixelblaze-library.json)"
          on:pick={(e) => dispatch("pick", e.detail)}
        />
      {/if}
    {:else}
      <div class="tab-empty dim">loading patterns…</div>
    {/if}
  </div>
</div>

<style>
  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .library-tab[hidden] {
    display: none;
  }

  .library-tab {
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

  .lib-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .lib-title {
    font-weight: 700;
    letter-spacing: 0.03em;
    color: var(--accent);
    font-size: 14px;
  }

  .saved-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
  }

  .chip {
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 999px;
  }

  .lib-gallery {
    flex: 1;
    min-height: 0;
  }

  .tab-empty {
    padding: 24px;
    font-size: 13px;
  }
</style>
