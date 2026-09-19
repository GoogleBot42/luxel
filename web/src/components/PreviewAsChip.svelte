<script lang="ts">
  // The playground's Layout control (proposal §4, mockup S5) and the ONE new
  // visible control in Gitea #463: "Preview as  64×64 matrix ▾".
  //
  // Shape only — no wiring. Serpentine, start corner and outputs describe
  // hardware and live in the console's LED layout settings; the playground's
  // "By index" is row-major by definition. Everything on the page (tiles,
  // thumbnails, the editor's preview) renders through what this picks,
  // because they all read `stores/geometry.ts`.
  import {
    AUTO_LATTICE,
    AUTO_MATRIX,
    DEFAULT_STRIP_PIXELS,
    layoutName,
    previewAs,
    setPreviewAs,
  } from "../stores/geometry";

  let open = false;
  let wrapEl: HTMLElement;

  /** Click anywhere outside the chip closes it (no click handler on the
   *  popover itself, which would need a keyboard twin to be accessible). */
  function onWindowClick(e: MouseEvent): void {
    if (open && wrapEl && !wrapEl.contains(e.target as Node)) open = false;
  }

  // The numbers the rows hold, seeded from the current choice so switching
  // back and forth keeps what you typed.
  let px = $previewAs.mode === "strip" ? $previewAs.pixels : DEFAULT_STRIP_PIXELS;
  let w = $previewAs.mode === "matrix" ? $previewAs.w : AUTO_MATRIX;
  let h = $previewAs.mode === "matrix" ? $previewAs.h : AUTO_MATRIX;
  let n = $previewAs.mode === "lattice" ? $previewAs.n : AUTO_LATTICE;

  $: mode = $previewAs.mode;

  const clamp = (v: number, lo: number, hi: number): number =>
    Math.max(lo, Math.min(hi, Math.round(Number(v) || lo)));

  function num(e: Event): number {
    return Number((e.target as HTMLInputElement).value);
  }

  function chooseStrip(): void {
    px = clamp(px, 1, 4096);
    setPreviewAs({ mode: "strip", pixels: px });
  }

  function chooseMatrix(): void {
    w = clamp(w, 1, 256);
    h = clamp(h, 1, 256);
    setPreviewAs({ mode: "matrix", w, h });
  }

  function chooseLattice(): void {
    n = clamp(n, 2, 32);
    setPreviewAs({ mode: "lattice", n });
  }
</script>

<svelte:window on:click={onWindowClick} />

<span class="wrap" bind:this={wrapEl}>
  <button
    class="chip"
    class:open
    data-role="preview-as"
    title="what this playground previews on — every tile and preview uses it"
    on:click={() => (open = !open)}
  >
    Preview as
    <!-- under Auto each pattern gets its own shape, so say so: the label is
         then what the pattern in the editor resolved to, not a page-wide rig -->
    {#if mode === "auto"}<span class="dim">Auto ·</span>{/if}
    <span class="mono" data-role="preview-as-label">{$layoutName}</span>
    <span class="caret">▾</span>
  </button>
  {#if open}
    <div class="pop" data-role="preview-as-menu">
      <button
        class="row"
        class:on={mode === "auto"}
        role="menuitemradio"
        aria-checked={mode === "auto"}
        data-role="preview-as-auto"
        on:click={() => setPreviewAs({ mode: "auto" })}
      >
        <span class="radio"></span>
        Auto — follow the pattern <span class="dim mono">3D›2D›1D</span>
      </button>

      <div class="row" class:on={mode === "strip"}>
        <button
          class="pick"
          role="menuitemradio"
          aria-checked={mode === "strip"}
          data-role="preview-as-strip"
          on:click={chooseStrip}
        >
          <span class="radio"></span> Strip
        </button>
        <input
          class="num"
          data-role="preview-as-px"
          type="number"
          min="1"
          max="4096"
          value={px}
          on:change={(e) => {
            px = num(e);
            chooseStrip();
          }}
        />
        <span class="dim">px</span>
      </div>

      <div class="row" class:on={mode === "matrix"}>
        <button
          class="pick"
          role="menuitemradio"
          aria-checked={mode === "matrix"}
          data-role="preview-as-matrix"
          on:click={chooseMatrix}
        >
          <span class="radio"></span> Matrix
        </button>
        <input
          class="num"
          data-role="preview-as-w"
          type="number"
          min="1"
          max="256"
          value={w}
          on:change={(e) => {
            w = num(e);
            chooseMatrix();
          }}
        />
        <span class="dim">×</span>
        <input
          class="num"
          data-role="preview-as-h"
          type="number"
          min="1"
          max="256"
          value={h}
          on:change={(e) => {
            h = num(e);
            chooseMatrix();
          }}
        />
      </div>

      <div class="row" class:on={mode === "lattice"}>
        <button
          class="pick"
          role="menuitemradio"
          aria-checked={mode === "lattice"}
          data-role="preview-as-lattice"
          on:click={chooseLattice}
        >
          <span class="radio"></span> 3D lattice
        </button>
        <input
          class="num"
          data-role="preview-as-n"
          type="number"
          min="2"
          max="32"
          value={n}
          on:change={(e) => {
            n = num(e);
            chooseLattice();
          }}
        />
        <span class="dim">³</span>
      </div>

      <button
        class="row"
        class:on={mode === "map"}
        role="menuitemradio"
        aria-checked={mode === "map"}
        data-role="preview-as-map"
        on:click={() => setPreviewAs({ mode: "map", pixels: px })}
      >
        <span class="radio"></span>
        Custom map program →
      </button>

      <p class="foot">
        Every preview and tile on this page uses this layout. On a device it is the device's own.
      </p>
    </div>
  {/if}
</span>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-inset);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
  }

  .chip:hover,
  .chip.open {
    border-color: var(--accent);
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }

  .caret {
    color: var(--text-dim);
    font-size: 10px;
  }

  .pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    min-width: 280px;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-panel);
    box-shadow: 0 8px 24px rgb(0 0 0 / 45%);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }

  .row:hover {
    background: var(--bg-inset);
  }

  .pick {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font-size: inherit;
    cursor: pointer;
  }

  .radio {
    display: inline-block;
    width: 11px;
    height: 11px;
    flex: none;
    border: 1px solid var(--text-dim);
    border-radius: 50%;
  }

  .row.on .radio {
    border-color: var(--accent);
    background:
      radial-gradient(circle, var(--accent) 0 45%, transparent 46%);
  }

  .row.on {
    color: var(--accent);
  }

  .num {
    width: 54px;
    padding: 2px 4px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .dim {
    color: var(--text-dim);
  }

  .mono {
    font-family: ui-monospace, Menlo, Consolas, monospace;
  }

  .foot {
    margin: 6px 4px 2px;
    color: var(--text-dim);
    font-size: 11px;
    line-height: 1.4;
  }
</style>
