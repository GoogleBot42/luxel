<script lang="ts">
  // The device's own stored pattern library. Rows are lazily filled with
  // their source (one at a time — the device serves ~2 connections), which is
  // what makes the thumbnails render.
  import { createEventDispatcher } from "svelte";
  import PatternThumb from "../components/PatternThumb.svelte";
  import { device, deviceError, devicePatterns } from "../stores/device";
  import { devicePatternId, luxel } from "../stores/pattern";

  /** The tab is the visible one. */
  export let active = false;

  const dispatch = createEventDispatcher<{ open: string; new: void }>();
</script>

<div class="device-tab" data-role="device-panel" hidden={!active}>
  <div class="lib-head">
    <span class="lib-title">Device Patterns</span>
    <span class="dim">stored in the device's memory</span>
    <span class="spacer"></span>
    <button
      class="primary"
      data-role="device-new-pattern"
      disabled={!$device}
      on:click={() => dispatch("new")}
    >
      + New pattern
    </button>
  </div>
  {#if !$device}
    <p class="dim hint" data-role="device-offline">
      device unreachable — {$deviceError || "reload to retry"}.
    </p>
  {:else if $devicePatterns.length === 0}
    <p class="dim hint">no patterns stored on the device yet. Create one with “+ New pattern”.</p>
  {:else}
    <ul class="dev-list">
      {#each $devicePatterns as p (p.id)}
        <li>
          <button
            class="dev-item"
            data-role="device-pattern"
            class:active={p.id === $devicePatternId}
            on:click={() => dispatch("open", p.id)}
          >
            {#if $luxel}
              <PatternThumb luxel={$luxel} source={p.source} />
            {/if}
            <span class="dev-name">{p.name}</span>
            <span class="dim">edit ›</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .device-tab[hidden] {
    display: none;
  }

  .device-tab {
    flex: 1;
    min-height: 0;
    background: var(--bg-panel);
    overflow-y: auto;
  }

  .dim {
    color: var(--text-dim);
  }

  .spacer {
    flex: 1;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0;
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

  .dev-list {
    list-style: none;
    margin: 0;
    padding: 8px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 620px;
  }

  .dev-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    cursor: pointer;
  }

  .dev-item:hover {
    border-color: var(--accent);
  }

  .dev-item.active {
    border-color: var(--accent);
  }

  .dev-name {
    flex: 1;
    color: var(--text);
  }
</style>
