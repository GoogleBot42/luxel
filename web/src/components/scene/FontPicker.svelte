<script lang="ts">
  // The font picker — mockup S7i, and the ONLY font UI in the app (§5.6:
  // there is no Settings › Fonts group).
  //
  // "Every entry shows the same sample rendered in that face at 1:1 device
  // pixels, so the size difference is the thing you compare, not the names"
  // (S7i's note). The samples are therefore not drawings: each one is
  // `luxel_core::text::draw` run through the wasm compositor on a grid the
  // size of the sample, so what the list shows is literally what the device
  // will put on the panel. Hand-drawn bitmaps would go stale the first time a
  // font blob changed.
  //
  // There is no `Upload…` row and no "custom fonts" section: user fonts are
  // filed and not planned (#487), and an affordance for one would be a
  // promise the device cannot keep.
  import { createEventDispatcher, onDestroy } from "svelte";
  import Popover from "../Popover.svelte";
  import { emptyScene, newLayer, serializeScene, FONTS, type SceneFont } from "../../lib/scene";
  import { luxel } from "../../stores/pattern";

  export let value: SceneFont = "regular";
  /** The layout's width, for the "~N chars wide" line (S7i). */
  export let gridW = 64;

  const dispatch = createEventDispatcher<{ input: SceneFont }>();

  let open = false;
  let btn: HTMLElement | null = null;

  /** The mock's own labels and provenance lines (S7i). */
  const LABEL: Record<SceneFont, string> = {
    tiny: "4×6 tiny",
    regular: "5×7 regular",
    large: "5×8 large",
  };
  const FAMILY: Record<SceneFont, string> = {
    tiny: "Tom Thumb",
    regular: "misc-fixed",
    large: "Spleen",
  };
  /** `luxel_core::text::Font::advance()` — cell width + 1. */
  const ADVANCE: Record<SceneFont, number> = { tiny: 4, regular: 6, large: 6 };
  /** The sample box S7i draws, in device pixels (the SVGs are these at 2×). */
  const ROWS: Record<SceneFont, number> = { tiny: 6, regular: 8, large: 9 };

  /** The sample: the clock the mock's own text layer is showing (S7/S7c). */
  const SAMPLE = "12:48";
  /** `--text`, the colour the mock fills its glyph rects with. */
  const INK = "d7dae0";

  const sampleW = (f: SceneFont): number => ADVANCE[f] * SAMPLE.length - 1;

  $: chars = (f: SceneFont): number => Math.floor(gridW / ADVANCE[f]);

  /** One sample, painted through the real font blobs. Returns null on a wasm
   *  build without the compositor — the row then shows its names alone,
   *  which is still a usable list. */
  function sample(font: SceneFont): Uint8Array | null {
    const lx = $luxel;
    if (!lx) return null;
    const w = sampleW(font);
    const h = ROWS[font];
    const comp = lx.compositor(w, h);
    if (!comp) return null;
    try {
      const layer = newLayer("text");
      if (layer.body.kind !== "text") return null;
      layer.style.rect = { x: 0, y: 0, w: 0, h: 0 };
      layer.body.text = {
        ...layer.body.text,
        source: { kind: "lit", text: SAMPLE },
        font,
        color: INK,
        align: "l",
        scroll: "none",
        speed: 0,
      };
      const scene = { ...emptyScene(), layers: [layer] };
      if (comp.setScene(serializeScene(scene)) !== null) return null;
      comp.setText(0, SAMPLE);
      return comp.frame(0);
    } finally {
      comp.free();
    }
  }

  /** Paint a sample into its canvas at 1:1 device pixels, scaled 2× the way
   *  S7i draws it (`shape-rendering:crispEdges` there, `pixelated` here). */
  function paint(node: HTMLCanvasElement, font: SceneFont): { destroy(): void } {
    const px = sample(font);
    const ctx = node.getContext("2d");
    if (px && ctx) {
      const w = sampleW(font);
      const h = ROWS[font];
      const img = ctx.createImageData(w, h);
      for (let i = 0; i < w * h; i++) {
        img.data[i * 4] = px[i * 3] ?? 0;
        img.data[i * 4 + 1] = px[i * 3 + 1] ?? 0;
        img.data[i * 4 + 2] = px[i * 3 + 2] ?? 0;
        // black is the background, not ink: the compositor black-keys text
        img.data[i * 4 + 3] = (px[i * 3] ?? 0) + (px[i * 3 + 1] ?? 0) + (px[i * 3 + 2] ?? 0) > 0 ? 255 : 0;
      }
      ctx.putImageData(img, 0, 0);
    }
    return { destroy(): void {} };
  }

  function choose(f: SceneFont): void {
    open = false;
    dispatch("input", f);
  }

  onDestroy(() => (open = false));
</script>

<button
  class="sel wide"
  class:open
  bind:this={btn}
  data-role="scene-text-font"
  data-value={value}
  aria-haspopup="menu"
  aria-expanded={open}
  on:click={() => (open = !open)}>{LABEL[value]}</button
>

<Popover {open} anchor={btn} align="start" dataRole="scene-font-menu" on:close={() => (open = false)}>
  <div class="full-inner">
    {#each FONTS as f (f)}
      <button
        class="mi fontrow"
        class:hi={f === value}
        data-role={`scene-font-${f}`}
        on:click={() => choose(f)}
      >
        <span class="fmeta">
          <span class="fnm">{LABEL[f]}</span>
          <span class="hint">{FAMILY[f]} · {f === value ? "current" : `~${chars(f)} chars wide`}</span>
        </span>
        <canvas
          class="glyph"
          width={sampleW(f)}
          height={ROWS[f]}
          style={`width:${sampleW(f) * 2}px;height:${ROWS[f] * 2}px`}
          aria-hidden="true"
          use:paint={f}
        ></canvas>
      </button>
    {/each}
  </div>
</Popover>

<style>
  /* the mock's `.fontrow` / `.fmeta` / `.fnm` / `.glyph` (mockups.html
     :2623-2626) — the row is a button because it picks a font */
  .fontrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .fontrow .fmeta {
    flex: 1;
    min-width: 0;
    display: block;
    text-align: left;
  }

  /* the mock's highlighted row (`.menu .mi.hi`, mockups.html :121) */
  .fontrow.hi {
    background: rgba(255, 255, 255, 0.05);
  }

  .fontrow .fnm {
    display: block;
    font-size: 13px;
  }

  .fontrow .hint {
    display: block;
  }

  .glyph {
    display: block;
    flex: none;
    /* an `<svg>` clips with `overflow:hidden`; a `<canvas>` would compute
       `clip` and read as a delta against the mock's sample */
    overflow: hidden;
    image-rendering: pixelated;
  }
</style>
