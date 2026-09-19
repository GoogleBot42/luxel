<script lang="ts">
  // The device's projection DEFAULTS (proposal §5.4d, mockups S3e–S3h).
  //
  // Three rules, all of them visibility rules:
  //  * only the pattern kinds that are NOT native to this Layout get a row —
  //    a 2D pattern on a matrix needs no projection, so that row does not
  //    exist rather than appearing greyed out;
  //  * each picker offers only the axes this fixture actually has;
  //  * a single-option case is a one-line NOTE, never a disabled control.
  //
  // Labels come from the ENGINE (`Luxel.projectionOptions`), so Settings, a
  // tile caption and the editor's quiet row caption a projection identically.
  import { createEventDispatcher } from "svelte";
  import type { Luxel } from "../lib/luxel";
  import type { Layout, PatternDims, Projection, ProjectionMode } from "../stores/geometry";
  import ProjectionCard from "./ProjectionCard.svelte";

  export let luxel: Luxel | null;
  export let layout: Layout;
  export let projection: Projection;
  export let active = false;

  const dispatch = createEventDispatcher<{ set: { dims: PatternDims; mode: ProjectionMode } }>();

  const ROW_LABEL: Record<number, string> = { 1: "1D patterns", 2: "2D patterns", 3: "3D patterns" };

  interface Row {
    dims: PatternDims;
    label: string;
    options: { mode: ProjectionMode; label: string }[];
    current: ProjectionMode;
  }

  /** One row per non-native pattern kind, from the engine's own table. */
  $: rows = buildRows(luxel, layout, projection);

  function currentFor(p: Projection, dims: number): ProjectionMode {
    return dims === 3 ? p.proj3d : dims === 2 ? p.proj2d : p.proj1d;
  }

  function buildRows(lx: Luxel | null, l: Layout, p: Projection): Row[] {
    if (!lx) return [];
    const out: Row[] = [];
    for (const dims of [1, 2, 3] as const) {
      const options = lx.projectionOptions(dims, l.dims);
      if (options.length === 0) continue; // native here — no row at all
      const first = options[0];
      const want = currentFor(p, dims);
      out.push({
        dims,
        label: ROW_LABEL[dims] ?? `${dims}D patterns`,
        options: options.map((o) => ({ mode: o.mode as ProjectionMode, label: o.label })),
        current: options.some((o) => o.mode === want) ? want : ((first?.mode ?? "index") as ProjectionMode),
      });
    }
    return out;
  }
</script>

{#if rows.length > 0}
  <div class="projection" data-role="projection-block">
    <div class="slabel">Projection</div>
    <p class="dim hint">
      How patterns made for another layout are shown on this one. A playlist item can override it.
    </p>
    {#each rows as row (row.dims)}
      <div class="prow" data-role="projection-kind" data-dims={row.dims}>
        <div class="prlab">{row.label}</div>
        {#if row.options.length === 1}
          <!-- one option is a fact, not a choice (§5.4d) -->
          <p class="dim hint" data-role="projection-only">
            Shown {row.options[0]?.label.toLowerCase()} — the only way this fixture can show them.
          </p>
        {:else}
          <div class="cards" style="--n:{Math.min(row.options.length, 3)}">
            {#each row.options as opt (opt.mode)}
              <ProjectionCard
                {luxel}
                {layout}
                {active}
                patternDims={row.dims}
                mode={opt.mode}
                label={opt.label}
                selected={row.current === opt.mode}
                on:click={() => dispatch("set", { dims: row.dims, mode: opt.mode })}
              />
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .projection {
    margin-top: 18px;
  }

  .prow {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
    margin-bottom: 14px;
  }

  .prlab {
    font-size: 12.5px;
    padding-top: 10px;
    color: var(--text-dim);
  }

  .cards {
    display: grid;
    /* minmax(0,…): a `1fr` track's implicit minimum is min-content, and one
       long label would then widen the track past its share (Gitea #467) */
    grid-template-columns: repeat(var(--n), minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 560px) {
    .prow {
      grid-template-columns: minmax(0, 1fr);
      gap: 6px;
    }

    .prlab {
      padding-top: 0;
    }

    .cards {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
