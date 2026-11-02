// The scene editor's half of the design size (`U <w> <h>`, Gitea #997,
// docs/spec/scenes.md §2 "Design size", docs/design/scenes-designed-size.md §5).
//
// The ENGINE resolves every box: `Compositor.layerRect(i)` is where a layer
// really draws, after the `U` mapping and the keyframe track, and the stage
// draws its marquee from that. What is left for the console is small and
// pure, so it lives here and is unit tested (`web/tests/scenePlace.test.mjs`):
//
//   * which Layout choices the header offers on this canvas, and which one a
//     scene's `U` line is;
//   * how many grid cells one design unit is on the canvas being previewed
//     (`designScale`) — what turns a marquee drag in cells into box numbers
//     in design units;
//   * the `×2 on this panel` line under the box fields;
//   * which sizes the `Preview at` picker offers.
//
// `designScale` restates the spec's `f` (and only `f` — the centring offset
// and the zero-axis rules stay the engine's, read back through `layerRect`).
// A drag only ever needs a DELTA, so the offset never matters here.

/** A canvas in cells: `w`×`h`, `h` 1 on a strip. */
export interface Canvas {
  w: number;
  h: number;
}

/** One segment of the header's Layout chooser. `space` null = `Pixels` (no
 *  `U` line). */
export interface LayoutChoice {
  id: string;
  label: string;
  space: [number, number] | null;
  /** What the segment's tooltip says it does. */
  title: string;
}

const MAX_SPACE = 4096;

function clampSpace(n: number): number {
  return Math.max(1, Math.min(MAX_SPACE, Math.round(n) || 1));
}

/**
 * The Layout chooser for a scene edited against `canvas` (the DEVICE's
 * canvas on a console, the `Preview as` one in the playground):
 * `Pixels | Fit 32 | Fit 64 | This panel` on a panel, and on a strip
 * `Pixels | Fit 150 | This strip` — a 32×32 design on one row is no design
 * at all (scenes-designed-size.md §3 "strip regime").
 */
export function layoutChoices(canvas: Canvas | null): LayoutChoice[] {
  const px: LayoutChoice = {
    id: "px",
    label: "Pixels",
    space: null,
    title: "boxes are this canvas’s own pixels (no design size)",
  };
  if (!canvas) return [px];
  const fit = (id: string, label: string, w: number, h: number): LayoutChoice => ({
    id,
    label,
    space: [clampSpace(w), clampSpace(h)],
    title: `boxes are a ${w}×${h} design, scaled to fit any ${h === 1 ? "strip" : "panel"}`,
  });
  if (canvas.h === 1) {
    return [px, fit("150", "Fit 150", 150, 1), fit("panel", "This strip", canvas.w, 1)];
  }
  return [
    px,
    fit("32", "Fit 32", 32, 32),
    fit("64", "Fit 64", 64, 64),
    fit("panel", "This panel", canvas.w, canvas.h),
  ];
}

/** Which choice a scene's `space` IS: the first one whose `U` matches (so a
 *  `U 64 64` scene on a 64×64 panel lights `Fit 64`, not `This panel`), or
 *  `"custom"` for a design size none of them writes (a hand-typed `U 48 40`,
 *  which the chooser then shows as its own segment rather than lying). */
export function currentChoice(space: [number, number] | undefined, choices: LayoutChoice[]): string {
  for (const c of choices) {
    if (c.space === null ? space === undefined : space !== undefined && c.space[0] === space[0] && c.space[1] === space[1])
      return c.id;
  }
  return "custom";
}

/**
 * Grid cells per design unit on `canvas`, per axis — the spec's `f`
 * (docs/spec/scenes.md §2): 1 without a `U` line and on an `X abs` layer;
 * on a strip (`rh` or `gh` 1) `gw / rw` along it and 1 across; on a panel
 * the whole-number zoom `min(gw / rw, gh / rh)` when it is at least 1, and
 * the rational `min(gw/rw, gh/rh)` below the design size.
 */
export function designScale(
  space: [number, number] | undefined,
  canvas: Canvas,
  abs = false,
): { x: number; y: number } {
  if (!space || abs) return { x: 1, y: 1 };
  const [rw, rh] = space;
  const gw = Math.max(1, canvas.w);
  const gh = Math.max(1, canvas.h);
  if (rh === 1 || gh === 1) return { x: gw / rw, y: 1 };
  const z0 = Math.min(Math.floor(gw / rw), Math.floor(gh / rh));
  const f = z0 >= 1 ? z0 : Math.min(gw / rw, gh / rh);
  return { x: f, y: f };
}

/** The content zoom the engine applies (`z = max(1, ⌊f⌋)`) — what
 *  `layerRect().zoom` answers, for the moments before a rebuild has run. */
export function designZoom(space: [number, number] | undefined, canvas: Canvas, abs = false): number {
  return Math.max(1, Math.floor(designScale(space, canvas, abs).x + 1e-9));
}

/**
 * The line under a layer's box fields in a scene with a design size: what
 * one unit of those numbers is on the canvas being previewed. `where` is
 * `on this panel` / `on this strip` while the preview IS the device, and
 * `at 128×128` while `Preview at` has re-rigged it. Empty without a `U`
 * line (the numbers are pixels and there is nothing to say).
 */
export function zoomHint(
  space: [number, number] | undefined,
  canvas: Canvas,
  zoom: number,
  abs: boolean,
  where: string,
): string {
  if (!space) return "";
  if (abs) return "grid pixels, 1:1";
  const f = designScale(space, canvas).x;
  if (zoom > 1) return `×${zoom} ${where}`;
  if (Math.abs(f - 1) < 1e-9) return `1:1 ${where}`;
  return f < 1 ? `shrunk to fit ${where}` : `scaled to fit ${where}`;
}

/** One entry of the `Preview at` picker. `id` "" = the device's own canvas. */
export interface PreviewSize {
  id: string;
  label: string;
  canvas: Canvas | null;
}

/**
 * The sizes the `Preview at` picker offers: the device's own canvas first,
 * then 32×32, 64×64 and 128×128, and 150×1 when it applies — on a strip, or
 * for a scene designed on one row. A size equal to the device's is not
 * offered twice.
 */
export function previewSizes(device: Canvas | null, space: [number, number] | undefined): PreviewSize[] {
  const own = device?.h === 1 ? "at strip" : "at panel";
  const out: PreviewSize[] = [{ id: "", label: own, canvas: null }];
  const sizes: Canvas[] = [
    { w: 32, h: 32 },
    { w: 64, h: 64 },
    { w: 128, h: 128 },
  ];
  if (device?.h === 1 || space?.[1] === 1) sizes.push({ w: 150, h: 1 });
  for (const c of sizes) {
    if (device && device.w === c.w && device.h === c.h) continue;
    out.push({ id: `${c.w}x${c.h}`, label: `at ${c.w}×${c.h}`, canvas: c });
  }
  return out;
}

/** One axis of the mapping: `v ↦ off + ⌊v·num/den⌋` (compose.rs `Axis`). */
interface Axis {
  num: number;
  den: number;
  off: number;
}

const ID: Axis = { num: 1, den: 1, off: 0 };

/** compose.rs `Place::new`, the axes only. */
function placeAxes(space: [number, number] | undefined, canvas: Canvas): { x: Axis; y: Axis } {
  if (!space) return { x: ID, y: ID };
  const [rw, rh] = [Math.max(1, space[0]), Math.max(1, space[1])];
  const { w: gw, h: gh } = canvas;
  if (gw <= 0 || gh <= 0) return { x: ID, y: ID };
  if (rh === 1 || gh === 1) return { x: { num: gw, den: rw, off: 0 }, y: ID };
  const z0 = Math.min(Math.floor(gw / rw), Math.floor(gh / rh));
  const [num, den] = z0 >= 1 ? [z0, 1] : gw * rh <= gh * rw ? [gw, rw] : [gh, rh];
  const axis = (g: number, r: number): Axis => ({ num, den, off: Math.trunc((g - Math.floor((r * num) / den)) / 2) });
  return { x: axis(gw, rw), y: axis(gh, rh) };
}

const mapV = (a: Axis, v: number, off = true): number => (off ? a.off : 0) + Math.floor((v * a.num) / a.den);

/**
 * Where a layer's `L` box lands on `canvas`, in cells — the engine's
 * `layer_rect` without the keyframe track (docs/spec/scenes.md §2). The
 * marquee normally reads the live compositor (`SceneRenderer.layerRect`,
 * which HAS the track); this is what it draws in the moments the installed
 * scene is behind the document — the second a pattern scene waits before it
 * rebuilds (`REBUILD_DELAY_MS`), during which a dragged box must still
 * follow the pointer. `natural` is a fill/contain sprite's own size when its
 * box is unset; null leaves a zero axis meaning the rest of the layout.
 */
export function mapRect(
  rect: { x: number; y: number; w: number; h: number },
  space: [number, number] | undefined,
  canvas: Canvas,
  abs: boolean,
  natural: { w: number; h: number } | null,
): { x: number; y: number; w: number; h: number } {
  const { x: ax, y: ay } = abs ? { x: ID, y: ID } : placeAxes(space, canvas);
  const isNatural = natural !== null && (rect.w === 0 || (rect.h === 0 && canvas.h !== 1));
  const span = (a: Axis, v: number, n: number): [number, number] => {
    if (n === 0) return [mapV(a, v, isNatural), 0];
    const lo = mapV(a, v);
    return [lo, Math.max(1, mapV(a, v + n) - lo)];
  };
  const [x, w] = span(ax, rect.x, rect.w);
  const [y, h] = span(ay, rect.y, rect.h);
  if (isNatural && natural) {
    const z = abs ? 1 : designZoom(space, canvas);
    return { x, y, w: natural.w * z, h: natural.h * z };
  }
  const rest = (n: number, at: number, full: number): number =>
    n !== 0 ? n : at > 0 ? Math.max(0, full - at) : full;
  return { x, y, w: rest(w, x, canvas.w), h: rest(h, y, canvas.h) };
}
