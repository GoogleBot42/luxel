<script lang="ts">
  // The Outputs table (proposal §5.3b, mockups S3j/S3k) — shown only when
  // `caps.outputs > 1`.
  //
  // The rule it encodes: an output drives a CONSECUTIVE RUN of the Layout's
  // one index space — a pixel range on a strip, a run of panels along the
  // existing chain order on a matrix. So each row computes the range it owns
  // rather than asking anyone to add up offsets, and everything downstream
  // (patterns, playlist, brightness, Home Assistant) never learns there are
  // two wires.
  //
  // The strip graphic lives here; the matrix one is the arrangement SVG with
  // its `outputCounts` filled in, because a chain tinted by output IS the
  // per-output chain polyline.
  import { createEventDispatcher } from "svelte";
  import { outputRanges, outputsSum } from "../lib/settingsCaps";
  import type { LayoutWire } from "../lib/device";

  type Output = LayoutWire["outputs"][number];

  export let outputs: Output[] = [];
  /** What a `count` counts on this Layout. */
  export let unit: "pixels" | "panels" = "pixels";
  /** Physical outputs the board has (`caps.outputs`). */
  export let maxOutputs = 1;
  /** Pins the board's picker accepts; empty = a free-text GPIO number. */
  export let pins: number[] = [];
  export let protocols: string[] = [];
  /** The index space the runs must add up to. */
  export let total = 0;

  const ORDERS = ["rgb", "rbg", "grb", "gbr", "brg", "bgr"];
  const COLORS = ["#e8a33d", "#5b9bd5", "#77b57a", "#c07ad0"];

  const dispatch = createEventDispatcher<{ apply: Output[] }>();

  $: rows = outputs.slice(0, Math.max(1, maxOutputs));
  $: ranges = outputRanges(
    rows.map((o) => o.count),
    unit,
  );
  $: sum = outputsSum(rows.map((o) => o.count));
  /** The firmware refuses a table that does not partition the space exactly,
   *  so say so here rather than letting the POST bounce. */
  $: mismatch = total > 0 && sum !== total;

  function edit(i: number, patch: Partial<Output>): void {
    const next = rows.map((o, n) => (n === i ? { ...o, ...patch } : o));
    dispatch("apply", next);
  }

  /** A pad no other output is on — two outputs on one pin is refused
   *  (docs/api.md, `/api/layout` validation), so the default must not be the
   *  one the row above already uses. */
  function freePin(used: readonly number[]): number {
    const taken = new Set(used);
    for (const p of pins) if (!taken.has(p)) return p;
    for (let p = 0; p < 64; p++) if (!taken.has(p)) return p;
    return 0;
  }

  /** Add an output. Every run must be at least 1 and the table must still
   *  partition the space exactly (the device refuses otherwise), so the new
   *  run takes what is spare — and when nothing is spare, half of the last
   *  one, which is the split anybody adding a second wire wants. */
  function addOutput(): void {
    const next = rows.map((o) => ({ ...o }));
    const last = next[next.length - 1];
    let count = Math.max(0, total - sum);
    if (count < 1 && last) {
      count = Math.max(1, Math.floor(last.count / 2));
      last.count = Math.max(1, last.count - count);
    }
    dispatch("apply", [
      ...next,
      {
        n: next.length,
        pin: freePin(next.map((o) => o.pin)),
        proto: last?.proto ?? protocols[0] ?? "ws2812",
        order: last?.order ?? "grb",
        count: Math.max(1, count),
        rev: false,
      },
    ]);
  }

  function removeOutput(i: number): void {
    const kept = rows.filter((_, n) => n !== i).map((o, n) => ({ ...o, n }));
    // give the lost run back to the last remaining output, so the table keeps
    // partitioning the space exactly and the POST is accepted
    const last = kept[kept.length - 1];
    if (last && total > 0) last.count = Math.max(0, total - outputsSum(kept.slice(0, -1).map((o) => o.count)));
    dispatch("apply", kept);
  }
</script>

<div class="outputs" data-role="outputs">
  <div class="slabel">Outputs</div>
  <div class="outtable">
    {#each rows as o, i (o.n)}
      <div class="orow" data-role="output-row" data-n={o.n}>
        <span class="ochip" style="background:{COLORS[i % COLORS.length]}">{o.n + 1}</span>
        <span class="un">GPIO</span>
        {#if pins.length}
          <select
            data-role="output-pin"
            value={String(o.pin)}
            on:change={(e) => edit(i, { pin: Number(e.currentTarget.value) })}
          >
            {#each pins as p}<option value={String(p)}>{p}</option>{/each}
          </select>
        {:else}
          <input
            class="num"
            data-role="output-pin"
            type="number"
            min="0"
            max="63"
            value={o.pin}
            on:change={(e) => edit(i, { pin: Number(e.currentTarget.value) })}
          />
        {/if}
        <select
          data-role="output-proto"
          value={o.proto}
          on:change={(e) => edit(i, { proto: e.currentTarget.value })}
        >
          {#each protocols.length ? protocols : [o.proto] as p}<option value={p}>{p}</option>{/each}
        </select>
        <select
          data-role="output-order"
          value={o.order}
          on:change={(e) => edit(i, { order: e.currentTarget.value })}
        >
          {#each ORDERS as c}<option value={c}>{c.toUpperCase()}</option>{/each}
        </select>
        <input
          class="num"
          data-role="output-count"
          type="number"
          min="0"
          value={o.count}
          on:change={(e) => edit(i, { count: Number(e.currentTarget.value) })}
        />
        <span class="un">{unit === "panels" ? "panels" : "px"}</span>
        <label class="ckrow">
          <input
            type="checkbox"
            data-role="output-rev"
            checked={o.rev}
            on:change={(e) => edit(i, { rev: e.currentTarget.checked })}
          />
          reverse
        </label>
        <span class="range mono" data-role="output-range">
          {unit} {ranges[i]?.from ?? 0}–{ranges[i]?.to ?? 0}
        </span>
        {#if rows.length > 1}
          <button
            class="link"
            data-role="output-remove"
            title="remove this output"
            on:click={() => removeOutput(i)}>✕</button
          >
        {/if}
      </div>
    {/each}
  </div>

  {#if rows.length < maxOutputs}
    <button class="link" data-role="output-add" on:click={addOutput}>+ Add output</button>
  {/if}

  {#if unit === "pixels" && total > 0}
    <!-- the strip, split into output-tinted runs with an IN marker and a
         direction arrow per run; a reversed run's IN sits at its far end -->
    <svg class="splitbar" viewBox="0 0 640 60" role="img" aria-label="how the outputs split the strip">
      {#each rows as o, i (o.n)}
        {@const from = ranges[i]?.from ?? 0}
        {@const w = (Math.max(0, o.count) / total) * 570}
        {@const x = 34 + (from / total) * 570}
        <rect {x} y="24" width={Math.max(1, w)} height="16" rx="2" style="fill:{COLORS[i % COLORS.length]}" opacity="0.6" />
        <text x={x + w / 2} y="16" style="fill:{COLORS[i % COLORS.length]}" class="lbl" text-anchor="middle"
          >output {o.n + 1}</text
        >
        <!-- the IN end of the run, arrow pointing the way the wire counts -->
        {#if o.rev}
          <path d={`M${x + w - 11},32 L${x + w - 2},27 L${x + w - 2},37 z`} style="fill:{COLORS[i % COLORS.length]}" />
        {:else}
          <path d={`M${x + 11},32 L${x + 2},27 L${x + 2},37 z`} style="fill:{COLORS[i % COLORS.length]}" />
        {/if}
        <text x={x + 2} y="54" class="lbl dimtext" text-anchor="start">{from}</text>
      {/each}
      <text x="604" y="54" class="lbl dimtext" text-anchor="end">{Math.max(0, total - 1)}</text>
    </svg>
  {/if}

  <p class="dim hint" data-role="outputs-note">
    {#if mismatch}
      The runs add up to {sum} {unit}, but the Layout has {total} — the device refuses a table
      that does not partition its pixel space exactly.
    {:else}
      Total {total}
      {unit} · one fixture to every pattern, playlist and Home Assistant light. Each output needs
      its own pad.
    {/if}
  </p>
</div>

<style>
  .outputs {
    margin-top: 16px;
  }

  .outtable {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 6px 0;
  }

  .orow {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
  }

  .ochip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: #14161b;
    font-size: 11px;
    font-weight: 700;
    flex: none;
  }

  .un {
    font-size: 11px;
    color: var(--text-dim);
  }

  .orow :global(.num) {
    width: 52px;
  }

  .orow select {
    max-width: 96px;
  }

  .ckrow {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .range {
    font-size: 11px;
    color: var(--accent);
  }

  .splitbar {
    display: block;
    width: 100%;
    max-width: 640px;
    height: auto;
    margin: 10px 0 2px;
  }

  .lbl {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 10px;
  }

  .dimtext {
    fill: var(--text-dim);
  }
</style>
