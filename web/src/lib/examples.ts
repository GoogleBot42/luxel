/** The editor's initial content on first load. Every other pattern comes
 *  from the library via gallery.json (tools/gen-gallery.mjs) — this is the
 *  one built-in so the editor isn't blank before the gallery is opened.
 *
 *  It carries no geometry: the Layout is reconciled from the device (or the
 *  "Preview as" choice) and the pattern's own dimensionality — see
 *  `stores/geometry.ts`. */
export const DEFAULT_PATTERN: { name: string; source: string } = {
  name: "Rainbow",
  source: `// The canonical default pattern: a moving rainbow.
export function render(index) {
  hsv(time(.1) + index / pixelCount, 1, 1)
}
`,
};
