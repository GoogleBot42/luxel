<script lang="ts">
  // The playground's Layout control (proposal §4, mockup S5) and the ONE new
  // visible control in Gitea #463: "Preview as  64×64 matrix ▾".
  //
  // Shape only — no wiring. Serpentine, start corner and outputs describe
  // hardware and live in the console's LED layout settings; the playground's
  // "By index" is row-major by definition. Everything on the page (tiles,
  // thumbnails, the editor's preview) renders through what this picks,
  // because they all read `stores/geometry.ts`.
  import { createEventDispatcher } from "svelte";
  import Popover from "./Popover.svelte";
  import {
    AUTO_LATTICE,
    AUTO_MATRIX,
    DEFAULT_STRIP_PIXELS,
    layoutName,
    previewAs,
    setPreviewAs,
  } from "../stores/geometry";

  /** "Custom map program" is not a shape you type a number into — it is a
   *  program, and it has a screen (A10, Gitea #471). The chip picks the
   *  choice and asks the shell to open that screen. */
  const dispatch = createEventDispatcher<{ openmap: void }>();

  let open = false;
  /** The chip the chooser hangs off (components/Popover.svelte). */
  let chipEl: HTMLElement;

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

<span class="wrap">
  <button
    class="btn chip"
    bind:this={chipEl}
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
  <Popover
    {open}
    anchor={chipEl}
    kind="pop"
    ariaRole="dialog"
    dataRole="preview-as-menu"
    on:close={() => (open = false)}
  >
    <button
      class="pr"
      class:on={mode === "auto"}
      role="menuitemradio"
      aria-checked={mode === "auto"}
      data-role="preview-as-auto"
      on:click={() => setPreviewAs({ mode: "auto" })}
    >
      <span class="radio"></span>
      Auto — follow the pattern <span class="dim mono">3D›2D›1D</span>
    </button>

    <div class="pr" class:on={mode === "strip"}>
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
        class="inp xs num"
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

    <div class="pr" class:on={mode === "matrix"}>
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
        class="inp xs num"
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
        class="inp xs num"
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

    <div class="pr" class:on={mode === "lattice"}>
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
        class="inp xs num"
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
      class="pr"
      class:on={mode === "map"}
      role="menuitemradio"
      aria-checked={mode === "map"}
      data-role="preview-as-map"
      on:click={() => {
        setPreviewAs({ mode: "map", pixels: px });
        open = false;
        dispatch("openmap"); // the program that makes the points has a screen
      }}
    >
      <span class="radio"></span>
      Custom map program →
    </button>

  <p class="popfoot">
    Every preview and tile on this page uses this layout. On a device it is the device's own.
  </p>
  </Popover>
</span>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
  }

  /* mockup S5: the layout chooser is a `.btn` in the accent's own tint — the
     one control in the header that is not neutral chrome */
  .chip {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .caret {
    color: var(--text-dim);
    font-size: 10px;
  }

  /* the radio row's clickable half (the label); the row itself carries the
     global `.pop .pr` looks */
  .pick {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .dim {
    color: var(--text-dim);
  }

  .mono {
    font-family: var(--mono);
  }
</style>
