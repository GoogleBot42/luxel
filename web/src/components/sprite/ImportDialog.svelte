<script lang="ts">
  // `Import image…` — the fit/quantize step, with the record's size against
  // the 16 KiB cap on screen the whole time (Gitea #784).
  //
  // Jeremy (2026-09-26): "the ability to import/convert images to sprites".
  //
  // It is its OWN modal rather than a `stores/dialog` request because that
  // primitive renders text and one field (by design — #472), and this screen
  // is a live preview plus eight knobs. It keeps that dialog's chrome to the
  // pixel (`components/Dialog.svelte`: same backdrop, same panel, same
  // Escape/backdrop-cancel contract) so the app still has one modal LOOK, and
  // it wears `.scenes` for the inspector row vocabulary the sprite editor
  // already speaks rather than growing a second one.
  //
  // THE CAP IS NEVER ENFORCED BY TRUNCATION (#784 item 4). An over-cap import
  // says how far over it is, `Import` is disabled with the reason on
  // `data-reason` (§5.7), and `Fit under the cap` turns the two knobs — drop
  // every Nth frame, fewer colours — until it fits. Dropping frames silently
  // is what this is instead of.
  import { createEventDispatcher } from "svelte";
  import "../scene/scene.css";
  import RichSelect from "../RichSelect.svelte";
  import SpriteThumb from "./SpriteThumb.svelte";
  import {
    defaultImportOptions,
    fitUnderCap,
    importSprite,
    keptIndices,
    type FitMode,
    type ImportOptions,
    type ImportResult,
    type Resample,
  } from "../../lib/imageImport";
  import type { DecodedImage } from "../../lib/imageDecode";
  import { MAX_SOURCE_FRAMES } from "../../lib/imageDecode";
  import { SPRITE_MAX_COLORS, SPRITE_MAX_EDGE, type Sprite } from "../../lib/sprite";
  import type { RichOption } from "../../lib/blendMeta";

  /** The decoded file, or null when nothing is being imported. */
  export let source: DecodedImage | null = null;
  /** The fixture the sprite is being drawn for — the default target size. */
  export let panel: { w: number; h: number } | undefined = undefined;
  /** The per-record ceiling the host enforces. */
  export let maxBytes = 16 * 1024;

  const dispatch = createEventDispatcher<{ import: Sprite; close: void }>();

  let opts: ImportOptions | null = null;
  /** The file `opts` was derived from, so re-deriving happens once per file. */
  let forFile: DecodedImage | null = null;

  // A `$:` that both reads and writes `opts` would be its own dependency, so
  // the derivation is a FUNCTION call (.claude/rules/web.md).
  $: adopt(source);

  function adopt(src: DecodedImage | null): void {
    if (src === forFile) return;
    forFile = src;
    opts = src === null ? null : defaultImportOptions(src, src.name, panel);
  }

  /** The whole pipeline, re-run whenever a knob moves. Named dependencies. */
  $: result = source !== null && opts !== null ? importSprite(source, opts) : null;
  $: over = result !== null && result.bytes > maxBytes;
  $: capReason = over && result
    ? `this import is ${fmt(result.bytes)} B — the limit is ${fmt(maxBytes)} B. Drop frames or use fewer colours.`
    : "";

  const FIT_ROWS: readonly RichOption<FitMode>[] = [
    {
      value: "fit",
      label: "Fit",
      desc: "The whole image, proportions kept. What does not fill the box stays transparent.",
      icon: "imp-fit",
    },
    {
      value: "fill",
      label: "Stretch",
      desc: "Stretches to the box exactly, squashing the image if the shapes differ.",
      icon: "imp-fill",
    },
    {
      value: "crop",
      label: "Crop",
      desc: "Proportions kept and scaled up to cover the box; the overflowing edges are cut off.",
      icon: "imp-crop",
    },
  ];

  const RESAMPLE_ROWS: readonly RichOption<Resample>[] = [
    {
      value: "nearest",
      label: "Nearest",
      desc: "Takes the pixel under each texel. Exact for pixel art — and jagged for a photo.",
      icon: "imp-near",
    },
    {
      value: "area",
      label: "Area average",
      desc: "Averages every source pixel a texel covers. Right for photos and anything scanned.",
      icon: "imp-area",
    },
  ];

  function setFit(v: string): void {
    if (opts === null) return;
    opts = { ...opts, fit: v as FitMode };
  }

  function setResample(v: string): void {
    if (opts === null) return;
    opts = { ...opts, resample: v as Resample };
  }

  /** A number field, clamped IN the field so what you see is what is used —
   *  the rule every size field in this app keeps. */
  function setNum(
    field: "w" | "h" | "colors" | "alphaThreshold" | "keepEvery",
    el: HTMLInputElement,
    lo: number,
    hi: number,
  ): void {
    if (opts === null) return;
    const v = Math.max(lo, Math.min(hi, Math.floor(Number(el.value))));
    if (!Number.isFinite(v)) {
      el.value = String(opts[field]);
      return;
    }
    if (String(v) !== el.value) el.value = String(v);
    if (v === opts[field]) return;
    opts = { ...opts, [field]: v };
  }

  function fixCap(): void {
    if (source === null || opts === null) return;
    const fix = fitUnderCap(source, opts, maxBytes);
    opts = fix.options;
  }

  function accept(): void {
    if (result === null || over) return;
    dispatch("import", result.sprite);
  }

  function close(): void {
    dispatch("close");
  }

  function onKeydown(e: KeyboardEvent): void {
    if (source === null) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  /** `1,234` — grouped the same way the editor's byte line groups it. */
  function fmt(n: number): string {
    return n.toLocaleString("en-US");
  }

  /** `12 frames · 10 fps`, or `still`. */
  function playLine(r: ImportResult): string {
    if (r.sprite.frames < 2) return "1 frame · still";
    return `${r.sprite.frames} frames · ${r.sprite.fps} fps`;
  }

  /** What the source looks like, in one line. */
  function sourceLine(src: DecodedImage): string {
    const n = src.frames.length;
    const kind = src.type.replace(/^image\//, "").toUpperCase();
    return `${kind} · ${src.w}×${src.h} · ${n} frame${n === 1 ? "" : "s"}`;
  }

</script>

<svelte:window on:keydown|capture={onKeydown} />

{#if source !== null && opts !== null}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div class="backdrop" data-role="sprite-import-backdrop" on:mousedown|self={close}>
    <div
      class="panel scenes"
      data-role="sprite-import-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="lx-import-title"
    >
      <h2 id="lx-import-title">Import image</h2>
      <p class="body" data-role="sprite-import-source">
        <strong>{source.name}</strong> · {sourceLine(source)}
      </p>
      {#if source.firstFrameOnly}
        <p class="warn" data-role="sprite-import-firstframe">
          This browser reads only the first frame of an animation. Chromium reads them all.
        </p>
      {/if}
      {#if source.sourceTruncated > 0}
        <p class="warn" data-role="sprite-import-truncated">
          Only the first {MAX_SOURCE_FRAMES} frames of the file were read ({source.sourceTruncated}
          more are in it).
        </p>
      {/if}

      <div class="two">
        <!-- the PREVIEW is the record itself, drawn by the same component the
             tiles and the editor's preview use, animating at the fps the
             import derived. What you see here is the sprite you get. -->
        <div class="pcol">
          <div class="rhead"><div class="slabel">Preview</div></div>
          <div class="plate" data-role="sprite-import-plate">
            <SpriteThumb
              sprite={result === null ? null : result.sprite}
              size={176}
              dataRole="sprite-import-preview"
            />
          </div>
          <div class="hint" data-role="sprite-import-play">
            {result === null ? "" : playLine(result)}
          </div>
          <div class="hint" data-role="sprite-import-colors-used">
            {result === null
              ? ""
              : `${result.colorsUsed} colour${result.colorsUsed === 1 ? "" : "s"} of ${result.colorsSeen} seen`}
          </div>
        </div>

        <div class="fcol">
          <div class="irow">
            <div class="ilab">Size</div>
            <div class="boxrow" data-role="sprite-import-size">
              <label
                >w <input
                  class="inp"
                  type="number"
                  min="1"
                  max={SPRITE_MAX_EDGE}
                  data-role="sprite-import-w"
                  value={opts.w}
                  on:change={(e) => setNum("w", e.currentTarget, 1, SPRITE_MAX_EDGE)}
                /></label
              >
              <label
                >h <input
                  class="inp"
                  type="number"
                  min="1"
                  max={SPRITE_MAX_EDGE}
                  data-role="sprite-import-h"
                  value={opts.h}
                  on:change={(e) => setNum("h", e.currentTarget, 1, SPRITE_MAX_EDGE)}
                /></label
              >
              <span class="un">px</span>
            </div>
          </div>

          <div class="irow">
            <div class="ilab">Placement</div>
            <RichSelect
              value={opts.fit}
              options={FIT_ROWS}
              dataRole="sprite-import-fit"
              menuRole="sprite-import-fit-menu"
              ariaLabel="how the image is placed in the box"
              on:input={(e) => setFit(e.detail)}
            >
              <svelte:fragment slot="icon" let:option>
                <svg viewBox="0 0 24 24">
                  <rect x="0" y="0" width="24" height="24" rx="4" fill="#0e1014" />
                  <rect
                    x="3.5"
                    y="3.5"
                    width="17"
                    height="17"
                    rx="2"
                    fill="none"
                    stroke="#6f7686"
                    stroke-dasharray="2 2"
                  />
                  {#if option.value === "fit"}
                    <rect x="5" y="8" width="14" height="8" rx="1" fill="#e8a33d" />
                  {:else if option.value === "fill"}
                    <rect x="4.5" y="4.5" width="15" height="15" rx="1" fill="#e8a33d" />
                  {:else}
                    <!-- cover: the accent runs off two edges of the box -->
                    <rect x="1" y="4.5" width="22" height="15" rx="1" fill="#e8a33d" />
                    <rect x="3.5" y="3.5" width="17" height="17" rx="2" fill="none" stroke="#0e1014" />
                  {/if}
                </svg>
              </svelte:fragment>
            </RichSelect>
          </div>

          <div class="irow">
            <div class="ilab">Resample</div>
            <RichSelect
              value={opts.resample}
              options={RESAMPLE_ROWS}
              dataRole="sprite-import-resample"
              menuRole="sprite-import-resample-menu"
              ariaLabel="how source pixels become texels"
              on:input={(e) => setResample(e.detail)}
            >
              <svelte:fragment slot="icon" let:option>
                <svg viewBox="0 0 24 24">
                  <rect x="0" y="0" width="24" height="24" rx="4" fill="#0e1014" />
                  {#if option.value === "nearest"}
                    {#each [[5, 5], [13, 5], [5, 13], [13, 13]] as p (`${p[0]}:${p[1]}`)}
                      <rect x={p[0]} y={p[1]} width="6" height="6" rx="0.5" fill="#e8a33d" />
                    {/each}
                  {:else}
                    <defs>
                      <linearGradient id="lxImpArea" x1="0" y1="0" x2="1" y2="1">
                        <stop offset="0" stop-color="#e8a33d" />
                        <stop offset="1" stop-color="#e8a33d" stop-opacity="0.25" />
                      </linearGradient>
                    </defs>
                    <rect x="5" y="5" width="14" height="14" rx="2" fill="url(#lxImpArea)" />
                  {/if}
                </svg>
              </svelte:fragment>
            </RichSelect>
          </div>

          <div class="irow">
            <div class="ilab">Colours</div>
            <div class="boxrow">
              <input
                class="inp num xs"
                style="width:64px"
                type="number"
                min="1"
                max={SPRITE_MAX_COLORS}
                data-role="sprite-import-colors"
                value={opts.colors}
                on:change={(e) => setNum("colors", e.currentTarget, 1, SPRITE_MAX_COLORS)}
              />
              <span class="un">max</span>
            </div>
          </div>

          <div class="irow">
            <div class="ilab">Clear below</div>
            <div class="boxrow">
              <input
                class="inp num xs"
                style="width:64px"
                type="number"
                min="1"
                max="255"
                data-role="sprite-import-alpha"
                value={opts.alphaThreshold}
                on:change={(e) => setNum("alphaThreshold", e.currentTarget, 1, 255)}
              />
              <span class="un">alpha</span>
            </div>
          </div>

          {#if source.frames.length > 1}
            <div class="irow">
              <div class="ilab">Keep every</div>
              <div class="boxrow">
                <input
                  class="inp num xs"
                  style="width:64px"
                  type="number"
                  min="1"
                  max={source.frames.length}
                  data-role="sprite-import-keep"
                  value={opts.keepEvery}
                  on:change={(e) => setNum("keepEvery", e.currentTarget, 1, source === null ? 1 : source.frames.length)}
                />
                <span class="un"
                  >of {keptIndices(source.frames.length, opts.keepEvery).length} frames</span
                >
              </div>
            </div>
          {/if}

          <div class="irow">
            <div class="ilab">Dither</div>
            <label class="ck">
              <input
                type="checkbox"
                data-role="sprite-import-dither"
                checked={opts.dither}
                on:change={(e) => (opts = opts === null ? null : { ...opts, dither: e.currentTarget.checked })}
              />
              <span class="hint">diffuse the quantizing error — usually worse on LEDs</span>
            </label>
          </div>

          <div class="irule"></div>

          <div class="irow">
            <div class="ilab">Record</div>
            <div class="hint" class:over data-role="sprite-import-bytes">
              {result === null ? "" : `${fmt(result.bytes)} of ${fmt(maxBytes)} B`}
            </div>
          </div>

          {#if result !== null && result.droppedFrames > 0}
            <p class="warn" data-role="sprite-import-dropped">
              {result.droppedFrames} frame{result.droppedFrames === 1 ? "" : "s"} past the
              255-frame limit {result.droppedFrames === 1 ? "is" : "are"} not in this record — raise
              <em>Keep every</em> to sample the whole animation.
            </p>
          {/if}

          {#if over}
            <p class="warn" data-role="sprite-import-over">
              Over the {fmt(maxBytes)} B cap. Nothing is dropped silently: turn a knob, or let
              this do it.
            </p>
            <button class="btn" data-role="sprite-import-fitcap" on:click={fixCap}
              >Fit under the cap</button
            >
          {/if}
        </div>
      </div>

      <div class="buttons">
        <button class="btn" data-role="sprite-import-cancel" on:click={close}>Cancel</button>
        <button
          class="btn primary"
          data-role="sprite-import-ok"
          disabled={over}
          data-reason={over ? capReason : null}
          title={capReason}
          on:click={accept}>Import</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  /* the chrome is `components/Dialog.svelte`'s, to the pixel: one modal look */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    background: rgba(0, 0, 0, 0.55);
  }

  .panel {
    width: min(620px, 100%);
    max-height: calc(100vh - 32px);
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-panel);
    box-shadow: 0 14px 34px rgba(0, 0, 0, 0.65);
  }

  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }

  .body {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .two {
    display: grid;
    grid-template-columns: 192px 1fr;
    gap: 16px;
    align-items: start;
  }

  .pcol {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .fcol {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .plate {
    width: 176px;
    max-width: 100%;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  .warn {
    margin: 6px 0 0;
    padding: 6px 8px;
    border: 1px solid color-mix(in srgb, var(--warn) 45%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--warn) 12%, transparent);
    color: var(--warn);
    font-size: 12px;
    line-height: 1.45;
  }

  .ck {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 24px;
  }

  .un {
    font: 11px/1 var(--mono);
    color: var(--text-dim);
  }

  .hint.over {
    color: var(--error);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }

  /* the spinners never fit these fields (the inspector keeps the same rule) */
  input[type="number"] {
    appearance: textfield;
    -moz-appearance: textfield;
  }

  input[type="number"]::-webkit-outer-spin-button,
  input[type="number"]::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
  }

  @media (max-width: 600px) {
    .two {
      grid-template-columns: 1fr;
    }

    .backdrop {
      align-items: flex-start;
      padding: 8px;
    }

    .buttons {
      flex-direction: column-reverse;
      gap: 6px;
    }

    .buttons button {
      width: 100%;
      padding: 8px;
    }
  }
</style>
