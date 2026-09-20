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
  //  * `mode="pixels"` — the SAME widget one level down: how the strip snakes
  //    through a single matrix's pixels. Identical question, identical
  //    picture, only the noun changes.
  import { chainOrder, type Corner, type RunDir } from "../lib/settingsCaps";

  /** `chain` = tiles in a chain; `pixels` = the pixel run inside one matrix. */
  export let mode: "chain" | "pixels" = "chain";
  export let pw: number;
  export let ph: number;
  export let cols: number;
  export let rows: number;
  export let start: Corner;
  export let dir: RunDir;
  export let snake: boolean;
  export let rot180 = false;
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

  $: tiles = chainOrder(nc, nr, start, dir, snake);
  $: ownerOf = buildOwners(tiles.length, outputCounts);
  $: multi = outputCounts.length > 1;
  /** In pixel mode the whole grid is ONE box; in chain mode it is nc×nr. */
  $: box = fit(mode === "chain" ? nc : 1, mode === "chain" ? nr : 1, totalW, totalH);
  $: cell =
    mode === "chain"
      ? box
      : { w: box.w, h: box.h, x0: box.x0, y0: box.y0 };
  $: radius = Math.min(cell.w, cell.h) * 0.24;
  $: showIndex = mode === "chain" && radius >= 7 && tiles.length > 1;
  $: runs = mode === "chain" ? splitRuns(tiles, ownerOf) : [];
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

  function splitRuns(
    list: ReturnType<typeof chainOrder>,
    owners: number[],
  ): { color: string; pts: string; head: { x: number; y: number } }[] {
    const out: { color: string; pts: string; head: { x: number; y: number } }[] = [];
    let cur: typeof list = [];
    let owner = owners[0] ?? 0;
    const flush = (): void => {
      const head = cur[0];
      if (head === undefined) return;
      out.push({
        color: OUT_COLORS[owner % OUT_COLORS.length] ?? "#e8a33d",
        pts: cur.map((t) => `${cx(t).toFixed(1)},${cy(t).toFixed(1)}`).join(" "),
        head: { x: cx(head), y: cy(head) },
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
      ? `${nc} by ${nr} panel chain, ${totalW} by ${totalH} pixels`
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
      {#each tiles as t (t.index)}
        <rect
          x={cell.x0 + t.col * cell.w + 2}
          y={cell.y0 + t.row * cell.h + 2}
          width={Math.max(1, cell.w - 4)}
          height={Math.max(1, cell.h - 4)}
          rx="4"
          class="tile"
          class:dark={drive > 0 && t.index >= drive}
          style={multi
            ? `stroke:${OUT_COLORS[(ownerOf[t.index] ?? 0) % OUT_COLORS.length]}`
            : undefined}
        />
        <line
          x1={cx(t) + (t.flipX ? 1 : -1) * cell.w * 0.22}
          y1={cy(t) + cell.h * 0.34}
          x2={cx(t) - (t.flipX ? 1 : -1) * cell.w * 0.22}
          y2={cy(t) + cell.h * 0.34}
          class="scan"
          marker-end="url(#arr-scan)"
        />
        {#if rot180 && t.line % 2 === 1}
          <text x={cell.x0 + t.col * cell.w + 9} y={cell.y0 + t.row * cell.h + 20} class="rot">↻</text>
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
        <rect x={r.head.x - cell.w * 0.5 - 22} y={r.head.y - 6} width="12" height="12" rx="2" style="fill:{r.color}" />
        <line
          x1={r.head.x - cell.w * 0.5 - 10}
          y1={r.head.y}
          x2={r.head.x - cell.w * 0.5}
          y2={r.head.y}
          style="stroke:{r.color}"
          stroke-width="2.4"
        />
        <text x={r.head.x - cell.w * 0.5 - 26} y={r.head.y + 4} style="fill:{r.color}" class="inlbl">IN</text>
      {/each}
      {#if showIndex}
        {#each tiles as t (t.index)}
          <circle cx={cx(t)} cy={cy(t)} r={radius} class="knock" />
          <text x={cx(t)} y={cy(t) + radius * 0.38} class="idx" font-size={radius * 1.1}>{t.index + 1}</text>
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
        {tiles.length} panel{tiles.length === 1 ? "" : "s"} · {totalW}×{totalH} px{drive > 0 &&
        drive < tiles.length
          ? ` · ${tiles.length - drive} dark`
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
</style>
