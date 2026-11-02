// name: Bokeh
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// Out-of-focus city lights: big soft discs drift and breathe, warm amber and
// cool teal, overlapping into glowing clusters. Like real lens bokeh each disc
// has a brighter rim — built from a ring of additive splats around a dim core,
// so a hexagonal aperture (Shape 1) falls out of the same code with 6 splats.
// Nearer discs are bigger, dimmer and slower.

var MAXD = 30
var dx = array(MAXD), dy = array(MAXD), dd = array(MAXD), da = array(MAXD)
var dh = array(MAXD), dp = array(MAXD)

var discs = 22
//# min=6 max=30 step=1 default=22
export function sliderDiscs(v) { discs = clamp(floor(v), 6, MAXD) }

// Typical disc radius as a percentage of the panel width.
var sizePct = 10
//# min=3 max=20 step=0.5 default=10
export function sliderSizePct(v) { sizePct = clamp(v, 3, 20) }

// Drift speed in percent of the panel per second.
var driftPct = 3
//# min=0 max=20 step=0.5 default=3
export function sliderDriftPct(v) { driftPct = clamp(v, 0, 20) }

// Aperture: 0 round, 1 hexagonal.
var shape = 0
//# min=0 max=1 step=1 default=0
export function sliderShape(v) { shape = clamp(floor(v), 0, 1) }

// 0 amber & teal · 1 street (amber, red, white) · 2 rainbow
var palette = 0
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

for (var k = 0; k < MAXD; k++) {
  dx[k] = frac(k * 0.618 + 0.1)
  dy[k] = frac(k * 0.755 + 0.3)
  dd[k] = hash(k * 1.37 + 0.2)                // depth: 1 = near
  da[k] = random(PI2)
  dh[k] = hash(k * 2.71 + 0.6)
  dp[k] = random(PI2)
}

var t = 0

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  var sp = driftPct / 100
  for (var i = 0; i < discs; i++) {
    da[i] += sin(t * 0.13 + dp[i]) * 0.4 * dt  // headings meander
    var s = sp * (1.3 - 0.8 * dd[i])
    var m = sizePct / 100 * 1.6                // wrap margin: a disc leaves fully
    dx[i] = mod(dx[i] + cos(da[i]) * s * dt + m, 1 + 2 * m) - m
    dy[i] = mod(dy[i] + sin(da[i]) * s * dt + m, 1 + 2 * m) - m
  }
}

function hueOf(i) {
  var q = dh[i]
  if (palette == 1) return q < 0.5 ? 0.08 : q < 0.75 ? 0.99 : 0.11
  if (palette == 2) return q
  return q < 0.55 ? 0.07 + q * 0.06 : 0.47 + q * 0.06
}

export function renderBulk() {
  hsv(0.64, 0.7, 0.025)                        // night, not black
  fill()
  var K = shape ? 6 : 8, a0 = shape ? PI / 6 : 0
  var gw = gridWidth(), rmax = 14 / max(gw, 64)   // cost is disc area: cap it on big panels
  for (var i = 0; i < discs; i++) {
    var d = dd[i], r = min(sizePct / 100 * (0.55 + 0.9 * d), rmax)
    var v = (0.8 - 0.35 * d) * (0.8 + 0.2 * sin(t * (0.7 + dh[i]) + dp[i]))
    var h = hueOf(i), s = palette == 1 && dh[i] >= 0.75 && dh[i] < 0.9 ? 0.15 : 0.85
    hsv(h, s, v * 0.6)
    splat(dx[i], dy[i], r, 1)                  // the dim core
    hsv(h, s * 0.85, v * 0.4)
    for (var k = 0; k < K; k++) {              // the brighter rim
      var a = a0 + k * PI2 / K
      splat(dx[i] + cos(a) * r * 0.62, dy[i] + sin(a) * r * 0.62, r * 0.48, 1)
    }
  }
}
