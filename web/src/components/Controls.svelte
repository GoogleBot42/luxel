<script lang="ts">
  import type { ControlHint } from "../lib/hints";
  import type { Control } from "../lib/luxel";
  import { createEventDispatcher } from "svelte";

  export let controls: Control[] = [];
  /** Saved values by control name (bound — persisted across recompiles).
   *  A plain object, reassigned on change: Svelte's template dependency
   *  tracking is static, so values MUST appear directly in expressions. */
  export let values: Record<string, number[]> = {};
  /** Live readouts for showNumber/gauge controls. */
  export let readouts: Map<string, number>;
  /** Script-specified bounds (`//#` directives). */
  export let hints: Map<string, ControlHint>;

  const dispatch = createEventDispatcher<{ set: { name: string; values: number[] } }>();

  function set(name: string, vals: number[]): void {
    values = { ...values, [name]: vals };
    dispatch("set", { name, values: vals });
  }

  function numFrom(e: Event): number {
    return Number((e.target as HTMLInputElement).value);
  }

  function scalar(name: string, e: Event): void {
    const v = numFrom(e);
    if (!Number.isNaN(v)) set(name, [v]);
  }

  function picker(name: string, i: number, e: Event): void {
    const cur = [0, 1, 2].map((j) => values[name]?.[j] ?? (j === 0 ? 0 : 1));
    cur[i] = numFrom(e);
    set(name, cur);
  }

  function toggle(name: string, e: Event): void {
    set(name, [(e.target as HTMLInputElement).checked ? 1 : 0]);
  }
</script>

{#if controls.length > 0}
  <div class="panel">
    {#each controls as c (c.name)}
      {@const h = hints.get(c.name) ?? {}}
      <div class="control">
        <span class="label" title={c.name}>{c.label}</span>
        {#if c.kind === "slider"}
          <input
            type="range"
            min={h.min ?? 0}
            max={h.max ?? 1}
            step={h.step ?? 0.001}
            value={values[c.name]?.[0] ?? h.default ?? 0.5}
            on:input={(e) => scalar(c.name, e)}
          />
          <input
            class="num"
            type="number"
            min={h.min ?? 0}
            max={h.max ?? 1}
            step={h.step ?? 0.001}
            value={values[c.name]?.[0] ?? h.default ?? 0.5}
            on:change={(e) => scalar(c.name, e)}
          />
        {:else if c.kind === "inputNumber"}
          <input
            class="num wide"
            type="number"
            min={h.min}
            max={h.max}
            step={h.step ?? 1}
            value={values[c.name]?.[0] ?? h.default ?? 0}
            on:change={(e) => scalar(c.name, e)}
          />
        {:else if c.kind === "hsvPicker" || c.kind === "rgbPicker"}
          {#each c.kind === "hsvPicker" ? ["H", "S", "V"] : ["R", "G", "B"] as ch, i}
            <span class="dim">{ch}</span>
            <input
              type="range"
              min="0"
              max="1"
              step="0.001"
              value={values[c.name]?.[i] ?? (i === 0 ? 0 : 1)}
              on:input={(e) => picker(c.name, i, e)}
            />
          {/each}
        {:else if c.kind === "toggle"}
          <input
            type="checkbox"
            checked={(values[c.name]?.[0] ?? h.default ?? 0) > 0.5}
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .num {
    width: 76px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .num.wide {
    width: 120px;
  }

  meter {
    flex: 1;
  }
</style>
