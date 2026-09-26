// Display preferences: the user's, not the pattern's and not the device's.
//
// Everything here is per BROWSER — persisted in `localStorage` through
// `lib/store.ts` and never sent anywhere. The Layout lives in
// `stores/geometry.ts` because it changes what is rendered; these only change
// how it is drawn.

import { writable, type Writable } from "svelte/store";
import { DEFAULT_PREVIEW_STYLE, type PreviewStyle } from "../lib/draw";
import { loadPreviewStyle, savePreviewStyle } from "../lib/store";

/** How the 2D grid preview is drawn: hard `squares` (the default, and what
 *  every thumbnail and tile always uses) or the HUB75-like `panel` look —
 *  dots on a black substrate with a bloom (Gitea #786). */
export const previewStyle: Writable<PreviewStyle> = writable(
  loadPreviewStyle() ?? DEFAULT_PREVIEW_STYLE,
);
previewStyle.subscribe((v) => savePreviewStyle(v));

/** The label the chooser and the Settings status row both show. */
export function previewStyleLabel(style: PreviewStyle): string {
  return style === "panel" ? "LED panel" : "Squares";
}
