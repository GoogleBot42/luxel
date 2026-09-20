<script lang="ts">
  import type { ControlHint } from "../lib/hints";
  import type { Control } from "../lib/luxel";
  import { createEventDispatcher } from "svelte";
  import ColorPicker from "./ColorPicker.svelte";

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

  const READONLY_KINDS: ReadonlySet<Control["kind"]> = new Set(["showNumber", "gauge"]);

  /** An UNTOUCHED control is running whatever the pattern's own top-level code
   *  put in the variable, and the engine offers no way to read that back
   *  (`lx_set_control` with no args INVOKES the handler, which would overwrite
   *  it). Only a `//# default=` declares it. Without one, the position drawn is
   *  a GUESS, so say so — a slider parked at a fabricated 0.5 reads as the
   *  pattern's real value when the engine actually holds something else.
   *  First user input writes `values[name]`, which clears the flag.
   *  NOTE: `cur`/`dflt` are passed in (not read off `values`/`hints` inside)
   *  so the template expression depends on them directly — Svelte's dependency
   *  tracking is static and would otherwise never re-run this. */
  function isGuess(
    cur: number[] | undefined,
    dflt: number | undefined,
    kind: Control["kind"],
  ): boolean {
    return (
      cur === undefined && dflt === undefined && kind !== "trigger" && !READONLY_KINDS.has(kind)
    );
  }

  const GUESS_TIP =
    "No //# default declared, and a control's live value cannot be read back from " +
    "the engine. The position shown is a PLACEHOLDER, not the running value — the " +
    "pattern's own top-level initialiser is what is rendering. Move it to take control.";

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

  function toggle(name: string, e: Event): void {
    set(name, [(e.target as HTMLInputElement).checked ? 1 : 0]);
  }
</script>

{#if controls.length > 0}
  <div class="panel">
    {#each controls as c (c.name)}
      {@const h = hints.get(c.name) ?? {}}
      {@const guess = isGuess(values[c.name], h.default, c.kind)}
      <!-- mockup S2 `.ctlrow`: exactly three cells, always — name, the
           widget, and whatever reads the widget out. The third is empty for a
           swatch or a switch, which is what keeps every row's widget starting
           on the same line. -->
      <div class="control" class:guess>
        <span class="label" title={c.name}>
          <span class="txt">{c.label}</span>
          {#if guess}<span class="guessflag" title={GUESS_TIP}>?</span>{/if}
        </span>
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
            class="inp xs num"
            type="number"
            min={h.min ?? 0}
            max={h.max ?? 1}
            step={h.step ?? 0.001}
            value={values[c.name]?.[0] ?? h.default ?? 0.5}
            on:change={(e) => scalar(c.name, e)}
          />
        {:else if c.kind === "inputNumber"}
          <input
            class="inp xs num"
            type="number"
            min={h.min}
            max={h.max}
            step={h.step ?? 1}
            value={values[c.name]?.[0] ?? h.default ?? 0}
            on:change={(e) => scalar(c.name, e)}
          />
          <span></span>
        {:else if c.kind === "hsvPicker" || c.kind === "rgbPicker"}
          <!-- a colour is picked, not typed as three raw channels (#538). The
               swatch carries the value; the popover carries the field, the hue
               strip and direct entry in both spaces. What it emits is still
               the control's own triple, so the device push is unchanged. -->
          <ColorPicker
            kind={c.kind === "hsvPicker" ? "hsv" : "rgb"}
            label={c.label}
            value={values[c.name] ?? []}
            dim={guess}
            on:input={(e) => set(c.name, e.detail)}
          />
          <span></span>
        {:else if c.kind === "toggle"}
          <!-- native tri-state says "unknown" better than any badge can -->
          <input
            type="checkbox"
            indeterminate={guess}
            checked={(values[c.name]?.[0] ?? h.default ?? 0) > 0.5}
            on:change={(e) => toggle(c.name, e)}
          />
          <span></span>
        {:else if c.kind === "trigger"}
          <button class="btn sm" on:click={() => dispatch("set", { name: c.name, values: [] })}>
            fire
          </button>
          <span></span>
        {:else if c.kind === "gauge"}
          <meter min="0" max="1" value={Math.max(0, Math.min(1, readouts.get(c.name) ?? 0))}></meter>
          <span class="mono readout">{(readouts.get(c.name) ?? 0).toFixed(4)}</span>
        {:else}
          <span class="mono readout">{(readouts.get(c.name) ?? 0).toFixed(4)}</span>
          <span></span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* mockup S2 `.ctlrow`: name, the widget, its number — one grid so every
     row's slider starts and ends on the same two lines */
  .control {
    display: grid;
    grid-template-columns: 82px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
  }

  .label {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    color: var(--text-dim);
    font-size: 12px;
  }

  /* the NAME ellipsizes; the placeholder flag beside it never does, or the
     one thing that explains the dimmed widget is the first thing clipped */
  .txt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* an untouched, undeclared control draws a placeholder position, not a real
     value — dim the widget and flag it (see isGuess) */
  .control.guess input[type="range"],
  .control.guess input[type="number"] {
    opacity: 0.4;
  }

  .guessflag {
    flex: none;
    color: var(--warn);
    font-weight: 700;
    cursor: help;
  }

  .readout {
    color: var(--accent);
  }

  input[type="range"] {
    width: 100%;
    min-width: 0;
  }

  /* a switch or a swatch is its own width, not the column's */
  input[type="checkbox"] {
    justify-self: start;
  }

  meter {
    width: 100%;
  }
</style>
