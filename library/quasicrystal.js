// name: Quasicrystal
// tags: panel, ambient
// Original work from the prose idea spec docs/pattern-specs/patternflow/nodal-lattice.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// Plane waves at evenly spaced angles summed into a rosette lattice that never
// repeats: flowers, stars and wheels bloom out of the centre as the shared phase
// runs, and the whole star slowly turns. Bands folds the sum through a palette;
// Nodal lines lights only where the waves cancel, as thin glowing curves of
// constant width. Per pixel: one wave() per direction, all else is per-frame.

var style = 0
var sym = 5
var wl = 12
var bloom = 10
var palette = 0

// 0 bands · 1 nodal lines
//# min=0 max=1 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 1) }
// Wave directions (the rosette's fold); fixtures over 8192 px use at most 5.
//# min=5 max=9 step=1 default=5
export function sliderSymmetry(v) { sym = clamp(floor(v), 5, 9) }
// Crest-to-crest spacing of each wave; bigger fixtures show more of the never-repeating lattice.
//# min=6 max=32 step=1 default=12
export function sliderWavelengthPx(v) { wl = clamp(v, 6, 32) }
// Seconds for one bloom of the shared phase.
//# min=2 max=60 step=1 default=10
export function sliderBloomS(v) { bloom = clamp(v, 2, 60) }
// 0 drifting hue · 1 ice · 2 ember
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

var pal = array(128), tmp = array(3)
var n = 5, ph = 0, rot = 0, hueT = 0, aspect = 1, half = 2.5, lw = 1, F = 2.7
var a0, b0, a1, b1, a2, b2, a3, b3, a4, b4, a5, b5, a6, b6, a7, b7, a8, b8

function okRGB(L, C, h, o) {   // OKLCH -> gamma sRGB, Ottosson's OKLab matrices
  var a = C * cos(h * PI2), b = C * sin(h * PI2)
  var l = L + 0.3963 * a + 0.2158 * b, m = L - 0.1056 * a - 0.0639 * b, s = L - 0.0895 * a - 1.2915 * b
  l = l * l * l; m = m * m * m; s = s * s * s
  o[0] = pow(clamp(4.0767 * l - 3.3077 * m + 0.2310 * s, 0, 1), 0.4167)
  o[1] = pow(clamp(-1.2684 * l + 2.6098 * m - 0.3413 * s, 0, 1), 0.4167)
  o[2] = pow(clamp(-0.0042 * l - 0.7034 * m + 1.7076 * s, 0, 1), 0.4167)
}

setPalette(pal)

function dirA(j) { return F * cos((rot + j / n) * PI) }
function dirB(j) { return F * aspect * sin((rot + j / n) * PI) }

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  ph = mod(ph + dt / bloom, 1)
  rot = mod(rot + dt / 240, 2)
  hueT = mod(hueT + dt / 45, 1)
  n = pixelCount > 8192 ? 5 : sym
  half = n / 2
  var gw = gridWidth()
  aspect = gw > 0 ? gridHeight() / gw : 1
  F = (gw > 0 ? gw : 32) / wl            // crests per panel width
  // nodal line half-width: about 1.8 px of the field's typical slope
  lw = 1.8 * sqrt(n / 2) * PI / wl
  a0 = dirA(0); b0 = dirB(0); a1 = dirA(1); b1 = dirB(1); a2 = dirA(2); b2 = dirB(2)
  a3 = dirA(3); b3 = dirB(3); a4 = dirA(4); b4 = dirB(4); a5 = dirA(5); b5 = dirB(5)
  a6 = dirA(6); b6 = dirB(6); a7 = dirA(7); b7 = dirB(7); a8 = dirA(8); b8 = dirB(8)
  for (var k = 0; k < 32; k++) {
    var p = k / 31, bump = 4 * p * (1 - p)
    if (palette == 0) okRGB(0.05 + 0.9 * p, 0.07 + 0.11 * bump, hueT + 0.3 * p, tmp)
    else if (palette == 1) okRGB(0.05 + 0.92 * p, 0.03 + 0.12 * (1 - p), 0.64 - 0.1 * p, tmp)
    else okRGB(0.04 + 0.9 * p, 0.03 + 0.17 * bump, 0.08 + 0.17 * p, tmp)
    pal[k * 4] = p; pal[k * 4 + 1] = tmp[0]; pal[k * 4 + 2] = tmp[1]; pal[k * 4 + 3] = tmp[2]
  }
}

export function render2D(index, x, y) {
  var X = x - 0.5, Y = y - 0.5
  var s = wave(X * a0 + Y * b0 + ph) + wave(X * a1 + Y * b1 + ph) + wave(X * a2 + Y * b2 + ph)
  s += wave(X * a3 + Y * b3 + ph) + wave(X * a4 + Y * b4 + ph)
  if (n > 5) s += wave(X * a5 + Y * b5 + ph)
  if (n > 6) s += wave(X * a6 + Y * b6 + ph)
  if (n > 7) s += wave(X * a7 + Y * b7 + ph)
  if (n > 8) s += wave(X * a8 + Y * b8 + ph)
  if (style == 0) {
    var v = triangle(s * 0.5)
    paint(v, 1)
  } else {
    var b = 1 - smoothstep(0, lw, abs(s - half))
    b = b * b
    paint(0.35 + 0.65 * b, 0.15 + 0.85 * b)     // lines over a dim wash of the same ramp
  }
}
