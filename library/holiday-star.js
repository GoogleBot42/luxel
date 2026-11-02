// name: Holiday Star
// tags: panel, holiday
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/carrie-s-holiday-star-2d.md,
// blue-holiday-star-2d.md — reimagined, not a replica.
// An eight-pointed holiday star filling most of the panel: four long axis rays
// and four shorter diagonals that twinkle (noise swells their reach), a white-hot
// core in a soft halo, a slow spin, and a faint field of twinkling stars behind.
// Rays are hyperbolic |u*v| falloffs in coordinates the engine's rotate()
// transform already turned (the diagonals use u*u - v*v), so a pixel costs a
// handful of multiplies — no pow, no atan2.

var sizePct = 70        // ray reach, percent of the panel
//# min=20 max=100 step=1 default=70
export function sliderSizePercent(v) { sizePct = clamp(v, 20, 100) }

var twinkleSec = 2      // seconds per twinkle swell
//# min=0.5 max=8 step=0.1 default=2
export function sliderTwinkleSeconds(v) { twinkleSec = clamp(v, 0.5, 8) }

// Star colour: hue and saturation (the picker's brightness is ignored).
var hue0 = 0.55, sat0 = 0.85
export function hsvPickerColor(h, s, v) { hue0 = h; sat0 = s }

var drift = 0           // 1 = the hue walks the wheel, once every ~4 minutes
//# default=0
export function toggleHueDrift(on) { drift = on > 0 ? 1 : 0 }

var skyPct = 3          // background stars, percent of the pixels
//# min=0 max=10 step=0.5 default=3
export function sliderBackgroundStars(v) { skyPct = clamp(v, 0, 10) }

var tw = 0, sky = 0, ang = 0, hueOff = 0
var RR = 0.12, RR2 = 0.04, L1 = 8, K1 = 200, L2 = 20, K2 = 120, G = 0.003, HA = 0.003, HB = 0.02, hue = 0.55, dens = 0.03

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  tw = mod(tw + dt / twinkleSec, 1000)
  sky = mod(sky + dt * 0.35, 1000)
  ang = mod(ang + dt * PI2 / 60, PI2)            // one turn a minute
  if (drift) hueOff = mod(hueOff + dt / 240, 1)
  hue = hue0 + hueOff
  dens = skyPct / 100

  var n = simplex2(tw, 4.3), n2 = simplex2(tw * 1.6, 9.1)
  var r1 = sizePct / 200 * (1 + 0.12 * n)     // axis ray tip radius
  var r2 = r1 * (0.58 + 0.1 * n2)                // diagonal ray tip radius
  RR = r1 * r1
  L1 = 1 / RR
  RR2 = r2 * r2
  L2 = 1 / RR2
  // ray thickness scales with the pixel pitch so the rays stay ~2 px wide
  var gw = gridWidth() > 0 ? gridWidth() : 32, gh = gridHeight() > 0 ? gridHeight() : gw
  var px = 1 / max(min(gw, gh), 16)
  K1 = 2 / (px * r1) * (1 - 0.15 * n)
  K2 = 2.2 / (px * r2)
  var c = 0.03 * (1 + 0.25 * n)
  G = c * c
  HB = 0.03 * sizePct / 70
  HA = HB * (0.24 + 0.06 * n2)

  resetTransform()
  translate(-0.5, -0.5)
  scale(max(gw / gh, 1), max(gh / gw, 1))         // square units on any panel
  rotate(ang)
}

export function render2D(index, x, y) {
  var uu = x * x, vv = y * y, d2 = uu + vv
  var val = G / (d2 + G) + HA / (d2 + HB)
  if (d2 < RR) {
    var a = 1 - abs(x * y) * K1
    if (a > 0) val += a * (1 - d2 * L1) * 1.3
    if (d2 < RR2) {
      var b = 1 - abs(uu - vv) * K2
      if (b > 0) val += b * (1 - d2 * L2) * 0.9
    }
  } else if (dens > 0) {
    var hh = hash(index)
    if (hh < dens) {
      var ph = hh / dens
      var t = triangle(sky * (0.6 + ph) + ph * 7)
      t = t * t
      hsv(hue + 0.08, 0.3, val + t * t * 0.45)
      return
    }
  }
  hsv(hue, sat0 * clamp(1.3 - val, 0, 1), min(val, 1))
}
