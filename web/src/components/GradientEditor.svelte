<script lang="ts" context="module">
  // ONE gradient / colour-ramp editor, for BOTH homes the app has: the scene
  // pattern layer's `Color ramp` (mockups S7b/S7g) and Settings › Output ›
  // Palette (S3/S3i). Gitea #734.
  //
  // Before #734 each home carried its own copy of the markup, each painted a
  // CSS `linear-gradient` that could not agree with what the device renders,
  // each used the FORBIDDEN native `<input type="color">`
  // (ColorPicker.svelte:2-4, .claude/rules/web.md), and neither let you drag a
  // stop. This file is the only copy of any of it now.
  //
  // The idiom is the standard one (Jeremy named PatternFlow's): a wide bar
  // showing the REAL gradient, draggable handles under it, the selected
  // stop's colour opened in the app's own `ColorPicker`, and a numeric
  // position field as the exact fallback.
  //
  // ── THE PREVIEW IS THE ENGINE'S 256-ENTRY TABLE, RE-DERIVED HERE ─────────
  // A ramp is not a CSS gradient: the device cooks the stop list into a
  // 768-byte luma → colour LUT and indexes it per pixel
  // (`luxel_core::outpipe::fill_palette_lut`, the per-layer cache is
  // `compose.rs::ensure_lut`, the interpolation is `vm::sample_palette`).
  // The table is NOT sRGB-linear interpolation: it is 16.16 fixed point with
  // truncating divides, it CLAMPS below the first stop and goes BLACK above
  // the last one, and it quantizes with `floor(v·255)`.
  //
  // **There is no wasm entry point that hands the browser that table** — the
  // only palette path across the boundary is `Engine.setOutpipe()`, which
  // needs a whole compiled engine and a rendered frame. So the maths below is
  // a DELIBERATE SECOND IMPLEMENTATION of `sample_palette` + `fill_palette_lut`
  // + `palette_remap_frame`, bit-for-bit (`Fx` semantics included:
  // `Fx::mul` floors, `Fx::div` truncates, a zero divisor is 0). If you change
  // one side you MUST change the other. Gitea #748 tracks exporting
  // `lx_palette_lut` from luxel-wasm and pinning the two against each other in
  // `web/tests/`, which is what should finally delete this block.
  import { MAX_RAMP_STOPS } from "../lib/scene";

  /** One editable stop: a byte position along the luma axis, and a colour. */
  export interface GradientStop {
    /** 0..255 — the engine's own position domain, not a percentage. */
    pos: number;
    /** `rrggbb`, no leading `#` (the scene wire form; `#${hex}` for CSS). */
    hex: string;
  }

  /** The cap, from the ONE constant (`lib/scene.ts`, itself pinned to
   *  `outpipe::MAX_OUTPUT_PALETTE_STOPS`). Never write `32` again. */
  export const MAX_STOPS = MAX_RAMP_STOPS;

  const FX_ONE = 65536;

  /** `Fx::mul` — the full product shifted right 16. Rust's `>>` on i64 is
   *  ARITHMETIC, so this floors; the channel deltas below go negative and a
   *  `trunc` here would be off by one on every falling edge. */
  const fxMul = (a: number, b: number): number => Math.floor((a * b) / FX_ONE);

  /** `Fx::div` — truncated, and 0 on a zero divisor (oracle-verified). The
   *  integer-divisor shortcut is kept so the two paths are visibly the same
   *  value the Rust takes. */
  function fxDiv(a: number, b: number): number {
    if (b === 0) return 0;
    if ((b & 0xffff) === 0) return Math.trunc(a / (b >> 16));
    return Math.trunc((a * FX_ONE) / b);
  }

  /** `ensure_lut`'s `b`: a 0..255 byte → 16.16 over 0..1, truncating. */
  const fxByte = (v: number): number => Math.trunc((v * FX_ONE) / 255);

  /** `engine::quantize`: `floor(v·255)` on a clamped unit value. */
  const quant = (v: number): number =>
    Math.floor(((v < 0 ? 0 : v > FX_ONE ? FX_ONE : v) * 255) / FX_ONE);

  type FxStop = [number, [number, number, number]];

  /** `luxel_core::vm::sample_palette`, verbatim. */
  function samplePalette(pal: readonly FxStop[], v: number): [number, number, number] {
    const first = pal[0];
    const last = pal[pal.length - 1];
    if (first === undefined || last === undefined) return [v, v, v];
    if (v <= first[0]) return first[1];
    if (v === last[0]) return last[1];
    if (v > last[0]) return [0, 0, 0];
    for (let i = 0; i + 1 < pal.length; i++) {
      const lo = pal[i];
      const hi = pal[i + 1];
      if (lo === undefined || hi === undefined) break;
      if (v <= hi[0]) {
        const span = hi[0] - lo[0];
        const t = span === 0 ? 0 : fxDiv(v - lo[0], span);
        return [
          lo[1][0] + fxMul(hi[1][0] - lo[1][0], t),
          lo[1][1] + fxMul(hi[1][1] - lo[1][1], t),
          lo[1][2] + fxMul(hi[1][2] - lo[1][2], t),
        ];
      }
    }
    return last[1];
  }

  const clampByte = (v: number): number =>
    Number.isFinite(v) ? Math.max(0, Math.min(255, Math.round(v))) : 0;

  /**
   * The 768-byte table the device builds from these stops, blended at
   * `amountPct` the way `palette_remap_frame` blends it over a greyscale
   * frame — which is exactly what the bar draws, because `luma([i,i,i]) === i`
   * (54+183+19 = 256). So entry `i` is literally "what the device turns a
   * pixel of brightness `i` into at this amount".
   *
   * An EMPTY stop list is not special-cased: `sample_palette` returns the
   * identity for one, so the bar goes grey — which is what "no palette" means.
   */
  export function rampLut(stops: readonly GradientStop[], amountPct = 100): Uint8Array {
    const pal: FxStop[] = [...stops]
      .sort((a, b) => a.pos - b.pos)
      .map((s) => {
        const v = Number.parseInt(s.hex.replace(/^#/, ""), 16);
        const n = Number.isFinite(v) ? v : 0;
        return [
          fxByte(clampByte(s.pos)),
          [fxByte((n >> 16) & 0xff), fxByte((n >> 8) & 0xff), fxByte(n & 0xff)],
        ];
      });
    // `r.pct as u32 * 256 / 100`, then `amount.min(256)` (compose.rs:753)
    const pct = Math.max(0, Math.min(100, Math.round(amountPct)));
    const a = Math.min(256, Math.trunc((pct * 256) / 100));
    const out = new Uint8Array(768);
    for (let i = 0; i < 256; i++) {
      const c = samplePalette(pal, fxByte(i));
      const t = [quant(c[0]), quant(c[1]), quant(c[2])];
      for (let k = 0; k < 3; k++) {
        const target = t[k] ?? 0;
        out[i * 3 + k] = a >= 256 ? target : (i * (256 - a) + target * a) >> 8;
      }
    }
    return out;
  }

  /** Paint one of those tables into a canvas: 256 columns, one per entry, so
   *  the control literally cannot disagree with the render. */
  export function paintRamp(
    cv: HTMLCanvasElement | null | undefined,
    stops: readonly GradientStop[],
    amountPct = 100,
  ): void {
    if (!cv) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    if (cv.width !== 256) cv.width = 256;
    if (cv.height !== 1) cv.height = 1;
    const lut = rampLut(stops, amountPct);
    const img = ctx.createImageData(256, 1);
    for (let i = 0; i < 256; i++) {
      img.data[i * 4] = lut[i * 3] ?? 0;
      img.data[i * 4 + 1] = lut[i * 3 + 1] ?? 0;
      img.data[i * 4 + 2] = lut[i * 3 + 2] ?? 0;
      img.data[i * 4 + 3] = 255;
    }
    ctx.putImageData(img, 0, 0);
  }

  const pair = (n: number): string => n.toString(16).padStart(2, "0");

  /** The colour the ramp ALREADY has at `pos` — what a stop added there must
   *  take, so that adding one never changes the gradient. */
  function hexAtPos(stops: readonly GradientStop[], pos: number): string {
    const lut = rampLut(stops, 100);
    const i = clampByte(pos) * 3;
    return `${pair(lut[i] ?? 0)}${pair(lut[i + 1] ?? 0)}${pair(lut[i + 2] ?? 0)}`;
  }
</script>

<script lang="ts">
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import ColorPicker from "./ColorPicker.svelte";
  import { hexToRgb, rgbToHex, type Rgb } from "../lib/color";

  /** The stops, ascending by position. */
  export let stops: GradientStop[] = [];
  /** Blend amount 0..100 — the scene's `Ramp.pct`, the device's
   *  `paletteAmount`. */
  export let amount = 100;
  /** `data-role` prefix: `scene-ramp` / `out-palette`. Every role this
   *  component emits hangs off it, which is what let the two homes keep the
   *  names their harnesses and `mockdiff.map.json` already use. */
  export let role = "gradient";
  /** The bar's own role — the two homes named it differently
   *  (`scene-ramp` vs `out-palette-preview`), so it is explicit. */
  export let previewRole = "";
  /** Below this, a removal CLEARS instead (a scene ramp needs two stops; a
   *  device palette is legitimately empty). */
  export let minStops = 2;
  /** Accessible name for the bar. */
  export let label = "colour ramp";
  /** The half-sentence after the stop count. */
  export let summary = "";
  /** What the count line says with no stops at all. */
  export let emptyLabel = "no stops";
  /** Mock copy for the disclosure (S7g / S3i both say `Edit stops…`). */
  export let editLabel = "Edit stops…";

  const dispatch = createEventDispatcher<{
    /** Mid-gesture: cheap, local, do not push it to a device. */
    input: { stops: GradientStop[]; amount: number };
    /** Committed: the value to store or push. */
    change: { stops: GradientStop[]; amount: number };
    /** The ramp/palette itself is gone (below `minStops`, or `clear`). */
    clear: null;
  }>();

  /** How far off the bar a drag has to go to mean "delete this stop". */
  const DROP_OFF = 28;

  let bar: HTMLCanvasElement | null = null;
  let blendBar: HTMLCanvasElement | null = null;
  let row: HTMLElement | null = null;
  let picked = 0;
  let editing = false;
  /** The in-flight value during a drag. It exists because the parent may be a
   *  device round-trip: without it a poll landing mid-drag snaps the handle
   *  back to the stored ramp. Cleared on commit. */
  let draft: GradientStop[] | null = null;
  let drag: { i: number; id: number; off: boolean } | null = null;

  $: view = draft ?? stops;
  $: barRole = previewRole === "" ? role : previewRole;
  // Every dependency is NAMED in the block's own syntax — a bare `paint()`
  // that closed over them would silently stop re-running (.claude/rules/web.md),
  // which is precisely how the old CSS preview "only showed two stops".
  $: paintRamp(bar, view, 100);
  $: paintRamp(blendBar, view, amount);
  // ordered AFTER the `view` block on purpose: Svelte sequences `$:` by the
  // assignments it can SEE, and this one reads that block's output
  // (.claude/rules/web.md).
  $: cur = view[picked] ?? view[0] ?? null;

  const clamp255 = clampByte;
  const canRemove = (list: readonly GradientStop[]): boolean => list.length > minStops;
  const unit = (hex: string): number[] => hexToRgb(hex) ?? [0, 0, 0];

  function countText(list: readonly GradientStop[]): string {
    if (list.length === 0) return emptyLabel;
    return `${list.length} stop${list.length === 1 ? "" : "s"}`;
  }

  /** Emit. `commit` distinguishes a finished edit from a drag frame. */
  function emit(next: GradientStop[], commit: boolean): void {
    draft = commit ? null : next;
    if (commit) dispatch("change", { stops: next, amount });
    else dispatch("input", { stops: next, amount });
  }

  /** Sort by position, STABLY, and say where the stop that was at `i` went —
   *  without which dragging one stop past another swaps which one you hold. */
  function sortFrom(list: GradientStop[], i: number): { stops: GradientStop[]; index: number } {
    const tagged = list.map((s, n) => ({ s, n }));
    tagged.sort((a, b) => a.s.pos - b.s.pos || a.n - b.n);
    const index = tagged.findIndex((t) => t.n === i);
    return { stops: tagged.map((t) => t.s), index: index < 0 ? 0 : index };
  }

  /** Move stop `i` to `pos`; returns its index after the re-sort. */
  function moveStop(i: number, pos: number, commit: boolean): number {
    const list = view.map((s, n) => (n === i ? { pos: clamp255(pos), hex: s.hex } : s));
    const r = sortFrom(list, i);
    picked = r.index;
    if (drag) drag = { ...drag, i: r.index };
    emit(r.stops, commit);
    return r.index;
  }

  /** `ColorPicker` emits on every pointermove across its saturation field, so
   *  a colour edit is a STORM like a drag is: live locally, committed once the
   *  hand stops. Without the trailing timer Settings would POST the whole
   *  palette per mouse move. */
  let colorTimer = 0;
  function setColor(i: number, rgb: number[]): void {
    const c: Rgb = [rgb[0] ?? 0, rgb[1] ?? 0, rgb[2] ?? 0];
    const hex = rgbToHex(c).replace(/^#/, "");
    const next = view.map((s, n) => (n === i ? { pos: s.pos, hex } : s));
    emit(next, false);
    clearTimeout(colorTimer);
    colorTimer = window.setTimeout(() => emit(next, true), 250);
  }

  onDestroy(() => clearTimeout(colorTimer));

  function setAmount(v: number): void {
    const next = Math.max(0, Math.min(100, Math.round(Number.isFinite(v) ? v : 0)));
    amount = next;
    dispatch("change", { stops: view, amount: next });
  }

  /** Add at `pos`, taking the colour the ramp already has there. Returns the
   *  new stop's index, or -1 at the cap. */
  function addAt(pos: number, commit = true): number {
    if (view.length >= MAX_STOPS) return -1;
    const list: GradientStop[] = [...view, { pos: clamp255(pos), hex: hexAtPos(view, pos) }];
    const r = sortFrom(list, list.length - 1);
    picked = r.index;
    emit(r.stops, commit);
    return r.index;
  }

  /** Where the `add stop` button puts one: past the last, as it always has. */
  function nextPos(list: readonly GradientStop[]): number {
    const last = list[list.length - 1];
    return last ? Math.min(255, last.pos + 64) : 0;
  }

  function removeStop(i: number): void {
    if (!canRemove(view)) {
      clearAll();
      return;
    }
    const list = view.filter((_, n) => n !== i);
    picked = Math.max(0, Math.min(i, list.length - 1));
    emit(list, true);
  }

  function clearAll(): void {
    draft = null;
    picked = 0;
    dispatch("clear", null);
  }

  // ---- the bar: drag a stop, click to add one, drag off to remove ----

  function posFromX(x: number): number {
    const r = bar?.getBoundingClientRect();
    if (!r || r.width === 0) return 0;
    return clamp255(((x - r.left) / r.width) * 255);
  }

  function onDown(e: PointerEvent): void {
    const mk = (e.target as Element | null)?.closest?.("[data-stop]");
    let i: number;
    if (mk instanceof HTMLElement) {
      i = Number(mk.dataset["stop"] ?? 0);
      picked = i;
      // touching a stop is what the detail panel is FOR — the mock's resting
      // state stays collapsed, the first interaction opens it
      editing = true;
    } else {
      i = addAt(posFromX(e.clientX));
      if (i < 0) return;
      editing = true;
    }
    drag = { i, id: e.pointerId, off: false };
    const host = e.currentTarget;
    if (host instanceof HTMLElement) host.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent): void {
    const d = drag;
    if (!d || e.pointerId !== d.id) return;
    const r = bar?.getBoundingClientRect();
    const off =
      !!r && canRemove(view) && (e.clientY < r.top - DROP_OFF || e.clientY > r.bottom + DROP_OFF);
    if (off !== d.off) drag = { ...d, off };
    if (!off) moveStop(d.i, posFromX(e.clientX), false);
  }

  function onUp(e: PointerEvent): void {
    const d = drag;
    drag = null;
    if (!d || e.pointerId !== d.id) return;
    if (d.off) {
      removeStop(d.i);
      return;
    }
    if (draft) emit(view, true);
  }

  /** A handle is a slider, so it takes the keys one takes. */
  function onStopKey(e: KeyboardEvent, i: number): void {
    const cur = view[i];
    if (!cur) return;
    const step = e.shiftKey ? 16 : 1;
    let next: number;
    switch (e.key) {
      case "ArrowLeft":
      case "ArrowDown":
        next = cur.pos - step;
        break;
      case "ArrowRight":
      case "ArrowUp":
        next = cur.pos + step;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = 255;
        break;
      case "Delete":
      case "Backspace":
        e.preventDefault();
        removeStop(i);
        return;
      default:
        return;
    }
    e.preventDefault();
    picked = i;
    editing = true;
    const moved = moveStop(i, next, false);
    // the each block is keyed by index, so a re-sort leaves the focus on the
    // node that now holds a DIFFERENT stop — follow the one being dragged
    if (moved !== i) void refocus(moved);
  }

  function onStopKeyUp(e: KeyboardEvent): void {
    if (draft && /^(Arrow|Home|End)/.test(e.key)) emit(view, true);
  }

  async function refocus(i: number): Promise<void> {
    await tick();
    row?.querySelector<HTMLElement>(`[data-stop="${i}"]`)?.focus();
  }
</script>

<div class="ge">
  <!-- the bar and its handles are ONE pointer surface: a press on a handle
       drags it, a press on the bar adds one and drags that -->
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="barwrap"
    role="group"
    aria-label={label}
    on:pointerdown={onDown}
    on:pointermove={onMove}
    on:pointerup={onUp}
    on:pointercancel={onUp}
  >
    <canvas
      class="bar"
      bind:this={bar}
      data-role={barRole}
      aria-hidden="true"
      title="the device's own 256-entry ramp table — click to add a stop"
    ></canvas>
    <div class="stoprow" data-role={`${role}-stops`} bind:this={row}>
      {#each view as s, i (i)}
        <button
          class="stopmk"
          class:on={picked === i}
          class:dropping={drag !== null && drag.i === i && drag.off}
          type="button"
          data-stop={i}
          data-role={`${role}-stop`}
          role="slider"
          tabindex="0"
          aria-label={`stop ${i + 1} position`}
          aria-valuemin={0}
          aria-valuemax={255}
          aria-valuenow={s.pos}
          aria-valuetext={`${s.pos} of 255, #${s.hex}`}
          style={`left:${((s.pos / 255) * 100).toFixed(2)}%;background:#${s.hex}`}
          on:click={() => {
            picked = i;
            editing = true;
          }}
          on:keydown={(e) => onStopKey(e, i)}
          on:keyup={onStopKeyUp}
        ></button>
      {/each}
    </div>
  </div>

  {#if amount < 100 && view.length > 0}
    <!-- the same table blended at `amount`: what the device ACTUALLY puts on
         the LEDs, since the stage blends against the pattern's own pixel -->
    <canvas
      class="blend"
      bind:this={blendBar}
      data-role={`${role}-blend`}
      aria-hidden="true"
      title={`the ramp applied at ${amount}%`}
    ></canvas>
  {/if}

  <div class="editrow">
    <button
      class="btn sm"
      type="button"
      data-role={`${role}-edit`}
      aria-expanded={editing}
      on:click={() => (editing = !editing)}>{editLabel}</button
    >
    <span class="hint" data-role={`${role}-summary`}
      >{countText(view)}{summary === "" ? "" : ` · ${summary}`}</span
    >
  </div>

  <div class="toolrow">
    <!-- The ONE deliberate §5.7 exception: a budget the user has to learn. It
         stays disabled AT the cap and carries `data-reason`, with the same
         words on screen under the row (Gitea #529). -->
    {#if view.length >= MAX_STOPS}
      <button
        class="btn sm"
        type="button"
        data-role={`${role}-add`}
        disabled
        data-reason={`all ${MAX_STOPS} stops are used`}>add stop</button
      >
    {:else}
      <button
        class="btn sm"
        type="button"
        data-role={`${role}-add`}
        on:click={() => addAt(nextPos(view))}>add stop</button
      >
    {/if}
    <!-- nothing to clear with no stops → absent, never disabled -->
    {#if view.length > 0}
      <button class="btn sm" type="button" data-role={`${role}-clear`} on:click={clearAll}
        >clear</button
      >
    {/if}
    <label class="amt">
      amount
      <input
        class="inp num"
        type="number"
        min="0"
        max="100"
        step="5"
        data-role={`${role}-amount`}
        value={amount}
        on:change={(e) => setAmount(Number(e.currentTarget.value))}
      />
      %
    </label>
  </div>

  {#if view.length >= MAX_STOPS}
    <span class="hint" data-role={`${role}-cap`}>
      all {MAX_STOPS} stops are used — remove one to add another
    </span>
  {/if}

  {#if editing}
    <div class="panel" data-role={`${role}-editor`}>
      {#if cur}
        <div class="srow">
          <span class="k">Stop {picked + 1}</span>
          <!-- the app's OWN picker, never `<input type="color">`
               (ColorPicker.svelte:2-4) -->
          <span class="pick" data-role={`${role}-color`}>
            <ColorPicker
              kind="rgb"
              value={unit(cur.hex)}
              label={`stop ${picked + 1} colour`}
              on:input={(e) => setColor(picked, e.detail)}
            />
          </span>
          <label class="amt">
            position
            <input
              class="inp num"
              type="number"
              min="0"
              max="255"
              data-role={`${role}-pos`}
              value={cur.pos}
              on:change={(e) => moveStop(picked, Number(e.currentTarget.value), true)}
            />
          </label>
          <button
            class="btn sm"
            type="button"
            data-role={`${role}-remove`}
            on:click={() => removeStop(picked)}>remove</button
          >
        </div>
      {/if}
      <span class="hint">
        drag a stop along the bar · click the bar to add one · arrow keys nudge the focused stop,
        Delete removes it
      </span>
    </div>
  {/if}
</div>

<style>
  .ge {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .barwrap {
    position: relative;
    touch-action: none;
  }

  /* mockups' `.gradbar`: 22px tall, radius 4, a 1px border — and now a
     canvas, because what it shows is the engine's table, not a CSS guess */
  .bar {
    display: block;
    width: 100%;
    height: 22px;
    border: 1px solid var(--border);
    border-radius: 4px;
    /* 256 real entries, scaled — no browser-invented interpolation over them */
    image-rendering: pixelated;
    cursor: copy;
  }

  .blend {
    display: block;
    width: 100%;
    height: 10px;
    border: 1px solid var(--border);
    border-radius: 3px;
    image-rendering: pixelated;
  }

  /* mockups' `.stoprow` / `.stopmk` */
  .stoprow {
    position: relative;
    height: 12px;
    margin-top: 4px;
  }

  .stopmk {
    position: absolute;
    top: 0;
    width: 9px;
    height: 9px;
    margin-left: -5px;
    padding: 0;
    border-radius: 2px;
    transform: rotate(45deg);
    border: 1px solid rgba(255, 255, 255, 0.45);
    cursor: grab;
  }

  .stopmk.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(232, 163, 61, 0.25);
  }

  .stopmk.dropping {
    opacity: 0.35;
  }

  .stopmk:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }

  .editrow,
  .toolrow,
  .srow {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .k,
  .amt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .pick {
    display: inline-flex;
  }

  .amt .inp.num {
    width: 68px;
  }
</style>
