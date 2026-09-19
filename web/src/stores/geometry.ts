// Geometry: the preview rig and how it is chosen.
//
// PLACEHOLDER (Gitea #462 → #463). Today this is exactly the rig derivation
// that lived in App.svelte — `layout` plus the #372 derive-once latch — moved
// verbatim so that A2 (#463) can replace the *whole file* with the real
// reconciler (device Layout × Engine.preferredDims() × user override) without
// touching a single consumer. Every consumer already reads it from here.
//
// Known limits, all inherited and all A2's to fix (research/ui-audit.md §3):
//   * `deriveRig` only ever UPGRADES a strip, and only to a grid (or a cloud
//     in the playground).
//   * `deviceMap` is consulted only when it is a procedural `grid W H`.
//   * There are still three sources of truth for geometry (`layout`,
//     `devicePixels`, `deviceMap`).

import { derived, get, writable, type Readable, type Writable } from "svelte/store";
import { DEFAULT_PATTERN, type Layout } from "../lib/examples";
import type { Engine } from "../lib/luxel";
import { device, deviceMap, devicePixels } from "./device";

export type { Layout };

/** The preview rig: how the local engine arranges its pixels. */
export const layout: Writable<Layout> = writable(DEFAULT_PATTERN.layout);

/** Total pixels the current rig declares. */
export function pixelCount(l: Layout = get(layout)): number {
  return l.kind === "strip" ? l.pixels : l.kind === "grid" ? l.w * l.h : l.coords.length;
}

/** Reactive form of `pixelCount` — the Settings readout tracks the rig. */
export const pixelTotal: Readable<number> = derived(layout, (l) => pixelCount(l));

/** The user picked a rig by hand for the pattern in the editor, so nothing
 *  derived from the source may move it (Gitea #372). Cleared by every load of
 *  a different pattern — a new pattern is a new choice. */
let rigChosen = false;
/** A pattern was just LOADED (pasted, imported, opened from the library or the
 *  device, restored from a share link), so the rig is re-derived on the next
 *  successful compile. Never set by ordinary typing: the rig must not move
 *  under someone mid-edit. */
let rigDerivePending = false;

/** A different pattern arrived (gallery/library/device pick, .epe import,
 *  share link, device connect): re-derive the rig, and forget any manual rig
 *  choice — it belonged to the pattern being replaced. */
export function markPatternLoaded(): void {
  rigChosen = false;
  rigDerivePending = true;
}

/** A paste landed in the editor: re-derive, but this is an edit to the pattern
 *  already open, so a rig its user chose by hand still stands. */
export function markSourcePasted(): void {
  rigDerivePending = true;
}

/** An explicit pick outranks anything derived (#372). */
export function markRigChosen(): void {
  rigChosen = true;
}

/** Consume the derive-once latch: true exactly once per pattern load. */
export function takeRigDerivePending(): boolean {
  const pending = rigDerivePending;
  rigDerivePending = false;
  return pending;
}

/** n×n×n lattice map — the default geometry for render3D patterns. */
export function cubeLattice(n: number): number[][] {
  const coords: number[][] = [];
  for (let z = 0; z < n; z++)
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) coords.push([x, y, z]);
  return coords;
}

/** Pick the preview rig from the COMPILED pattern (#372): a `render2D`
 *  pattern — or a `renderFrame` one that draws in coordinate/grid space —
 *  wants a grid; a `render3D`-only pattern wants the rotating point cloud.
 *  Read off the compiled program, so a `render2D` inside a comment or a string
 *  never counts, and every load path gets what a gallery pick has always got
 *  from the manifest's `kind`.
 *
 *  Only ever upgrades a STRIP: a grid, a 2D map, or a rig the user chose by
 *  hand is left exactly as it is. Returns true when `layout` changed, so the
 *  caller can rebuild the engine at the new geometry. */
export function deriveRig(e: Engine): boolean {
  const l = get(layout);
  if (rigChosen || l.kind !== "strip") return false;
  const dims = e.preferredDims();
  const dm = get(deviceMap);
  const connected = get(device) !== null;
  if (dims === 2) {
    // A connected device's own matrix geometry beats the 16×16 default:
    // previewing what the hardware will actually show is the whole point.
    const dw = dm.kind === "grid" ? (dm.w ?? 0) : 0;
    const dh = dm.kind === "grid" ? (dm.h ?? 0) : 0;
    let w = 16;
    let h = 16;
    if (dw > 0 && dh > 0) {
      w = dw;
      h = dh;
    } else if (connected) {
      // no map installed: the same square the manual selector would build
      // from the hardware pixel count
      const side = Math.max(2, Math.round(Math.sqrt(get(devicePixels))));
      w = side;
      h = side;
    }
    layout.set({ kind: "grid", w, h });
    return true;
  }
  if (dims === 3 && !connected) {
    // The rig a gallery "cloud" pick installs. Playground only: on a device
    // the pixel count is hardware truth and a 125-point lattice is not it.
    layout.set({ kind: "map", coords: cubeLattice(5) });
    return true;
  }
  return false;
}
