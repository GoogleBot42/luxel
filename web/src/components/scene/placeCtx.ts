// What a layer inspector's box row needs to know about the scene's design
// size (Gitea #997) — handed down by `pages/SceneEditor.svelte` through
// Svelte context rather than through four inspectors' props, because only
// `BoxRow` reads it and none of the inspectors has anything to do with it.
import type { Readable } from "svelte/store";

export const PLACE_CTX = "luxel.scene-place";

export interface BoxPlace {
  /** The scene's `U` line; absent = `Pixels`, and the row says nothing. */
  space?: [number, number];
  /** The selected layer is `X abs` ("Pin to pixels"). */
  abs: boolean;
  /** `×2 on this panel` — `lib/scenePlace.ts` `zoomHint`. */
  hint: string;
}

export interface PlaceCtx {
  place: Readable<BoxPlace>;
  /** Pin (or unpin) the selected layer: `X abs`. */
  setAbs(on: boolean): void;
}
