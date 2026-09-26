<script lang="ts">
  // A dropdown whose OPTIONS explain themselves: a small drawing of what the
  // mode does, its name, and one line of plain English (Gitea #735).
  //
  // A `<select>` cannot hold any of that — an `<option>` is text — so this is
  // the same shape `components/scene/FontPicker.svelte` already uses: a
  // `.sel`-clothed button that opens a `Popover kind="pop"`. The trigger is a
  // real button rather than a `<div>`, so it keeps the keyboard and the focus
  // ring, and unlike an `appearance:none` `<select>` it cannot clip its own
  // value (#735's dropdown-overflow audit — the trigger grows with the name).
  //
  // The DRAWINGS are the point, and they are drawn rather than described:
  //   * the blends are ONE `<svg>` for all five — the same base swatch and
  //     the same layer swatch, composited by the CSS blend mode that matches
  //     the formula in §5.5 (`plus-lighter` IS `min(255, B + αL)`, `lighten`
  //     IS `max(B, αL)`, `multiply` IS `B·L/255`, and Mask is that multiply
  //     with a luminance ramp for L). So the icon is not an artist's
  //     impression of the mode: the browser computes it with the same maths
  //     the firmware does, which is also why five modes cost one drawing.
  //   * the key and fit drawings are plain geometry.
  // The whole set is inline and shares its geometry because the packed
  // bundle is CI-gated at 983,040 B (#592).
  //
  // `isolation:isolate` on the composited group is load-bearing: without it a
  // `mix-blend-mode` reaches past the icon and blends with the panel.
  import { createEventDispatcher, onDestroy } from "svelte";
  import Popover from "./Popover.svelte";
  import type { RichOption } from "../lib/blendMeta";

  /** The option currently stored. A value with no row shows as itself. */
  export let value: string;
  export let options: readonly RichOption[];
  /** `data-role` for the trigger; the rows get `<dataRole>-<value>`. */
  export let dataRole = "";
  /** `data-role` for the popover itself. */
  export let menuRole = "";
  /** What the control is called, for a screen reader. */
  export let ariaLabel = "";

  const dispatch = createEventDispatcher<{ input: string }>();

  let open = false;
  let btn: HTMLElement | null = null;

  $: current = options.find((o) => o.value === value) ?? null;

  /** The two swatches every blend drawing composites, and the CSS blend mode
   *  that reproduces that blend's formula. `mask` multiplies a luminance ramp
   *  because `B·luma(L)/255` IS a multiply by L's brightness. */
  const PAINT: Record<string, { mode: string; fill: string }> = {
    normal: { mode: "normal", fill: "#e8a33d" },
    add: { mode: "plus-lighter", fill: "#e8a33d" },
    lighten: { mode: "lighten", fill: "#e8a33d" },
    multiply: { mode: "multiply", fill: "#e8a33d" },
    mask: { mode: "multiply", fill: "url(#lxLumRamp)" },
  };

  function choose(v: string): void {
    open = false;
    dispatch("input", v);
  }

  /** A popover left open while its inspector is swapped out would hang over
   *  the next one (`FontPicker` does the same). */
  onDestroy(() => (open = false));
</script>

<button
  class="sel wide"
  class:open
  bind:this={btn}
  type="button"
  data-role={dataRole || null}
  data-value={value}
  aria-haspopup="menu"
  aria-expanded={open}
  aria-label={ariaLabel || null}
  on:click={() => (open = !open)}
>
  <span class="tval">{current ? current.label : value}</span>
</button>

<Popover
  {open}
  anchor={btn}
  align="start"
  kind="pop"
  dataRole={menuRole}
  on:close={() => (open = false)}
>
  {#each options as o (o.value)}
    <!-- hoisted so TypeScript can narrow it: an index expression repeated in
         the markup is `| undefined` at every use, however it was guarded -->
    {@const paint = PAINT[o.icon]}
    <button
      class="orow"
      class:on={o.value === value}
      type="button"
      role="menuitemradio"
      aria-checked={o.value === value}
      data-role={dataRole ? `${dataRole}-${o.value}` : null}
      on:click={() => choose(o.value)}
    >
      <span class="ico" aria-hidden="true">
        {#if paint}
          <!-- the five blends: one drawing, one blend mode apart -->
          <svg viewBox="0 0 24 24">
            {#if o.icon === "mask"}
              <defs>
                <linearGradient id="lxLumRamp" x1="0" y1="0" x2="1" y2="0">
                  <stop offset="0" stop-color="#fff" />
                  <stop offset="1" stop-color="#000" />
                </linearGradient>
              </defs>
            {/if}
            <g style="isolation:isolate">
              <rect x="0" y="0" width="24" height="24" rx="4" fill="#0e1014" />
              <rect x="2.5" y="7" width="12" height="12" rx="2" fill="#3b7ddd" />
              <rect
                x="9.5"
                y="5"
                width="12"
                height="12"
                rx="2"
                fill={paint.fill}
                style={`mix-blend-mode:${paint.mode}`}
              />
            </g>
            <!-- the layer's footprint, outlined OUTSIDE the blend group so a
                 mode that darkens it to nothing (Multiply, Mask) still shows
                 where the layer is -->
            <rect
              x="9.5"
              y="5"
              width="12"
              height="12"
              rx="2"
              fill="none"
              stroke="#e8a33d"
              stroke-opacity="0.5"
            />
            <rect x="0.5" y="0.5" width="23" height="23" rx="4" fill="none" stroke="#2b303a" />
          </svg>
        {:else}
          <!-- the key and fit drawings: the same plate, plain geometry on it -->
          <svg viewBox="0 0 24 24">
            {#if o.icon === "key-luma"}
              <defs>
                <linearGradient id="lxKeyRamp" x1="0" y1="0" x2="1" y2="1">
                  <stop offset="0" stop-color="#e8a33d" stop-opacity="1" />
                  <stop offset="1" stop-color="#e8a33d" stop-opacity="0" />
                </linearGradient>
              </defs>
            {/if}
            <rect x="0" y="0" width="24" height="24" rx="4" fill="#0e1014" />
            {#if o.icon === "key-none" || o.icon === "key-black" || o.icon === "key-luma"}
              <rect x="2.5" y="7" width="12" height="12" rx="2" fill="#3b7ddd" />
            {/if}
            {#if o.icon === "key-none"}
              <rect x="9.5" y="5" width="12" height="12" rx="2" fill="#e8a33d" />
            {:else if o.icon === "key-black"}
              <!-- only the lit pixels survive; the base shows through the rest -->
              <path d="M15.5 6.5l4 5-4 5-4-5z" fill="#e8a33d" />
              <rect
                x="9.5"
                y="5"
                width="12"
                height="12"
                rx="2"
                fill="none"
                stroke="#e8a33d"
                stroke-opacity="0.5"
                stroke-dasharray="2 2"
              />
            {:else if o.icon === "key-luma"}
              <rect x="9.5" y="5" width="12" height="12" rx="2" fill="url(#lxKeyRamp)" />
              <rect
                x="9.5"
                y="5"
                width="12"
                height="12"
                rx="2"
                fill="none"
                stroke="#e8a33d"
                stroke-opacity="0.5"
              />
            {:else if o.icon === "sprite"}
              <!-- a sprite ROW in the scene inspector's picker (Gitea #740):
                   a few texels, which is what a sprite is. The row's `desc`
                   carries its real size and frame count. -->
              {#each [[6, 8], [10, 8], [14, 8], [6, 12], [14, 12], [10, 16]] as p (`${p[0]}:${p[1]}`)}
                <rect x={p[0]} y={p[1]} width="4" height="4" rx="0.8" fill="#e8a33d" />
              {/each}
            {:else if o.icon === "fit-once" || o.icon === "fit-contain" || o.icon === "fit-tile"}
              <!-- the dashed frame is the layer's BOX; the accent shape is
                   what the sprite does inside it. Scaling is real since
                   #740/#741, so `Stretch` fills the frame, `Fit` is the
                   uniformly-scaled square centred in it, and `Tile` repeats. -->
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
              {#if o.icon === "fit-once"}
                <rect x="5.5" y="5.5" width="13" height="13" rx="1" fill="#e8a33d" />
              {:else if o.icon === "fit-contain"}
                <rect x="7.5" y="7.5" width="9" height="9" rx="1" fill="#e8a33d" />
              {:else}
                {#each [5.5, 11, 16.5] as cy (cy)}
                  {#each [5.5, 11, 16.5] as cx (cx)}
                    <rect x={cx} y={cy} width="4" height="4" rx="1" fill="#e8a33d" />
                  {/each}
                {/each}
              {/if}
            {/if}
            <rect x="0.5" y="0.5" width="23" height="23" rx="4" fill="none" stroke="#2b303a" />
          </svg>
        {/if}
      </span>
      <span class="ometa">
        <span class="onm">{o.label}</span>
        <span class="odesc">{o.desc}</span>
      </span>
    </button>
  {/each}
</Popover>

<style>
  /* The trigger's value. `.sel` is `justify-content:space-between` with the
     chevron as background art, so the one child sits left; the overflow
     guard is belt-and-braces — the labels are short by design (#735). */
  .tval {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* A row of the chooser. Same metrics as `app.css`'s `.pop .pr` (the radio
     row the projection popup uses) except that it starts at the TOP: the
     description is two lines and the icon should line up with the name. */
  .orow {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 8px 9px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font: 13px/1.45 var(--sans);
    text-align: left;
    cursor: pointer;
  }

  .orow:hover {
    border-color: transparent;
    background: rgba(255, 255, 255, 0.04);
  }

  .orow.on {
    background: var(--accent-soft);
  }

  .ico {
    flex: none;
    display: block;
    width: 26px;
    height: 26px;
  }

  .ico svg {
    display: block;
    width: 26px;
    height: 26px;
  }

  .ometa {
    flex: 1;
    min-width: 0;
  }

  .onm {
    display: block;
    font-size: 13px;
  }

  .odesc {
    display: block;
    margin-top: 2px;
    font-size: 11.5px;
    line-height: 1.35;
    color: var(--text-dim);
  }
</style>
