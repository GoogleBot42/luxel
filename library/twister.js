// name: Twister
// Curated original (hand-written showcase of the Luxel language/builtins).
// The Amiga twister: a square column writhes like taffy, its twist rippling
// up the height. Per row the four rotated corners are projected; each face
// whose left edge sits left of its right edge is visible and lit by a key
// light from the front-left (OKLCH lightness, a specular glint near-on).
// renderBulk: a sky gradient, then <= 2 fillRect spans per row — no per-pixel work.

var speed = 0.3     // column spin, turns per second
var ripple = 55     // degrees of ripple amplitude
var twist = 120     // degrees of static twist, bottom to top
var width = 46      // % of the panel the column's diagonal spans
var palette = 0

var spin = 0, ph = 0, ph2 = 0, rows = 48, half = 0.01, h0 = 0
var FH = array(16)  // face hues per palette: Amiga, Neon, Chrome, Rainbow
var FC = array(4)   // chroma per palette
FH[0] = 0.08; FH[1] = 0.27; FH[2] = 0.42; FH[3] = 0.70
FH[4] = 0.92; FH[5] = 0.55; FH[6] = 0.80; FH[7] = 0.62
FH[8] = 0.65; FH[9] = 0.62; FH[10] = 0.66; FH[11] = 0.60
FC[0] = 0.17; FC[1] = 0.2; FC[2] = 0.035; FC[3] = 0.16

export function beforeRender(delta) {
  var dt = delta * 0.001
  spin = mod(spin + speed * dt, 1)
  ph = mod(ph + dt * 0.37, 1)
  ph2 = mod(ph2 + dt * 0.11, 1)
  h0 = mod(h0 + dt * 0.04, 1)
  rows = gridHeight() > 1 ? min(gridHeight(), 256) : 48
  half = 0.46 / (rows - 1)
}

export function renderBulk() {
  fillGradient(0.68 + 0.05 * sin(ph2 * PI2), 0.85, 0.16, 0.80, 0.9, 0.05, 2)
  var rd = width * 0.005, pal = palette * 4, ch = FC[palette]
  var r, y, a, cx, rr, i, xa, xb, f, d, l, hu
  for (r = 0; r < rows; r++) {
    y = r / (rows - 1)
    a = (spin + twist / 360 * (1 - y)) * PI2 + ripple * 0.01745 * sin((y * 1.1 + ph) * PI2)
    cx = 0.5 + 0.07 * sin((y * 0.8 + ph2) * PI2)
    rr = rd * (1 + 0.06 * sin((y * 2.3 - ph) * PI2))
    xb = cx + rr * sin(a)
    for (i = 0; i < 4; i++) {
      xa = xb
      xb = cx + rr * sin(a + (i + 1) * PI / 2)
      if (xb > xa) {
        f = (xb - xa) / (rr * SQRT2)                 // facing the viewer
        d = (xa + xb - 2 * cx) / (rr * SQRT2)       // sideways normal
        l = max(0, 0.86 * f - 0.5 * d)              // key light front-left
        hu = palette == 3 ? h0 + i * 0.25 + y * 0.35 : FH[pal + i]
        oklch(0.16 + 0.6 * l + 0.24 * pow(l, 8), ch * (1 - 0.5 * pow(l, 8)), hu)
        fillRect(xa, y - half, xb, y + half)
      }
    }
  }
}

//# min=0 max=1.5 step=0.05 default=0.3
export function sliderSpeed(v) { speed = clamp(v, 0, 1.5) }

//# min=0 max=180 step=5 default=55
export function sliderRipple(v) { ripple = clamp(v, 0, 180) }

//# min=-360 max=360 step=10 default=120
export function sliderTwist(v) { twist = clamp(v, -360, 360) }

//# min=20 max=80 step=1 default=46
export function sliderWidth(v) { width = clamp(v, 20, 80) }

//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }
