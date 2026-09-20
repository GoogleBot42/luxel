<script lang="ts">
  // A real colour picker for the `hsvPicker`/`rgbPicker` controls (Gitea #538,
  // Jeremy 2026-09-19: "should not be asking the user to input raw hsv
  // numbers … do not use the browser color picker").
  //
  // Shape: the mock's swatch (S2 `.swatches i`, 26×22) opens the shared
  // `.pop` popover holding a saturation/value field, a hue strip, and direct
  // numeric entry in BOTH spaces plus hex — because "put in values directly"
  // was the other half of the request, and a designer knows the hex while a
  // pattern author knows the hue.
  //
  // It is deliberately generic (kind + a unit triple in, a unit triple out)
  // rather than control-aware: the palette editor (#537) needs exactly this
  // widget per stop.
  //
  // WHAT IT EMITS is the control's own space, unrounded — `hsv` in, `hsv`
  // out; `rgb` in, `rgb` out — so the 16.16 values pushed to the device are
  // the same numbers the three raw sliders used to produce. Only the DISPLAY
  // converts.
  import { createEventDispatcher } from "svelte";
  import Popover from "./Popover.svelte";
  import {
    cssRgb,
    hexToRgb,
    hsvToRgb,
    rgbToHex,
    rgbToHsv,
    wrapHue,
    type Hsv,
    type Rgb,
  } from "../lib/color";

  /** Which space the caller's triple is in — and the one it gets back. */
  export let kind: "hsv" | "rgb" = "hsv";
  /** The control's three channels, 0..1. */
  export let value: number[] = [];
  /** The control's name, for the button's accessible label. */
  export let label = "colour";
  /** Dim the swatch: the value shown is a placeholder, not the running one
   *  (Controls.svelte's `isGuess`). */
  export let dim = false;

  const dispatch = createEventDispatcher<{ input: number[] }>();

  const clamp01 = (v: number): number => (v < 0 ? 0 : v > 1 ? 1 : v);
  /** The same fallback the raw rows used: hue 0, full saturation and value. */
  const DEFAULTS: Rgb = [0, 1, 1];

  let open = false;
  let anchor: HTMLElement | null = null;
  let field: HTMLElement | null = null;
  /** Hue is undefined for a grey, so remember the last one the user aimed at
   *  — otherwise dragging saturation to zero snaps the wheel back to red and
   *  dragging out again gives a different colour than you started with. */
  let hueMem = 0;
  /** Typed hex, kept while it is half-written (`#3e` moves nothing). */
  let hexDraft = "";
  let hexFocus = false;

  $: raw = [value[0] ?? DEFAULTS[0], value[1] ?? DEFAULTS[1], value[2] ?? DEFAULTS[2]] as Rgb;
  $: hsv = (
    kind === "hsv"
      ? [wrapHue(raw[0]), clamp01(raw[1]), clamp01(raw[2])]
      : withHue(rgbToHsv(raw), hueMem)
  ) as Hsv;
  $: rgb = (kind === "rgb" ? raw.map(clamp01) : hsvToRgb(hsv)) as Rgb;
  $: css = cssRgb(rgb);
  $: hex = rgbToHex(rgb);
  $: hueCss = cssRgb(hsvToRgb([hsv[0], 1, 1]));

  function withHue(h: Hsv, mem: number): Hsv {
    return [h[1] === 0 ? mem : h[0], h[1], h[2]];
  }

  /** Every edit funnels through here: one place converts, one place emits. */
  function apply(next: Hsv): void {
    const h = wrapHue(next[0]);
    hueMem = h;
    const s = clamp01(next[1]);
    const v = clamp01(next[2]);
    dispatch("input", kind === "hsv" ? [h, s, v] : hsvToRgb([h, s, v]));
  }

  function applyRgb(next: Rgb): void {
    const c = next.map(clamp01) as Rgb;
    const h = rgbToHsv(c);
    if (h[1] > 0) hueMem = h[0];
    if (kind === "rgb") dispatch("input", c);
    else apply(withHue(h, hueMem));
  }

  // ---- the saturation/value field ----

  function fromPoint(e: PointerEvent): void {
    if (!field) return;
    const r = field.getBoundingClientRect();
    apply([hsv[0], clamp01((e.clientX - r.left) / r.width), clamp01(1 - (e.clientY - r.top) / r.height)]);
  }

  function onFieldDown(e: PointerEvent): void {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    fromPoint(e);
  }

  function onFieldMove(e: PointerEvent): void {
    if ((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId)) fromPoint(e);
  }

  /** Arrows walk the field, so the picker is usable without a pointer; shift
   *  takes the coarse step. */
  function onFieldKey(e: KeyboardEvent): void {
    const step = e.shiftKey ? 0.1 : 0.01;
    const d: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, step],
      ArrowDown: [0, -step],
    };
    const m = d[e.key];
    if (!m) return;
    e.preventDefault();
    apply([hsv[0], hsv[1] + m[0], hsv[2] + m[1]]);
  }

  // ---- direct entry ----

  const num = (e: Event): number => Number((e.target as HTMLInputElement).value);

  function setHsvChannel(i: number, e: Event): void {
    const v = num(e);
    if (Number.isNaN(v)) return;
    const next: Hsv = [hsv[0], hsv[1], hsv[2]];
    next[i] = v;
    apply(next);
  }

  function setRgbChannel(i: number, e: Event): void {
    const v = num(e);
    if (Number.isNaN(v)) return;
    const next: Rgb = [rgb[0], rgb[1], rgb[2]];
    next[i] = v / 255;
    applyRgb(next);
  }

  function onHex(e: Event): void {
    hexDraft = (e.target as HTMLInputElement).value;
    const c = hexToRgb(hexDraft);
    if (c) applyRgb(c);
  }

  const f3 = (v: number): string => v.toFixed(3);
  const b255 = (v: number): number => Math.round(clamp01(v) * 255);
</script>

<span class="wrap">
  <button
    class="swatch"
    class:dim
    bind:this={anchor}
    data-role="color-swatch"
    data-value={hex}
    style={`background:${css}`}
    title={`${label} — ${hex}`}
    aria-label={`${label}, ${hex}`}
    aria-expanded={open}
    on:click|stopPropagation={() => (open = !open)}
  ></button>

  <!-- hung from the swatch's LEFT edge: a 296px chooser right-aligned to a
       26px button reaches back across the code column for no reason -->
  <Popover
    {open}
    {anchor}
    kind="pop"
    align="start"
    ariaRole="dialog"
    dataRole="color-pop"
    on:close={() => (open = false)}
  >
    <!-- saturation across, value up: the arrangement every picker uses, so
         nobody has to learn this one -->
    <div
      class="field"
      bind:this={field}
      data-role="color-field"
      role="slider"
      tabindex="0"
      aria-label="saturation and value"
      aria-valuetext={`saturation ${f3(hsv[1])}, value ${f3(hsv[2])}`}
      aria-valuenow={hsv[1]}
      aria-valuemin={0}
      aria-valuemax={1}
      style={`--hue:${hueCss}`}
      on:pointerdown={onFieldDown}
      on:pointermove={onFieldMove}
      on:keydown={onFieldKey}
    >
      <i class="dot" style={`left:${hsv[1] * 100}%;top:${(1 - hsv[2]) * 100}%;background:${css}`}></i>
    </div>

    <!-- the wheel laid flat. The gradient is the WRAPPER's: a range input's
         own background is painted over by its native track. -->
    <div class="huewrap">
      <input
        class="hue"
        type="range"
        min="0"
        max="1"
        step="0.001"
        data-role="color-hue"
        aria-label="hue"
        value={hsv[0]}
        on:input={(e) => apply([num(e), hsv[1], hsv[2]])}
      />
    </div>

    <div class="rows">
      <div class="row" data-role="color-hsv">
        <span class="k">HSV</span>
        {#each [0, 1, 2] as i}
          <input
            class="inp xs n"
            type="number"
            min="0"
            max="1"
            step="0.001"
            data-role={`color-hsv-${i}`}
            aria-label={`${["hue", "saturation", "value"][i]}`}
            value={f3(hsv[i] ?? 0)}
            on:change={(e) => setHsvChannel(i, e)}
          />
        {/each}
      </div>
      <div class="row" data-role="color-rgb">
        <span class="k">RGB</span>
        {#each [0, 1, 2] as i}
          <input
            class="inp xs n"
            type="number"
            min="0"
            max="255"
            step="1"
            data-role={`color-rgb-${i}`}
            aria-label={`${["red", "green", "blue"][i]}`}
            value={b255(rgb[i] ?? 0)}
            on:change={(e) => setRgbChannel(i, e)}
          />
        {/each}
      </div>
      <div class="row">
        <span class="k">Hex</span>
        <input
          class="inp xs hex"
          type="text"
          spellcheck="false"
          data-role="color-hex"
          aria-label="hex"
          value={hexFocus ? hexDraft : hex}
          on:focus={() => {
            hexFocus = true;
            hexDraft = hex;
          }}
          on:blur={() => (hexFocus = false)}
          on:input={onHex}
        />
        <!-- what you are about to get, beside what you typed -->
        <i class="live" style={`background:${css}`}></i>
      </div>
    </div>
  </Popover>
</span>

<style>
  .wrap {
    display: inline-flex;
  }

  /* mockup S2 `.swatches i` */
  .swatch {
    width: 26px;
    height: 22px;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 4px;
    cursor: pointer;
  }

  .swatch.dim {
    opacity: 0.4;
  }

  .field {
    position: relative;
    height: 132px;
    margin: 2px 2px 8px;
    border-radius: 6px;
    /* first listed paints on top: value down, saturation across, hue under */
    background:
      linear-gradient(to top, #000, rgba(0, 0, 0, 0)),
      linear-gradient(to right, #fff, rgba(255, 255, 255, 0)),
      var(--hue);
    touch-action: none;
    cursor: crosshair;
  }

  .field:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .dot {
    position: absolute;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border: 2px solid #fff;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }

  .huewrap {
    height: 14px;
    margin: 0 2px 8px;
    border-radius: 7px;
    /* the same six stops hsv() walks through */
    background: linear-gradient(
      to right,
      #f00 0%,
      #ff0 16.66%,
      #0f0 33.33%,
      #0ff 50%,
      #00f 66.66%,
      #f0f 83.33%,
      #f00 100%
    );
  }

  /* a native range keeps the keyboard behaviour; only its paint is ours */
  .hue {
    display: block;
    width: 100%;
    height: 14px;
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    appearance: none;
    -webkit-appearance: none;
  }

  .hue::-webkit-slider-runnable-track {
    height: 14px;
    background: transparent;
  }

  .hue::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 10px;
    height: 18px;
    margin-top: -2px;
    border: 2px solid #fff;
    border-radius: 3px;
    background: transparent;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.55);
    cursor: pointer;
  }

  .hue::-moz-range-track {
    height: 14px;
    background: transparent;
  }

  .hue::-moz-range-thumb {
    width: 6px;
    height: 18px;
    border: 2px solid #fff;
    border-radius: 3px;
    background: transparent;
    cursor: pointer;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 2px 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .k {
    width: 30px;
    flex: none;
    color: var(--text-dim);
    font: 11px/1 var(--mono);
  }

  .n {
    width: 0;
    flex: 1;
    min-width: 0;
    font-family: var(--mono);
  }

  .hex {
    flex: 1;
    min-width: 0;
    font-family: var(--mono);
  }

  .live {
    width: 26px;
    height: 22px;
    flex: none;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 4px;
  }
</style>
