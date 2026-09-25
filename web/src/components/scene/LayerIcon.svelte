<script lang="ts">
  // The per-layer-type mark, in ONE place: the layer list's rows and the
  // `+ Add layer` menu's items draw the same thing at the same size, so the
  // menu entry and the row it produces are recognisably the same layer
  // (Gitea #736 items 22 + 23).
  //
  // They used to be single glyph characters — `▤ ▦ ▭ T`. Jeremy: "the icons
  // for the different layer types are terrible and should be reconsidered
  // (except for the text icon, that's good)". Two problems with them, and the
  // second is the one that bites a harness: at 11px in a mono face `▤` and
  // `▦` are a grey smudge apiece and nothing distinguishes a "pattern" from a
  // "sprite"; and the nix chromium the screenshots come from has no symbol
  // font, so the box-drawing block renders as tofu (.claude/skills/verify-webui).
  // `T` is neither — it is ASCII, it is legible, and it is the one Jeremy
  // likes — so `T` stays a letter and the other three became real marks:
  //
  //   pat    a travelling wave — a layer that is a running program
  //   sprite a 2x2 pixel block — a layer that is drawn pixels
  //   color  a filled swatch — a layer that is one flat colour
  //
  // One `<svg>` element with a per-kind path, rather than three components or
  // three `{#if}` arms: the whole file is under 400 bytes of shipped markup
  // and the bundle is gated at 983,040 B (.claude/rules/web.md).
  import type { LayerKind } from "../../lib/scene";

  export let kind: LayerKind;
  /** The edge of the square box the mark is drawn in. */
  export let size = 14;

  /** The mark per kind: `d` is a 24-box path, `stroke` says whether it is a
   *  line drawing (the wave) or a filled shape (the pixels, the swatch). */
  const MARK: Record<Exclude<LayerKind, "text">, { d: string; stroke: boolean; label: string }> = {
    pat: { d: "M3 12c3-7 6-7 9 0s6 7 9 0", stroke: true, label: "pattern layer" },
    sprite: {
      d: "M3.5 3.5h7v7h-7zM13.5 3.5h7v7h-7zM3.5 13.5h7v7h-7zM13.5 13.5h7v7h-7z",
      stroke: false,
      label: "sprite layer",
    },
    color: { d: "M4 4h16v16H4z", stroke: false, label: "color layer" },
  };

  $: mark = kind === "text" ? null : MARK[kind];
</script>

{#if mark}
  <svg
    class="lico"
    data-kind={kind}
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill={mark.stroke ? "none" : "currentColor"}
    stroke={mark.stroke ? "currentColor" : "none"}
    stroke-width={mark.stroke ? 2.4 : 0}
    stroke-linecap="round"
    role="img"
    aria-label={mark.label}
  >
    <path d={mark.d} />
  </svg>
{:else}
  <!-- the one Jeremy kept: a letter, not a glyph, so it is legible at 11px
       and present in every font a browser here might fall back to. Boxed to
       the same square as the SVGs so the four marks share one column. -->
  <span
    class="lico tglyph"
    data-kind={kind}
    style={`width:${size}px;height:${size}px`}
    role="img"
    aria-label="text layer">T</span
  >
{/if}

<style>
  .lico {
    flex: none;
    display: block;
    color: var(--text-dim);
  }

  .tglyph {
    display: flex;
    align-items: center;
    justify-content: center;
    font: 600 12px/1 var(--mono);
  }
</style>
