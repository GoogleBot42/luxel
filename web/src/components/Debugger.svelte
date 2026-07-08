<script lang="ts">
  // The step/breakpoint debugger panel, shared by the pattern editor and the
  // map editor. Purely a view over a DebugSnapshot; it dispatches step/break
  // and the parent drives the matching engine.
  import { createEventDispatcher } from "svelte";
  import type { DebugSnapshot, StepKind } from "../lib/luxel";

  export let snapshot: DebugSnapshot;
  /** Hint shown while running (what a gutter click does here). */
  export let runningHint = "running — click the gutter to set breakpoints";

  const dispatch = createEventDispatcher<{ step: StepKind; break: void }>();

  function fmtRaw(raw: number): string {
    return (raw / 65536).toFixed(4).replace(/\.?0+$/, "") || "0";
  }
  function fmtLocal(l: { raw?: number; array?: number; fn?: number }): string {
    if (l.raw !== undefined) return fmtRaw(l.raw);
    if (l.array !== undefined) return `array[${l.array}]`;
    return `fn#${l.fn}`;
  }
</script>

<div class="debugger" data-paused={snapshot.paused}>
  <div class="debug-bar">
    {#if snapshot.paused}
      <button class="db-continue" on:click={() => dispatch("step", "continue")}>▶ continue</button>
      <button class="db-over" on:click={() => dispatch("step", "over")}>step</button>
      <button class="db-into" on:click={() => dispatch("step", "into")}>into</button>
      <button class="db-out" on:click={() => dispatch("step", "out")}>out</button>
    {:else}
      <button class="db-break" on:click={() => dispatch("break")}>break</button>
      <span class="dim">{runningHint}</span>
    {/if}
  </div>
  {#if snapshot.paused}
    <div class="debug-status mono">
      paused at line {snapshot.line}{snapshot.pixel !== null && snapshot.pixel !== undefined
        ? ` · pixel ${snapshot.pixel}`
        : ""}
    </div>
    {#if snapshot.stack && snapshot.stack.length > 0}
      <div class="stack" data-role="stack">
        {#each snapshot.stack as f, i (i)}
          <div class="stack-frame mono">
            <span class="fn-name">{f.name}</span>
            <span class="dim">line {f.line}</span>
          </div>
          {#if i === 0}
            <table class="locals">
              <tbody>
                {#each f.locals as l (l.name)}
                  <tr>
                    <td class="name mono">{l.name}</td>
                    <td class="value mono">
                      {#if l.raw !== undefined}{fmtRaw(l.raw)}
                      {:else if l.array !== undefined}array[{l.array}]
                      {:else}fn#{l.fn}{/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {/each}
      </div>
    {/if}
    {#if snapshot.globals && snapshot.globals.length > 0}
      <div class="scope-title dim">globals</div>
      <table class="locals" data-role="globals">
        <tbody>
          {#each snapshot.globals as g (g.name)}
            <tr>
              <td class="name mono">{g.name}</td>
              <td class="value mono">{fmtLocal(g)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</div>

<style>
  .debugger {
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-inset);
  }

  .debug-bar {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 12px;
  }

  .debug-status {
    color: var(--accent);
    font-size: 12px;
  }

  .dim {
    color: var(--text-dim);
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stack-frame {
    display: flex;
    gap: 8px;
    font-size: 12px;
  }

  .fn-name {
    color: var(--text);
  }

  .locals {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    margin: 2px 0 6px 12px;
  }

  .locals td {
    padding: 1px 6px;
  }

  .locals .name {
    color: var(--text-dim);
    width: 40%;
  }

  .locals .value {
    color: var(--accent);
  }

  .scope-title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-top: 4px;
  }

  .mono {
    font-family: ui-monospace, Menlo, Consolas, monospace;
  }
</style>
