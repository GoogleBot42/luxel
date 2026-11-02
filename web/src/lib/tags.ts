// Gallery tags — the closed vocabulary of the `// tags:` line every
// `library/*.js` carries right after its `// name:` header, and the filter the
// Patterns page's Library source runs over it (pages/Patterns.svelte).
//
//   // name: Aurora 2D
//   // tags: panel, ambient, nature
//
// `web/tools/gen-gallery.mjs` parses the line into `gallery.json`'s `tags`
// (shape first) and `tools/check-library.sh` lints it; both carry their own
// copy of these two lists, and the three move together (library/README.md
// "Gallery tags" is the grammar). Pure, so web/tests/tags.test.mjs drives it.

/** How a pattern wants to be seen: right only on a 1-D strip, needs a 2-D map,
 *  or right either way. Exactly one per pattern. */
export const SHAPES = ["strip", "panel", "both"] as const;
export type ShapeTag = (typeof SHAPES)[number];

/** What a pattern is — one or more per pattern. Display order. */
export const THEMES = ["ambient", "nature", "sound", "game", "retro", "clock", "holiday"] as const;
export type ThemeTag = (typeof THEMES)[number];

/** The Patterns page's shape control. `strip` offers strip + both, `panel`
 *  offers panel + both. */
export type ShapeChoice = "all" | "strip" | "panel";

export const isShape = (t: string): t is ShapeTag => (SHAPES as readonly string[]).includes(t);
export const isTheme = (t: string): t is ThemeTag => (THEMES as readonly string[]).includes(t);

/** The shape tag of an entry, or `null` for an untagged one (a device or
 *  corpus pattern, or an old bundle's gallery.json). */
export function shapeOf(tags: readonly string[] | undefined): ShapeTag | null {
  return tags?.find(isShape) ?? null;
}

/** Does an entry belong under this shape choice? An UNTAGGED entry always
 *  does — the control filters what it knows about, and never hides a pattern
 *  for having no line. */
export function shapeMatches(tags: readonly string[] | undefined, choice: ShapeChoice): boolean {
  if (choice === "all") return true;
  const s = shapeOf(tags);
  return s === null || s === "both" || s === choice;
}

/** OR over the picked themes; none picked = every theme. An untagged entry
 *  matches no theme chip. */
export function themeMatches(tags: readonly string[] | undefined, picked: readonly string[]): boolean {
  if (picked.length === 0) return true;
  return tags !== undefined && picked.some((t) => tags.includes(t));
}

/** Per-theme counts over the entries under `choice` — the chips' badges.
 *  Independent of which themes are picked (OR chips count what clicking them
 *  would ADD, so a badge never reads 0 just because another chip is on). */
export function themeCounts(
  all: readonly (readonly string[] | undefined)[],
  choice: ShapeChoice,
): Record<ThemeTag, number> {
  const out = Object.fromEntries(THEMES.map((t) => [t, 0])) as Record<ThemeTag, number>;
  for (const tags of all) {
    if (!tags || !shapeMatches(tags, choice)) continue;
    for (const t of tags) if (isTheme(t)) out[t]++;
  }
  return out;
}

/** The shape a console opens on when the user has not picked one for this
 *  device: a strip offers strip patterns, a matrix panel ones, anything else
 *  (a 3D lattice, the playground) everything. */
export function defaultShape(dims: number | null | undefined): ShapeChoice {
  return dims === 1 ? "strip" : dims === 2 ? "panel" : "all";
}

/** `localStorage` key for a device's explicit shape choice. */
export const shapeKey = (device: string): string => `luxel.libShape.${device}`;

export function parseShapeChoice(v: string | null | undefined): ShapeChoice | null {
  return v === "all" || v === "strip" || v === "panel" ? v : null;
}
