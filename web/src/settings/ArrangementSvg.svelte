<script lang="ts">
  // The arrangement picture (proposal §5.3, mockups S3c/S3d/S3k).
  //
  // "The graphic is the spec; prose can't describe a snaked 3×2 chain." So
  // this draws, from the same fields the form above it edits:
  //
  //  * `mode="chain"` — the tile grid, the chain path numbered from the IN
  //    connector, the scan direction each tile ends up with, the 180° markers,
  //    the resulting total size, and (with more than one output) each run in
  //    its own identity colour.
  //  * `editable` (a panel board, Gitea #920) — the same picture as the
  //    wall's EDITOR: each cell is a click target, and each panel's badge
  //    carries a pointer to the panel's own top — the arrow the device's
  //    Identify card draws on the real panel, so the picture and the wall
  //    read the same way. The tiles come from the device's `tiles` list
  //    (the explicit per-panel chain, or the rule it derived).
  //  * `mode="pixels"` — the SAME widget one level down: how the strip snakes
  //    through a single matrix's pixels. Identical question, identical
  //    picture, only the noun changes.
  import { createEventDispatcher } from "svelte";
  import {
    chainFromWire,
    chainOrder,
    pictureTurn,
    type ChainTile,
    type Corner,
    type RunDir,
    type WireTile,
  } from "../lib/settingsCaps";

  const dispatch = createEventDispatcher<{ pick: { cx: number; cy: number } }>();

  /** `chain` = tiles in a chain; `pixels` = the pixel run inside one matrix. */
  export let mode: "chain" | "pixels" = "chain";
  export let pw: number;
  export let ph: number;
  export let cols: number;
  export let rows: number;
  export let start: Corner;
  export let dir: RunDir;
  export let snake: boolean;
  /** Mount rotation in degrees clockwise, `[even lines, odd lines]` — tile 1's
   *  line is even (Gitea #917). Drawn on every turned tile as its degrees. */
  export let rot: readonly [number, number] = [0, 0];
  /** The device's chain, `[cx, cy, deg]` in ribbon order (`GET
   *  /api/layout` `matrix.tiles`, Gitea #920). `null` — or a list that does
   *  not cover the grid, i.e. a stale one mid-resize — falls back to the rule
   *  fields above, which is also all firmware older than #920 sends. */
  export let tiles: readonly WireTile[] | null = null;
  /** `tiles` is an explicit per-panel list rather than the rule's walk. */
  export let explicit = false;
  /** The picture is the editor: cells are buttons, badges point to each
   *  panel's top. */
  export let editable = false;
  /** The grid cell the editor is on, `[cx, cy]`. */
  export let selected: readonly [number, number] | null = null;
  /** Tiles per output, in chain order. `[]` / one entry = one undivided run. */
  export let outputCounts: readonly number[] = [];
  /** Leading tiles the board's framebuffer can actually shift out (`drive`,
   *  Gitea #475). Anything past it is DARK, and the picture says so rather
   *  than drawing four lit panels for a board that lights two. 0 = no limit. */
  export let drive = 0;

  /** Output identity colours (S3j: output 2's blue means "this wire" and
   *  appears nowhere else in the UI). */
  const OUT_COLORS = ["#e8a33d", "#5b9bd5", "#77b57a", "#c07ad0"];

  const VIEW_W = 420;
  const VIEW_H = 300;
  const PAD_L = 72;
  const PAD_T = 30;
  const PAD_R = 24;
  const PAD_B = 34;
  /** Pixel rows/columns are sampled down to this many drawn lines. */
  const MAX_LINES = 16;

  $: nc = Math.max(1, Math.round(cols) || 1);
  $: nr = Math.max(1, Math.round(rows) || 1);
  $: gw = Math.max(1, Math.round(pw));
  $: gh = Math.max(1, Math.round(ph));
  $: totalW = nc * gw;
  $: totalH = nr * gh;

  // every input is named here, so it re-runs when any of them moves
  $: rule = chainOrder(nc, nr, start, dir, snake, rot);
  $: chain = withShown(
    tiles !== null && tiles.length === nc * nr ? chainFromWire(tiles, rule, explicit) : rule,
    editable,
  );
  $: ownerOf = buildOwners(chain.length, outputCounts);
  $: multi = outputCounts.length > 1;
  /** In pixel mode the whole grid is ONE box; in chain mode it is nc×nr. */
  $: box = fit(mode === "chain" ? nc : 1, mode === "chain" ? nr : 1, totalW, totalH);
  $: cell =
    mode === "chain"
      ? box
      : { w: box.w, h: box.h, x0: box.x0, y0: box.y0 };
  // the editor's badge is a touch smaller: it carries a pointer, and the
  // tile's corner carries the `↻` label, on a 4×4 wall too
  // (capped, so a lone panel's badge does not swallow its tile)
  $: radius = editable ? Math.min(28, Math.min(cell.w, cell.h) * 0.2) : Math.min(cell.w, cell.h) * 0.24;
  $: showIndex = mode === "chain" && (editable || (radius >= 7 && chain.length > 1));
  /** The rotation label's size: 13px on a roomy cell, smaller on a crowded
   *  grid so `↻270°` stays inside its tile and clear of the badge. */
  $: rotFont = Math.max(8, Math.min(13, cell.w / 6.5));
  /** …and in the editor it is drawn only where it is still legible and
   *  clears the badge: on a crowded wall the badge's pointer already says
   *  which way the panel is turned, and the editor row says it in words. */
  $: rotFits = !editable || (rotFont >= 11 && 4 + rotFont * 1.1 + 2 <= cell.h / 2 - radius);
  $: runs = mode === "chain" ? splitRuns(chain, ownerOf) : [];
  $: pixelRun = mode === "pixels" ? pixelPath() : null;

  function buildOwners(n: number, counts: readonly number[]): number[] {
    const owners = new Array<number>(n).fill(0);
    if (counts.length <= 1) return owners;
    let at = 0;
    counts.forEach((c, o) => {
      for (let i = 0; i < Math.max(0, Math.round(c)) && at < n; i++) owners[at++] = o;
    });
    return owners;
  }

  /** One tile, as large as fits, with the real aspect ratio kept. */
  function fit(
    ncols: number,
    nrows: number,
    tw: number,
    th: number,
  ): { w: number; h: number; x0: number; y0: number } {
    const aspect = tw > 0 && th > 0 ? tw / ncols / (th / nrows) : 1;
    const availW = VIEW_W - PAD_L - PAD_R;
    const availH = VIEW_H - PAD_T - PAD_B;
    let w = availW / ncols;
    let h = w / aspect;
    if (h * nrows > availH) {
      h = availH / nrows;
      w = h * aspect;
    }
    return { w, h, x0: PAD_L + (availW - w * ncols) / 2, y0: PAD_T + (availH - h * nrows) / 2 };
  }

  const cx = (t: { col: number }): number => cell.x0 + (t.col + 0.5) * cell.w;
  const cy = (t: { row: number }): number => cell.y0 + (t.row + 0.5) * cell.h;

  /** One output's run through the tiles, and where its IN connector sits
   *  (`side` −1 = left of the head tile, +1 = right of it). */
  interface Run {
    color: string;
    pts: string;
    head: { x: number; y: number };
    side: -1 | 1;
  }

  /** A tile plus the turn the picture DRAWS for it. The editor speaks the
   *  PICTURE's rotation — the inverse of the mount the wire carries, and what
   *  the user watches the panel do (Gitea #920); the read-only picture of a
   *  strip-built matrix keeps its mount degrees (only ever 0/180, where the
   *  two agree anyway). */
  function withShown(list: readonly ChainTile[], picture: boolean): (ChainTile & { shown: number })[] {
    return list.map((t) => ({ ...t, shown: picture ? pictureTurn(t.turns) : t.turns }));
  }

  function isSel(t: ChainTile, sel: readonly [number, number] | null): boolean {
    return sel !== null && sel[0] === t.col && sel[1] === t.row;
  }

  function pickKey(e: KeyboardEvent, t: ChainTile): void {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    dispatch("pick", { cx: t.col, cy: t.row });
  }

  function splitRuns(
    list: readonly ChainTile[],
    owners: number[],
  ): Run[] {
    const out: Run[] = [];
    let cur: ChainTile[] = [];
    let owner = owners[0] ?? 0;
    const flush = (): void => {
      const head = cur[0];
      if (head === undefined) return;
      // The IN connector enters from the grid's left edge — or, when the
      // ribbon starts in the right-hand column of a wider grid, from its
      // right, so it never lands inside the neighbouring tile over its badge
      // (the editor only: the read-only picture is what the mockups pin).
      const fromRight = editable && nc > 1 && head.col === nc - 1;
      out.push({
        color: OUT_COLORS[owner % OUT_COLORS.length] ?? "#e8a33d",
        pts: cur.map((t) => `${cx(t).toFixed(1)},${cy(t).toFixed(1)}`).join(" "),
        head: { x: cx(head), y: cy(head) },
        side: fromRight ? 1 : -1,
      });
    };
    for (const t of list) {
      const o = owners[t.index] ?? 0;
      if (o !== owner) {
        flush();
        cur = [];
        owner = o;
      }
      cur.push(t);
    }
    flush();
    return out;
  }

  /**
   * The pixel run through ONE matrix, as a snaking polyline: `dir` says
   * whether it advances along rows or columns, `start` which corner it begins
   * at, `snake` whether alternate lines run backwards. Long grids are sampled
   * down to MAX_LINES drawn lines — the shape is the message, not the count.
   */
  function pixelPath(): { pts: string; head: { x: number; y: number } } {
    const byRow = dir === "row";
    const lines = Math.max(1, Math.min(MAX_LINES, byRow ? totalH : totalW));
    const fromRight = start === "tr" || start === "br";
    const fromBottom = start === "bl" || start === "br";
    const span = byRow ? cell.h : cell.w;
    const lo = byRow ? cell.x0 : cell.y0;
    const hi = lo + (byRow ? cell.w : cell.h);
    const pts: string[] = [];
    for (let i = 0; i < lines; i++) {
      const ordinal = byRow ? (fromBottom ? lines - 1 - i : i) : fromRight ? lines - 1 - i : i;
      const at = (byRow ? cell.y0 : cell.x0) + ((ordinal + 0.5) * span) / lines;
      const reversed = (byRow ? fromRight : fromBottom) !== (snake && i % 2 === 1);
      const ends = reversed ? [hi, lo] : [lo, hi];
      for (const e of ends) {
        pts.push(byRow ? `${e.toFixed(1)},${at.toFixed(1)}` : `${at.toFixed(1)},${e.toFixed(1)}`);
      }
    }
    const first = pts[0]?.split(",") ?? ["0", "0"];
    return { pts: pts.join(" "), head: { x: Number(first[0]), y: Number(first[1]) } };
  }
</script>

<div class="arrbox" data-role="arrangement" data-mode={mode}>
  <svg
    class="arrsvg"
    viewBox="0 0 {VIEW_W} {VIEW_H}"
    role="img"
    aria-label={mode === "chain"
      ? `${nc} by ${nr} panel chain, ${totalW} by ${totalH} pixels${editable ? " — click a panel to edit it" : ""}`
      : `pixel wiring of a ${totalW} by ${totalH} matrix`}
  >
    <defs>
      <marker
        id="arr-scan"
        viewBox="0 0 10 10"
        refX="9"
        refY="5"
        markerWidth="4.5"
        markerHeight="4.5"
        orient="auto-start-reverse"
      >
        <path d="M0 0 L10 5 L0 10 z" fill="#5f6775" />
      </marker>
    </defs>

    <!-- overall size, top and left -->
    <text x={cell.x0 + (cell.w * (mode === "chain" ? nc : 1)) / 2} y="13" class="dimtext" text-anchor="middle"
      >{totalW} px</text
    >
    <line x1={cell.x0} y1="21" x2={cell.x0 + cell.w * (mode === "chain" ? nc : 1)} y2="21" class="tick" />
    <line x1={cell.x0} y1="17" x2={cell.x0} y2="25" class="tick" />
    <line
      x1={cell.x0 + cell.w * (mode === "chain" ? nc : 1)}
      y1="17"
      x2={cell.x0 + cell.w * (mode === "chain" ? nc : 1)}
      y2="25"
      class="tick"
    />
    <text
      transform="translate({PAD_L - 30},{cell.y0 + (cell.h * (mode === 'chain' ? nr : 1)) / 2}) rotate(-90)"
      class="dimtext"
      text-anchor="middle">{totalH} px</text
    >
    <line
      x1={PAD_L - 22}
      y1={cell.y0}
      x2={PAD_L - 22}
      y2={cell.y0 + cell.h * (mode === "chain" ? nr : 1)}
      class="tick"
    />

    {#if mode === "chain"}
      {#each chain as t (t.index)}
        <rect
          x={cell.x0 + t.col * cell.w + 2}
          y={cell.y0 + t.row * cell.h + 2}
          width={Math.max(1, cell.w - 4)}
          height={Math.max(1, cell.h - 4)}
          rx="4"
          class="tile"
          class:dark={drive > 0 && t.index >= drive}
          class:sel={editable && isSel(t, selected)}
          style={multi && !(editable && isSel(t, selected))
            ? `stroke:${OUT_COLORS[(ownerOf[t.index] ?? 0) % OUT_COLORS.length]}`
            : undefined}
        />
        <!-- the scan arrow is a STRIP's fact (which way the pixel run goes
             through this tile); a panel's scan is its own, and what the
             editor draws instead is where the panel's top is -->
        {#if !editable}
          <line
            x1={cx(t) + (t.flipX ? 1 : -1) * cell.w * 0.22}
            y1={cy(t) + cell.h * 0.34}
            x2={cx(t) - (t.flipX ? 1 : -1) * cell.w * 0.22}
            y2={cy(t) + cell.h * 0.34}
            class="scan"
            marker-end="url(#arr-scan)"
          />
        {/if}
      {/each}
      {#each runs as r, i (i)}
        <polyline
          points={r.pts}
          fill="none"
          style="stroke:{r.color}"
          stroke-width="3"
          stroke-linejoin="round"
          stroke-linecap="round"
        />
        <rect
          x={r.head.x + r.side * (cell.w * 0.5 + 16) - 6}
          y={r.head.y - 6}
          width="12"
          height="12"
          rx="2"
          style="fill:{r.color}"
        />
        <line
          x1={r.head.x + r.side * (cell.w * 0.5 + 10)}
          y1={r.head.y}
          x2={r.head.x + r.side * cell.w * 0.5}
          y2={r.head.y}
          style="stroke:{r.color}"
          stroke-width="2.4"
        />
        {#if r.side < 0}
          <text x={r.head.x - cell.w * 0.5 - 26} y={r.head.y + 4} style="fill:{r.color}" class="inlbl">IN</text>
        {:else}
          <!-- the right margin is too narrow for the label beside the box -->
          <text x={r.head.x + cell.w * 0.5 + 16} y={r.head.y - 10} style="fill:{r.color}" class="inlbl mid"
            >IN</text
          >
        {/if}
      {/each}
      <!-- the turn labels go over the chain path, which can cross a corner -->
      {#each chain as t (t.index)}
        {#if t.shown !== 0 && rotFits}
          <text
            x={cell.x0 + t.col * cell.w + 2 + rotFont * 0.55}
            y={cell.y0 + t.row * cell.h + 4 + rotFont * 1.1}
            class="rot"
            style="font-size:{rotFont.toFixed(1)}px">↻{t.shown}°</text
          >
        {/if}
      {/each}
      {#if showIndex}
        {#each chain as t (t.index)}
          {#if editable}
            <!-- the badge's pointer: where the picture's TOP is on this
                 panel (upright = up, ↻90° = right, …) — it turns clockwise
                 with each press of ↻ rotate -->
            <path
              d="M0 {-(radius + 7)} L{-radius * 0.5} {-radius * 0.72} L{radius * 0.5} {-radius * 0.72} Z"
              transform="translate({cx(t).toFixed(1)},{cy(t).toFixed(1)}) rotate({t.shown})"
              class="top"
              class:sel={isSel(t, selected)}
            />
          {/if}
          <circle
            cx={cx(t)}
            cy={cy(t)}
            r={radius}
            class="knock"
            class:badge={editable}
            class:sel={editable && isSel(t, selected)}
          />
          <text
            x={cx(t)}
            y={cy(t) + radius * 0.38}
            class="idx"
            class:sel={editable && isSel(t, selected)}
            font-size={radius * 1.1}>{t.index + 1}</text
          >
        {/each}
      {/if}
      {#if editable}
        <!-- the hit targets, last so nothing drawn above can eat a click -->
        {#each chain as t (t.index)}
          <rect
            x={cell.x0 + t.col * cell.w + 2}
            y={cell.y0 + t.row * cell.h + 2}
            width={Math.max(1, cell.w - 4)}
            height={Math.max(1, cell.h - 4)}
            rx="4"
            class="hit"
            role="button"
            tabindex="0"
            aria-label="panel {t.index + 1}{t.shown ? `, picture turned ${t.shown}°` : ''}"
            aria-pressed={isSel(t, selected)}
            data-role="arr-cell-{t.col}-{t.row}"
            data-selected={isSel(t, selected) ? "" : undefined}
            data-panel={t.index + 1}
            data-turn={t.shown}
            on:click={() => dispatch("pick", { cx: t.col, cy: t.row })}
            on:keydown={(e) => pickKey(e, t)}
          />
        {/each}
      {/if}
    {:else if pixelRun}
      <rect x={cell.x0} y={cell.y0} width={cell.w} height={cell.h} rx="4" class="tile" />
      <polyline
        points={pixelRun.pts}
        fill="none"
        class="wire"
        stroke-width="2.5"
        stroke-linejoin="round"
        stroke-linecap="round"
      />
      <circle cx={pixelRun.head.x} cy={pixelRun.head.y} r="3.4" class="first" />
      <rect x={pixelRun.head.x - 26} y={pixelRun.head.y - 6} width="12" height="12" rx="2" class="first" />
      <text x={pixelRun.head.x - 30} y={pixelRun.head.y + 4} class="inlbl accent">IN</text>
    {/if}

    <text
      x={cell.x0 + (cell.w * (mode === "chain" ? nc : 1)) / 2}
      y={VIEW_H - 8}
      class="dimtext"
      text-anchor="middle"
    >
      {#if mode === "chain"}
        {chain.length} panel{chain.length === 1 ? "" : "s"} · {totalW}×{totalH} px{drive > 0 &&
        drive < chain.length
          ? ` · ${chain.length - drive} dark`
          : ""}
      {:else}
        {totalW * totalH} px, {dir === "row" ? "row by row" : "column by column"}
      {/if}
    </text>
  </svg>
</div>

<style>
  /* `.arrbox` itself is the shared box in settings/cards.css (mockup S3k) */
  .arrsvg {
    display: block;
    width: 100%;
    max-width: 420px;
    height: auto;
    margin: 0 auto;
  }

  .tile {
    fill: #171a20;
    stroke: var(--border);
  }

  /* past `drive`: the board stores this tile but cannot shift it out */
  .tile.dark {
    fill: #0c0e12;
    stroke-dasharray: 4 3;
    opacity: 0.55;
  }

  .tick {
    stroke: var(--border);
    stroke-width: 1;
  }

  .scan {
    stroke: #5f6775;
    stroke-width: 1.2;
  }

  .wire,
  .first {
    stroke: var(--accent);
    fill: none;
  }

  .first {
    fill: var(--accent);
  }

  .dimtext,
  .idx,
  .rot,
  .inlbl {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    fill: var(--text-dim);
  }

  .inlbl.accent {
    fill: var(--accent);
  }

  .dimtext {
    font-size: 11px;
  }

  .knock {
    fill: #171a20;
  }

  /* the editor's selected cell (Gitea #920): accent outline and a tinted
     fill on the tile, and the badge inverted, so it reads at 16 cells too */
  .tile.sel {
    stroke: var(--accent);
    stroke-width: 2.5;
    fill: color-mix(in srgb, var(--accent) 14%, #171a20);
  }

  /* in the editor the knockout is a BADGE — ringed, so the pointer to the
     panel's top reads as part of it rather than as a stray triangle */
  .knock.badge {
    stroke: #5f6775;
    stroke-width: 1.2;
  }

  .knock.sel {
    fill: var(--accent);
    stroke: var(--accent);
  }

  .idx.sel {
    fill: #171a20;
    font-weight: 700;
  }

  .top {
    fill: var(--text-dim);
  }

  .top.sel {
    fill: var(--accent);
  }

  /* the browser's own focus ring would outline the cell in white over the
     selection's accent; keyboard focus gets the dashed stroke below */
  .hit {
    fill: transparent;
    stroke: none;
    cursor: pointer;
    outline: none;
  }

  .hit:hover {
    fill: rgba(255, 255, 255, 0.04);
  }

  .hit:focus-visible {
    outline: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 3 3;
  }

  .idx {
    text-anchor: middle;
    fill: var(--text-dim);
  }

  .rot {
    font-size: 13px;
  }

  .inlbl {
    font-size: 10px;
    text-anchor: end;
  }

  .inlbl.mid {
    text-anchor: middle;
  }
</style>
