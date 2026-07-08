/** How pixels are arranged: a strip, a W×H grid, or an arbitrary 2D/3D
 *  map produced by the mapper (one [x,y(,z)] per pixel, any units). */
export type Layout =
  | { kind: "strip"; pixels: number }
  | { kind: "grid"; w: number; h: number }
  | { kind: "map"; coords: number[][] };

/** The editor's initial content on first load. Every other pattern comes
 *  from the library via gallery.json (tools/gen-gallery.mjs) — this is the
 *  one built-in so the editor isn't blank before the gallery is opened. */
export const DEFAULT_PATTERN: { name: string; layout: Layout; source: string } = {
  name: "Rainbow",
  layout: { kind: "strip", pixels: 60 },
  source: `// The canonical default pattern: a moving rainbow.
export function render(index) {
  hsv(time(.1) + index / pixelCount, 1, 1)
}
`,
};
