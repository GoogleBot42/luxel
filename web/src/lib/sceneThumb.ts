// `sceneThumb(scene, w, h)` — ONE composite frame of a scene, for a surface
// that wants a picture rather than an animation (Gitea #482: the playlist
// row's thumbnail, the picker's scene rows).
//
// This is the small, stable entry point other screens import; the machinery
// (the compositor, one engine per pattern/sprite layer, the host half of a
// text layer) lives in `lib/sceneRender.ts`. A grid of LIVE tiles keeps a
// `SceneRenderer` per tile instead — building and freeing N engines per frame
// is not a thing to do sixty times a second.

import { SceneRenderer, type SourceLookup } from "./sceneRender";
import type { Luxel } from "./luxel";
import type { Scene } from "./scene";
import { layoutFor, thumbLayout, type Layout } from "../stores/geometry";

export type { SourceLookup };
export { SceneRenderer };

/**
 * Render `scene` once, at `w`×`h`, and hand back the RGB bytes plus the rig
 * they were drawn on — both of which go straight into `paintGrid` from
 * `lib/draw.ts`.
 *
 * `lookup` resolves a store id to its pattern SOURCE: on a console that is
 * `stores/device.ts`'s `devicePatterns`, in the playground the local library
 * keyed by `playgroundPatternId`. Returns null when the scene does not parse
 * or this build has no compositor — the caller shows a blank thumbnail, the
 * way a pattern tile does while its source is still in flight.
 *
 * Costly: it builds and frees a wasm engine per pattern layer. Call it once
 * per row, not per frame.
 */
export function sceneThumb(
  lx: Luxel,
  scene: Scene,
  lookup: SourceLookup,
  w: number,
  h: number,
): { px: Uint8Array; layout: Layout } | null {
  return sceneThumbOn(lx, scene, lookup, gridRig(w, h));
}

/** The same, on a Layout you already have (the device's, shrunk to a tile). */
export function sceneThumbOn(
  lx: Luxel,
  scene: Scene,
  lookup: SourceLookup,
  rig: Layout,
  maxCells = 0,
): { px: Uint8Array; layout: Layout } | null {
  const small = maxCells > 0 ? thumbLayout(rig, maxCells) : rig;
  const r = new SceneRenderer(lx, small);
  try {
    if (r.setScene(scene, lookup)) return null;
    // Two frames: the first runs each pattern's top-level init (and the
    // compositor's first `advance`), the second is the one with motion in it.
    r.frame(0);
    const px = r.frame(33);
    return px ? { px, layout: small } : null;
  } finally {
    r.free();
  }
}

/** A plain `w`×`h` row-major rig — the shape a scene is always composited on
 *  (`docs/spec/scenes.md` §2). Built from the app's current Layout so the
 *  projection defaults and the wiring come from the ONE reconciler. */
export function gridRig(w: number, h: number): Layout {
  const base = layoutFor(2);
  return { ...base, dims: 2, regular: true, w, h, d: 1, pixels: w * h, coords: undefined };
}
