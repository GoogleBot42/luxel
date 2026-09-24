<script lang="ts">
  // The sprite tool row (mockup S7c `.toolrow`, Gitea #481): pencil · eraser ·
  // fill · the colour · the recent swatches · one hint.
  //
  // "The tool row sits directly above the canvas it acts on" (S7c's note), so
  // it is mounted between the preview header and the stage rather than in the
  // inspector — the inspector says what the sprite IS, this says what a click
  // on the picture DOES.
  //
  // The 16-colour palette is the EDITOR's rule, not the format's (contract
  // §4): at 16 the colour control is the one disabled thing on this screen and
  // it carries its reason, because a limit you cannot see is a bug report
  // (.claude/rules/web.md, `mockdiff --sweep`'s disabled sweep).
  import { createEventDispatcher } from "svelte";
  import SceneSwatch from "./SceneSwatch.svelte";
  import { hsvToRgb, rgbToHex, rgbToHsv, hexToRgb, type Hsv } from "../../lib/color";
  import { paletteCss, SPRITE_MAX_COLORS, type SpriteTool } from "../../lib/sprite";

  export let tool: SpriteTool = "pencil";
  /** The colour the pencil and the fill paint with, HSV 0..1. */
  export let color: Hsv = [0, 1, 1];
  /** Recently used colours, most recent first (S7c draws six). */
  export let recents: Hsv[] = [];
  /** How many distinct colours the sprite already uses. */
  export let paletteSize = 0;

  const dispatch = createEventDispatcher<{ tool: SpriteTool; color: Hsv }>();

  /** At the cap, picking a NEW colour would need a seventeenth slot. */
  $: full = paletteSize >= SPRITE_MAX_COLORS;
  $: fullReason = `this sprite already uses ${SPRITE_MAX_COLORS} colours — erase one to free a slot`;

  /** The swatch speaks hex (it is the app's ONE colour control); the format
   *  speaks HSV. 8-bit is the grain a painted pixel is stored at anyway. */
  $: hex = rgbToHex(hsvToRgb(color)).slice(1);

  function pick(e: CustomEvent<string>): void {
    const rgb = hexToRgb(`#${e.detail}`);
    if (rgb) dispatch("color", rgbToHsv(rgb));
  }

  const TOOLS: { id: SpriteTool; title: string; path: string[] }[] = [
    { id: "pencil", title: "pencil", path: ["M4 20l4-1L20 7a2 2 0 0 0-3-3L5 16z", "M15 6l3 3"] },
    { id: "eraser", title: "eraser", path: ["M7 20h13", "M5 16l7-7 6 6-5 5H7z"] },
    {
      id: "fill",
      title: "fill",
      path: ["M5 11l7-7 7 7-7 7z", "M19 15c1.4 1.9 2 2.9 2 3.8a2 2 0 1 1-4 0c0-.9.6-1.9 2-3.8z"],
    },
  ];
</script>

<div class="toolrow" data-role="sprite-tools">
  {#each TOOLS as t (t.id)}
    <button
      class="btn sm icon"
      class:on={tool === t.id}
      data-role={`sprite-tool-${t.id}`}
      aria-pressed={tool === t.id}
      title={t.title}
      on:click={() => dispatch("tool", t.id)}
    >
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        {#each t.path as d (d)}<path {d} />{/each}
      </svg>
    </button>
  {/each}

  {#if full}
    <!-- THE one disabled control on this screen, with its reason attached -->
    <span class="swatchbtn" data-role="sprite-color" data-value={hex}>
      <button
        class="swatch"
        disabled
        data-reason={fullReason}
        aria-label="sprite colour"
        title={fullReason}
        style={`background:${paletteCss(color)}`}
      ></button>
      palette full
    </span>
  {:else}
    <SceneSwatch value={hex} label="sprite colour" dataRole="sprite-color" on:input={pick} />
  {/if}

  <div class="recents" data-role="sprite-recents">
    {#each recents.slice(0, 6) as c, i (`${i}:${c[0]},${c[1]},${c[2]}`)}
      <button
        data-role="sprite-recent"
        title={`use ${paletteCss(c)}`}
        aria-label={`use ${paletteCss(c)}`}
        style={`background:${paletteCss(c)}`}
        on:click={() => dispatch("color", c)}
      ></button>
    {/each}
  </div>

  <div class="hint" data-role="sprite-tools-hint">
    click or drag on the preview to paint · erased pixels are transparent
  </div>
</div>

<style>
  /* the mock's `.recents i`, as buttons — a chip you click has to be one
     (.claude/rules/web.md) */
  .recents > button {
    display: block;
    width: 16px;
    height: 16px;
    padding: 0;
    border-radius: 3px;
    border: 1px solid rgba(255, 255, 255, 0.14);
  }

  /* the disabled twin of `.swatchbtn i`, drawn here because the picker is
     gone at the cap */
  .swatchbtn > button.swatch {
    width: 14px;
    height: 14px;
    padding: 0;
    border-radius: 3px;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }

  /* a finger needs 24px below the phone breakpoint (§5.7) */
  @media (max-width: 600px) {
    .recents > button,
    .swatchbtn > button.swatch {
      width: 24px;
      height: 24px;
    }
  }
</style>
