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
  /** The identity colour of a wire, in output order (mockup S3j: amber, then
   *  a blue that "means this wire and appears nowhere else in the UI").
   *
   *  `fill` is the wire's colour — the bar, the arrow, the chip's tint and
   *  ring. `ink` is what reads on that tint: output 1's amber is already
   *  light enough to be its own text, output 2's blue is not, so the mockup
   *  writes `2` in a lighter blue than the wire it names. Written as rgb()
   *  triples rather than `color-mix()` so the computed value is the plain
   *  `rgba(…)` the mockup states (mockdiff compares the computed string). */
  const COLORS: { fill: string; ink: string; rgb: string }[] = [
    { fill: "#e8a33d", ink: "#e8a33d", rgb: "232, 163, 61" },
    { fill: "#5b9bd5", ink: "#7db4e6", rgb: "91, 155, 213" },
    { fill: "#77b57a", ink: "#93c996", rgb: "119, 181, 122" },
    { fill: "#c07ad0", ink: "#d29ade", rgb: "192, 122, 208" },
  ];
  const wire = (i: number): { fill: string; ink: string; rgb: string } =>
    COLORS[i % COLORS.length] ?? COLORS[0]!;

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
        <span
          class="ochip"
          style="color:{wire(i).ink};background:rgba({wire(i).rgb}, 0.16);box-shadow:inset 0 0 0 1px rgba({wire(i).rgb}, 0.55)"
          >{o.n + 1}</span
        >
        <span class="un">GPIO</span>
        {#if pins.length}
          <select
            class="w64"
            data-role="output-pin"
            value={String(o.pin)}
            on:change={(e) => edit(i, { pin: Number(e.currentTarget.value) })}
          >
            {#each pins as p}<option value={String(p)}>{p}</option>{/each}
          </select>
        {:else}
          <input
            class="inp num w64"
            data-role="output-pin"
            type="number"
            min="0"
            max="63"
            value={o.pin}
            on:change={(e) => edit(i, { pin: Number(e.currentTarget.value) })}
          />
        {/if}
        <select
          class="w96"
          data-role="output-proto"
          value={o.proto}
          on:change={(e) => edit(i, { proto: e.currentTarget.value })}
        >
          {#each protocols.length ? protocols : [o.proto] as p}<option value={p}>{p}</option>{/each}
        </select>
        <select
          class="w68"
          data-role="output-order"
          value={o.order}
          on:change={(e) => edit(i, { order: e.currentTarget.value })}
        >
          {#each ORDERS as c}<option value={c}>{c.toUpperCase()}</option>{/each}
        </select>
        <input
          class="inp num"
          data-role="output-count"
          type="number"
          min="0"
          value={o.count}
          on:change={(e) => edit(i, { count: Number(e.currentTarget.value) })}
        />
        <span class="un">{unit === "panels" ? "panels" : "px"}</span>
        <label class="ckrow sm">
          <input
            class="cbxin"
            type="checkbox"
            data-role="output-rev-input"
            checked={o.rev}
            on:change={(e) => edit(i, { rev: e.currentTarget.checked })}
          />
          <span class="cbx" class:on={o.rev} data-role="output-rev">{o.rev ? "✓" : ""}</span>
          reverse
        </label>
        <span class="range mono" data-role="output-range">
          {unit} {ranges[i]?.from ?? 0}–{ranges[i]?.to ?? 0}
        </span>
        <!-- only the LAST wire can go (mockup S3j draws the ✕ on it alone):
             an output is a consecutive run of one space, so removing a middle
             one would renumber every wire after it -->
        {#if rows.length > 1 && i === rows.length - 1}
          <button
            class="btn sm icon quiet rm"
            data-role="output-remove"
            title="remove this output"
            on:click={() => removeOutput(i)}>✕</button
          >
        {:else}
          <!-- mockup S3j: a 24px hole where the ✕ would be, so every row's
               range column lines up whether or not the row can be removed -->
          <span class="ospacer"></span>
        {/if}
      </div>
    {/each}
  </div>

  {#if rows.length < maxOutputs}
    <button class="btn sm" data-role="output-add" on:click={addOutput}>+ Add output</button>
  {/if}

  {#if unit === "pixels" && total > 0}
    <!-- The strip, split into output-tinted runs (mockup S3j `.arrbox`): one
         run per output, an IN marker and a direction arrow at the end its
         wire plugs into — a reversed run's IN sits at its far end — and the
         index each run starts and finishes on under the bar. -->
    <div class="arrbox" data-role="arrangement" data-mode="split">
      <svg
        class="arrsvg"
        width="640"
        height="76"
        viewBox="0 0 640 76"
        role="img"
        aria-label="how the outputs split the strip"
      >
        <defs>
          <pattern id="outticks" width="3" height="16" patternUnits="userSpaceOnUse">
            <rect x="0" y="0" width="1" height="16" fill="rgba(0,0,0,.55)" />
          </pattern>
        </defs>
        {#each rows as o, i (o.n)}
          {@const from = ranges[i]?.from ?? 0}
          {@const w = (Math.max(0, o.count) / total) * 570}
          {@const x = 34 + (from / total) * 570}
          {@const c = wire(i).fill}
          <rect {x} y="30" width={Math.max(1, w)} height="16" rx="2" style="fill:{c}" opacity="0.6" />
          <text x={x + w / 2} y="22" style="fill:{c}" class="lbl" text-anchor="middle"
            >output {o.n + 1}</text
          >
          {#if i > 0}<line x1={x} y1="26" x2={x} y2="50" stroke="#0e1013" stroke-width="2" />{/if}
          <!-- the IN end of the run, arrow pointing the way the wire counts -->
          {#if o.rev}
            <text x={x + w + 25} y="27" style="fill:{c}" class="lbl" text-anchor="middle">IN</text>
            <rect x={x + w + 20} y="33" width="10" height="10" rx="2" style="fill:{c}" />
            <line x1={x + w + 10} y1="38" x2={x + w + 20} y2="38" style="stroke:{c}" stroke-width="2.4" />
            <path d={`M${x + w},38 L${x + w + 9},33 L${x + w + 9},43 z`} style="fill:{c}" />
          {:else}
            <text x={x - 21} y="27" style="fill:{c}" class="lbl" text-anchor="middle">IN</text>
            <rect x={x - 28} y="33" width="10" height="10" rx="2" style="fill:{c}" />
            <line x1={x - 18} y1="38" x2={x - 8} y2="38" style="stroke:{c}" stroke-width="2.4" />
            <path d={`M${x},38 L${x - 9},33 L${x - 9},43 z`} style="fill:{c}" />
          {/if}
          <text x={x + 2} y="62" class="lbl dimtext" text-anchor="start">{from}</text>
          <text x={x + w - 6} y="62" class="lbl dimtext" text-anchor="end">{ranges[i]?.to ?? 0}</text>
        {/each}
        <rect x="34" y="30" width="570" height="16" rx="2" fill="url(#outticks)" />
      </svg>
    </div>
  {/if}

  <!-- the matrix arrangement picture, handed in by LayoutCard: the mockups
       put the picture between the table that cuts the chain up and the notes
       that close the form (S3k) -->
  <slot />

  <div class="notes" data-role="outputs-note">
    {#if mismatch}
      <div>
        The runs add up to {sum} {unit}, but the Layout has {total} — the device refuses a table
        that does not partition its pixel space exactly.
      </div>
    {:else}
      <div>
        Total {total}
        {unit} · one fixture to every pattern, playlist and Home Assistant light.
      </div>
    {/if}
    <div>Each output needs its own pad. Data pin changes apply after a reboot.</div>
  </div>
</div>

<style>
  /* mockup S3j: the Outputs label, its table, then the picture — a plain
     block stack, because that is the rhythm every other form row keeps */
  .outputs {
    margin-top: 18px;
  }

  /* mockup S3j: ONE bordered table, hairlines between the rows */
  .outtable {
    margin-top: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: var(--bg-inset);
  }

  /* mockup S3j: ONE line per wire — it only wraps on a phone, where there is
     no width for eight controls in a row */
  .orow {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 10px;
    border-bottom: 1px solid var(--border);
  }

  @media (max-width: 700px) {
    .orow {
      flex-wrap: wrap;
    }
  }

  .orow:last-child {
    border-bottom: 0;
  }

  .ochip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    font: 11px/1 var(--mono);
    flex: none;
  }

  .un {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    flex: none;
  }

  /* the row's controls are the 28px scale of the shared primitives; the
     pixel count keeps the full 72px — it was unreadable at 52 (Jeremy) */
  .orow :global(.inp) {
    height: 28px;
    padding: 0 7px;
    font-size: 12px;
    flex: none;
  }

  .orow select {
    height: 28px;
    padding: 0 7px 0 8px;
    font-size: 12px;
    flex: none;
  }

  /* mockup S3j `.orow .btn.icon`: 24px square, with a matching hole on a row
     that has no remove */
  .orow .rm,
  .ospacer {
    width: 24px;
    height: 24px;
    flex: none;
  }

  /* the mockup's `.btn.quiet` drops the FILL, not the hairline */
  .orow .rm {
    border-color: var(--border);
  }

  .outputs > .btn {
    margin-top: 8px;
  }

  .range {
    margin-left: auto;
    font: 11px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
    flex: none;
  }

  /* mockup S3j `.arrsvg` — the picture is drawn at its own 640px scale and
     shrinks with the box rather than reflowing */
  .arrsvg {
    display: block;
    width: 100%;
    max-width: 640px;
    height: auto;
    margin: 0 auto;
  }

  .lbl {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 10px;
  }

  .dimtext {
    fill: var(--text-dim);
  }
</style>
