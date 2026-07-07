<script lang="ts">
  // One playlist entry: thumbnail + name, a duration override (checkbox +
  // seconds, inheriting the playlist default when off), inline parameter
  // controls (the same pattern can appear twice with different params), and
  // reorder/remove. The pattern's source is compiled locally just to read its
  // control metadata.
  import { createEventDispatcher } from "svelte";
  import Controls from "./Controls.svelte";
  import PatternThumb from "./PatternThumb.svelte";
  import { Engine, Luxel } from "../lib/luxel";
  import type { Control } from "../lib/luxel";
  import { parseControlHints } from "../lib/hints";
  import type { ControlHint } from "../lib/hints";
  import type { PlaylistItem } from "../lib/device";

  export let luxel: Luxel;
  export let source: string | undefined;
  export let item: PlaylistItem;
  export let defaultSec: number;
  export let missing = false;
  export let active = false;
  export let first = false;
  export let last = false;

  const dispatch = createEventDispatcher<{
    change: void;
    remove: void;
    move: number;
    dragstart: void;
    drop: void;
  }>();

  let dragover = false;

  let controls: Control[] = [];
  let hints = new Map<string, ControlHint>();
  let built = "";
  const readouts = new Map<string, number>();

  // compile once (per source) to read the control list + //# hints
  $: if (luxel && source !== undefined && source !== built) {
    built = source;
    const e = luxel.compile(source, 64);
    controls = e instanceof Engine ? (() => {
      const c = e.controls();
      e.free();
      return c;
    })() : [];
    hints = parseControlHints(source);
  }

  // settable controls only (showNumber/gauge are read-only readouts)
  $: params = controls.filter((c) => c.kind !== "showNumber" && c.kind !== "gauge");

  $: effective = item.sec ?? defaultSec;
  $: durationLabel = effective > 0 ? `${effective}s` : "manual";

  function onSet(e: CustomEvent<{ name: string; values: number[] }>): void {
    item.controls = { ...item.controls, [e.detail.name]: e.detail.values };
    item = item; // trigger local reactivity (in-place prop mutation)
    dispatch("change");
  }

  function toggleOverride(e: Event): void {
    item.sec = (e.target as HTMLInputElement).checked ? Math.max(1, defaultSec || 30) : null;
    item = item;
    dispatch("change");
  }

  function onSec(e: Event): void {
    const v = (e.target as HTMLInputElement).value.trim();
    item.sec = v === "" ? 0 : Math.max(0, Math.round(Number(v) || 0));
    item = item;
    dispatch("change");
  }
</script>

<li
  class="row"
  class:active
  class:missing
  class:dragover
  data-role="playlist-item"
  on:dragover|preventDefault={() => (dragover = true)}
  on:dragleave={() => (dragover = false)}
  on:drop|preventDefault={() => {
    dragover = false;
    dispatch("drop");
  }}
>
  <div class="head">
    <span
      class="grip"
      data-role="pl-grip"
      title="drag to reorder"
      role="button"
      tabindex="-1"
      aria-label="drag to reorder"
      draggable="true"
      on:dragstart={() => dispatch("dragstart")}
    >⠿</span>
    {#if luxel && !missing}<PatternThumb {luxel} {source} />{/if}
    <span class="name" data-role="pl-name">
      {item.name || item.id}{#if missing}<span class="miss"> (deleted)</span>{/if}
    </span>
    <span class="dur dim" data-role="pl-duration">{durationLabel}</span>
    <span class="spacer"></span>
    <label class="ovr" title="override the playlist default for this item">
      <input
        type="checkbox"
        data-role="pl-override"
        checked={item.sec !== null}
        on:change={toggleOverride}
      />
      <span class="dim">custom</span>
    </label>
    {#if item.sec !== null}
      <input
        class="num"
        data-role="pl-sec"
        type="number"
        min="0"
        title="seconds (0 = manual)"
        value={item.sec}
        on:change={onSec}
      />
      <span class="dim">s</span>
    {/if}
    <button class="mv" title="move up" disabled={first} on:click={() => dispatch("move", -1)}>↑</button>
    <button class="mv" title="move down" disabled={last} on:click={() => dispatch("move", 1)}>↓</button>
    <button class="rm" data-role="pl-remove" title="remove" on:click={() => dispatch("remove")}>×</button>
  </div>
  {#if params.length > 0}
    <div class="params">
      <Controls controls={params} values={item.controls} {readouts} {hints} on:set={onSet} />
    </div>
  {/if}
</li>

<style>
  .row {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-inset);
    padding: 8px 10px;
    margin-bottom: 8px;
  }

  .row.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .row.missing {
    opacity: 0.6;
    border-style: dashed;
  }

  .row.dragover {
    border-color: var(--accent);
    border-style: dashed;
  }

  .grip {
    cursor: grab;
    color: var(--text-dim);
    user-select: none;
    font-size: 14px;
    line-height: 1;
  }

  .grip:active {
    cursor: grabbing;
  }

  .miss {
    color: var(--error, #e05555);
    font-weight: 400;
    font-size: 12px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .name {
    color: var(--text);
    font-weight: 500;
  }

  .dur {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .ovr {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    cursor: pointer;
  }

  .num {
    width: 56px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .mv,
  .rm {
    padding: 2px 8px;
    line-height: 1;
  }

  .params {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .dim {
    color: var(--text-dim);
  }
</style>
