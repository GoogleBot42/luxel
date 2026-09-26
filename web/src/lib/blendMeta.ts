// Plain-English copy for the three compositing choosers — Blend, Transparent
// and Fit — plus the id of the little drawing each one gets.
//
// Jeremy, 2026-09-24 (Gitea #735): "Most users won't know what those mean. So
// there should be descriptions and probably even little svg graphics for each
// option." The words are §5.5 of docs/design/webui-v2/proposal.md condensed
// to one line apiece; the drawings are `components/RichSelect.svelte`, which
// renders an `icon` id rather than an SVG string so the five blend swatches
// can share ONE `<svg>` (the bundle is CI-gated, #592).
//
// Keep this file pure data: no stores, no Svelte, no fetch — it is imported
// by four inspectors and by the tests.

import type { Blend, Fit, LayerKey } from "./scene";

/** One row of a `RichSelect`: what it writes, what it is called, what it
 *  does, and which drawing goes beside it. */
export interface RichOption<T extends string = string> {
  value: T;
  label: string;
  desc: string;
  /** A drawing `components/RichSelect.svelte` knows how to render. */
  icon: string;
}

/**
 * The five blends, in `BLENDS` order. The formulae behind the sentences
 * (B = what is beneath, L = the layer, α = opacity) are §5.5's:
 *   normal `B + α(L−B)` · add `min(255, B + αL)` · lighten `max(B, αL)` per
 *   channel · multiply `B·L/255` faded to B by α · mask `B·luma(L)/255`.
 * `blend_px_mode` in crates/luxel-core/src/compose.rs is the implementation.
 */
export const BLEND_OPTIONS = [
  {
    value: "normal",
    label: "Normal",
    desc: "Paints over what is beneath — the everyday mode, and the only one where Transparent matters.",
    icon: "normal",
  },
  {
    value: "add",
    label: "Add",
    desc: "Light adds up; black costs nothing. Sparkles or a meteor over a wash.",
    icon: "add",
  },
  {
    value: "lighten",
    label: "Lighten",
    desc: "Keeps whichever is brighter, per channel. Never clips — the cheap way to share a panel.",
    icon: "lighten",
  },
  {
    value: "multiply",
    label: "Multiply",
    desc: "White leaves what is beneath alone, black kills it. A tint, a shadow, a spotlight.",
    icon: "multiply",
  },
  {
    value: "mask",
    label: "Mask",
    desc: "A brightness-only stencil — text over a rainbow base gives rainbow-filled letters.",
    icon: "mask",
  },
] as const satisfies readonly RichOption<Blend>[];

/**
 * The transparency KEY — which of the layer's pixels count at all. Offered
 * only under Normal blending (§5.5: the other modes carry their own).
 */
export const KEY_OPTIONS = [
  {
    value: "none",
    label: "Nothing",
    desc: "Every pixel in the box counts, black included. A base layer, or a wash at low opacity.",
    icon: "key-none",
  },
  {
    value: "black",
    label: "Black pixels",
    desc: "Exactly-black pixels are skipped. Hard-edged and cheap — sprites, text, comets on black.",
    icon: "key-black",
  },
  {
    value: "luma",
    label: "By brightness",
    desc: "Brightness becomes opacity: black vanishes, white is solid. Right for glows and fire.",
    icon: "key-luma",
  },
] as const satisfies readonly RichOption<LayerKey>[];

/* Exhaustiveness, at compile time rather than in a test: the two lists above
   must between them name every member of their wire union, so the day
   `BLENDS` or `KEYS` grows in lib/scene.ts and this file does not,
   `svelte-check` fails HERE — where the missing copy is — instead of the
   console quietly showing a raw wire word in the trigger. The `as const` on
   each list is what gives these teeth. */
const _blendsCovered: Blend extends (typeof BLEND_OPTIONS)[number]["value"] ? true : never = true;
const _keysCovered: LayerKey extends (typeof KEY_OPTIONS)[number]["value"] ? true : never = true;
void _blendsCovered;
void _keysCovered;

/**
 * FIT.
 *
 * `style.fit` is read in exactly ONE place — `compose::blit_sprite` — so it
 * means something on a SPRITE layer and nothing anywhere else:
 *   * pattern — the box CLIPS a full-layout render; the pattern still sees
 *     the whole grid. `fit` is ignored (docs/spec/scenes.md "Geometry").
 *   * text — ignored.
 *   * sprite — all three words differ now.
 *
 * SCALING IS REAL SINCE Gitea #740/#741 (item 29: "'1:1 · sprites are never
 * scaled' should that be an option?" — yes). The old note here said `contain`
 * was "a synonym of `fill` on every code path there is", which was true of
 * the firmware that existed then and is not true any more: with a box set,
 * `fill` STRETCHES the frame to it, `contain` scales uniformly and centres,
 * `tile` repeats at 1:1. An UNSET box (w or h = 0) means the sprite's natural
 * size at (x, y), which is why the chooser only appears once the box has one.
 */
export const FIT_OPTIONS: readonly RichOption<Fit>[] = [
  {
    value: "fill",
    label: "Stretch",
    desc: "Stretches the sprite to fill the box exactly, squashing it if the shapes differ.",
    icon: "fit-once",
  },
  {
    value: "contain",
    label: "Fit",
    desc: "Scales the sprite up or down to fit inside the box, keeping its proportions, centred.",
    icon: "fit-contain",
  },
  {
    value: "tile",
    label: "Tile",
    desc: "Repeats across the box at its own size in both directions — a small sprite becomes wallpaper.",
    icon: "fit-tile",
  },
];

/* Exhaustiveness at compile time, the twin of the two checks above: the day
   `FITS` grows in lib/scene.ts, `svelte-check` fails HERE rather than the
   console showing a raw wire word. */
const _fitsCovered: Fit extends (typeof FIT_OPTIONS)[number]["value"] ? true : never = true;
void _fitsCovered;

/** The row a stored `fit` selects. Every wire word has its own row now, so
 *  this is the identity — kept as the ONE place a future fold would live, and
 *  because four call sites already read `fit` through it. */
export function fitValue(fit: Fit): Fit {
  return fit;
}

/** `label` of the option carrying `value`, or the raw value if the list has
 *  no row for it (a record written by a newer firmware). */
export function labelOf(options: readonly RichOption[], value: string): string {
  return options.find((o) => o.value === value)?.label ?? value;
}
