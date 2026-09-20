// How a frame of RGB bytes is painted for each Layout shape (Gitea #463).
//
// One implementation per shape, shared by the editor's preview, the gallery
// tiles and the row thumbnails, so a pattern looks the same wherever it is
// shown and a fix lands everywhere at once. The canvas's INTRINSIC size is
// the pixel grid; CSS decides how big it appears (`image-rendering:
// pixelated`), which is why nothing here scales.

/** One pixel row: the canvas becomes `n`×1 and CSS stretches it. */
export function paintBar(c: HTMLCanvasElement, px: Uint8Array): void {
  const ctx = c.getContext("2d");
  if (!ctx) return;
  const n = Math.max(1, Math.floor(px.length / 3));
  if (c.width !== n || c.height !== 1) {
    c.width = n;
    c.height = 1;
  }
  ctx.putImageData(toImageData(ctx, px, n, 1), 0, 0);
}

/** A `w`×`h` matrix, row-major — the canvas becomes exactly that. */
export function paintGrid(c: HTMLCanvasElement, px: Uint8Array, w: number, h: number): void {
  const ctx = c.getContext("2d");
  if (!ctx || w <= 0 || h <= 0) return;
  if (c.width !== w || c.height !== h) {
    c.width = w;
    c.height = h;
  }
  ctx.putImageData(toImageData(ctx, px, w, h), 0, 0);
}

function toImageData(
  ctx: CanvasRenderingContext2D,
  px: Uint8Array,
  w: number,
  h: number,
): ImageData {
  const img = ctx.createImageData(w, h);
  const n = Math.min(Math.floor(px.length / 3), w * h);
  for (let i = 0; i < n; i++) {
    img.data[i * 4] = px[i * 3] ?? 0;
    img.data[i * 4 + 1] = px[i * 3 + 1] ?? 0;
    img.data[i * 4 + 2] = px[i * 3 + 2] ?? 0;
    img.data[i * 4 + 3] = 255;
  }
  return img;
}

/** Pixel positions normalized into a centred unit cube. `is3D` is false when
 *  the z axis does not vary — a flat scatter, which must not be rotated. */
export interface PointRig {
  pts: { x: number; y: number; z: number }[];
  is3D: boolean;
}

export function normalizePoints(coords: number[][]): PointRig {
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (const c of coords) {
    for (let d = 0; d < 3; d++) {
      const v = c[d] ?? 0;
      lo[d] = Math.min(lo[d]!, v);
      hi[d] = Math.max(hi[d]!, v);
    }
  }
  const is3D = hi[2]! - lo[2]! > 1e-6;
  const s = [hi[0]! - lo[0]! || 1, hi[1]! - lo[1]! || 1, hi[2]! - lo[2]! || 1];
  // centred on 0 so rotation is about the middle; y flipped (screen down)
  const pts = coords.map((c) => ({
    x: ((c[0] ?? 0) - lo[0]!) / s[0]! - 0.5,
    y: ((c[1] ?? 0) - lo[1]!) / s[1]! - 0.5,
    z: is3D ? ((c[2] ?? 0) - lo[2]!) / s[2]! - 0.5 : 0,
  }));
  return { pts, is3D };
}

/** Is pixel `i` emitting anything at all? An OFF pixel is not a black dot the
 *  viewer is meant to see through a lit one — it is a pixel that is simply not
 *  on, and the black disc it used to paint punched holes in whatever was lit
 *  behind it (Jeremy, 2026-09-19: "black scatter plot dots draw over other
 *  dots"). */
export function isLit(px: Uint8Array, i: number): boolean {
  return (px[i * 3] ?? 0) > 0 || (px[i * 3 + 1] ?? 0) > 0 || (px[i * 3 + 2] ?? 0) > 0;
}

/** The order a scatter is painted in: every UNLIT point first, then the lit
 *  ones, each group still back-to-front so the depth cue survives. Sorting by
 *  depth alone is not enough — an unlit point that happens to be nearer than a
 *  lit neighbour legitimately sorts on top of it, and paints it out. Pure and
 *  exported so the rule has a unit test (web/tests/draw.test.mjs). */
export function paintOrder<T extends { i: number; depth: number }>(
  items: T[],
  px: Uint8Array,
): T[] {
  return [...items].sort((a, b) => {
    const la = isLit(px, a.i);
    const lb = isLit(px, b.i);
    if (la !== lb) return la ? 1 : -1;
    return a.depth - b.depth;
  });
}

/** A point cloud (3D lattice or custom map): orthographic projection with a
 *  fixed tilt, painter's algorithm and a depth cue. `angle` rotates a 3D rig
 *  about the vertical axis; a flat scatter ignores it. */
export function paintPoints(
  c: HTMLCanvasElement,
  px: Uint8Array,
  rig: PointRig,
  angle: number,
): void {
  const ctx = c.getContext("2d");
  if (!ctx) return;
  const { width: w, height: h } = c;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, w, h);
  const n = rig.pts.length;
  if (n === 0) return;
  const baseR = Math.max(1, Math.min(7, (Math.min(w, h) / Math.sqrt(n)) * 0.3));

  if (!rig.is3D) {
    // A flat scatter has no depth at all, so index order WAS the paint order:
    // a dark pixel late in the strip covered a lit one that overlapped it.
    // Same rule as the cloud below — unlit first (`depth` is a constant here).
    const pad = Math.max(2, Math.round(w * 0.025));
    const span = w - 2 * pad;
    const flat: { i: number; depth: number }[] = [];
    for (let i = 0; i < n; i++) if (rig.pts[i]) flat.push({ i, depth: 0 });
    for (const q of paintOrder(flat, px)) {
      const p = rig.pts[q.i]!;
      ctx.fillStyle = `rgb(${px[q.i * 3] ?? 0},${px[q.i * 3 + 1] ?? 0},${px[q.i * 3 + 2] ?? 0})`;
      ctx.beginPath();
      ctx.arc(pad + (p.x + 0.5) * span, pad + (p.y + 0.5) * span, baseR, 0, Math.PI * 2);
      ctx.fill();
    }
    return;
  }

  const ca = Math.cos(angle);
  const sa = Math.sin(angle);
  const ct = Math.cos(0.45); // fixed tilt
  const st = Math.sin(0.45);
  const scale = Math.min(w, h) * 0.72;
  const proj: { sx: number; sy: number; depth: number; i: number }[] = [];
  for (let i = 0; i < n; i++) {
    const p = rig.pts[i];
    if (!p) continue;
    const x = p.x * ca - p.z * sa; // rotate about Y
    const z = p.x * sa + p.z * ca;
    proj.push({
      sx: w / 2 + x * scale,
      sy: h / 2 + (p.y * ct - z * st) * scale, // tilt about X
      depth: p.y * st + z * ct,
      i,
    });
  }
  // back to front, with every unlit point painted before any lit one
  for (const q of paintOrder(proj, px)) {
    const cue = Math.max(0.35, Math.min(1, 0.55 + 0.45 * (q.depth + 0.6)));
    const r = baseR * Math.max(0.6, Math.min(1.3, cue));
    ctx.fillStyle = `rgb(${Math.round((px[q.i * 3] ?? 0) * cue)},${Math.round(
      (px[q.i * 3 + 1] ?? 0) * cue,
    )},${Math.round((px[q.i * 3 + 2] ?? 0) * cue)})`;
    ctx.beginPath();
    ctx.arc(q.sx, q.sy, r, 0, Math.PI * 2);
    ctx.fill();
  }
}
