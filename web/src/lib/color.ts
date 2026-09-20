// Colour space conversions for the editor's picker (components/ColorPicker.svelte).
//
// Every value here is a UNIT float — h, s, v, r, g, b all 0..1 — because that
// is exactly what a `hsvPicker`/`rgbPicker` control carries: three 16.16
// fixed-point channels the engine reads as 0..1 (docs/spec/controls.md). The
// picker converts for DISPLAY only; what it emits is the control's own space,
// unrounded, so a device push is byte-identical to what the old raw-number
// rows produced.

export type Rgb = [number, number, number];
export type Hsv = [number, number, number];

const clamp01 = (v: number): number => (v < 0 ? 0 : v > 1 ? 1 : v);

/** Wrap a hue into 0..1 (1 reads as 0 — the wheel has no end). */
export function wrapHue(h: number): number {
  const w = h % 1;
  return w < 0 ? w + 1 : w;
}

export function hsvToRgb(hsv: Hsv): Rgb {
  const h = wrapHue(hsv[0]) * 6;
  const s = clamp01(hsv[1]);
  const v = clamp01(hsv[2]);
  const i = Math.floor(h);
  const f = h - i;
  const p = v * (1 - s);
  const q = v * (1 - s * f);
  const t = v * (1 - s * (1 - f));
  switch (i % 6) {
    case 0:
      return [v, t, p];
    case 1:
      return [q, v, p];
    case 2:
      return [p, v, t];
    case 3:
      return [p, q, v];
    case 4:
      return [t, p, v];
    default:
      return [v, p, q];
  }
}

/** The inverse. Hue is UNDEFINED for a grey (`s === 0`) and comes back 0;
 *  the picker keeps its own last hue rather than letting the wheel jump to
 *  red every time the saturation slider reaches the left edge. */
export function rgbToHsv(rgb: Rgb): Hsv {
  const r = clamp01(rgb[0]);
  const g = clamp01(rgb[1]);
  const b = clamp01(rgb[2]);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  if (d === 0) return [0, 0, max];
  let h: number;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [wrapHue(h / 6), max === 0 ? 0 : d / max, max];
}

/** `#rrggbb` for a unit-float triple — what the hex field shows. */
export function rgbToHex(rgb: Rgb): string {
  const b = rgb.map((c) => Math.round(clamp01(c) * 255));
  return `#${b.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
}

/** `#rgb`/`#rrggbb` (with or without the `#`) back to unit floats, or null
 *  if it is not one — a half-typed field must not move the colour. */
export function hexToRgb(hex: string): Rgb | null {
  const t = hex.trim().replace(/^#/, "");
  const full = t.length === 3 ? t.split("").map((c) => c + c).join("") : t;
  if (!/^[0-9a-fA-F]{6}$/.test(full)) return null;
  const n = parseInt(full, 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/** A CSS colour for a unit-float triple. */
export function cssRgb(rgb: Rgb): string {
  return `rgb(${rgb.map((c) => Math.round(clamp01(c) * 255)).join(",")})`;
}
