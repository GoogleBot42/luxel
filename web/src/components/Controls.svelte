<script lang="ts">
  import type { Control } from "../lib/luxel";
  import { createEventDispatcher } from "svelte";

  export let controls: Control[] = [];
  /** Saved values by control name (persisted across recompiles). */
  export let values: Map<string, number[]>;
  /** Live readouts for showNumber/gauge controls. */
  export let readouts: Map<string, number>;

  const dispatch = createEventDispatcher<{ set: { name: string; values: number[] } }>();

  function val(name: string, i: number, fallback = 0.5): number {
    return values.get(name)?.[i] ?? fallback;
  }

  function set(name: string, vals: number[]): void {
    values.set(name, vals);
    dispatch("set", { name, values: vals });
  }

  function slider(name: string, e: Event): void {
    set(name, [Number((e.target as HTMLInputElement).value)]);
  }

  function picker(name: string, i: number, e: Event): void {
    const cur = [val(name, 0), val(name, 1), val(name, 2)];
    cur[i] = Number((e.target as HTMLInputElement).value);
    set(name, cur);
  }

  function toggle(name: string, e: Event): void {
    set(name, [(e.target as HTMLInputElement).checked ? 1 : 0]);
  }
</script>

{#if controls.length > 0}
  <div class="panel">
    {#each controls as c (c.name)}
      <div class="control">
        <span class="label">{c.label}</span>
        {#if c.kind === "slider" || c.kind === "inputNumber"}
          <input
            type="range"
            min="0"
            max="1"
            step="0.001"
            value={val(c.name, 0)}
            on:input={(e) => slider(c.name, e)}
          />
          <span class="mono dim">{val(c.name, 0).toFixed(3)}</span>
        {:else if c.kind === "hsvPicker" || c.kind === "rgbPicker"}
          {#each c.kind === "hsvPicker" ? ["H", "S", "V"] : ["R", "G", "B"] as ch, i}
            <span class="dim">{ch}</span>
            <input
              type="range"
              min="0"
              max="1"
              step="0.001"
              value={val(c.name, i, i === 0 ? 0 : 1)}
              on:input={(e) => picker(c.name, i, e)}
            />
          {/each}
        {:else if c.kind === "toggle"}
          <input
            type="checkbox"
            checked={val(c.name, 0, 0) > 0.5}
            on:change={(e) => toggle(c.name, e)}
          />
        {:else if c.kind === "trigger"}
          <button on:click={() => dispatch("set", { name: c.name, values: [] })}>fire</button>
        {:else}
          <span class="mono readout">{(readouts.get(c.name) ?? 0).toFixed(4)}</span>
          {#if c.kind === "gauge"}
            <meter min="0" max="1" value={Math.max(0, Math.min(1, readouts.get(c.name) ?? 0))}
            ></meter>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .control {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .label {
    min-width: 110px;
    color: var(--text-dim);
  }

  .dim {
    color: var(--text-dim);
    font-size: 12px;
  }

  .readout {
    color: var(--accent);
  }

  input[type="range"] {
    flex: 1;
  }

  meter {
    flex: 1;
  }
</style>
