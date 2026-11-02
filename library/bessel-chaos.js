// name: Bessel Chaos
// tags: both, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/bessel-chaos.md — reimagined, not a replica.
// Deterministic "chaos" with no randomness: cubic and quartic chirps inside two
// sines around a wandering focal point — broad breathing blobs at the focus,
// ever finer ripples away from it — driven by slow incommensurate oscillators.
// The chirp is capped at a quarter cycle per pixel, so a short strip shows a fine
// shimmer rather than aliased noise. On a panel the chirp runs along the radius:
// rings that squeeze and swell about a focal circle, read from a per-frame table.

var phase = 0, hueDiv = 6, pan = 0
var baseHue = 0.5, baseSat = 1

// Pan shaping: 1 = smooth sweep; higher = dwell at the ends, snap across the middle.
var panShape = 9
//# min=1 max=15 step=0.1 default=9
export function sliderTransitionSpeed(v) { panShape = clamp(v, 1, 15) }

// Centre colour of the hue spread (the picker's brightness is ignored).
export function hsvPickerColor(h, s, v) { baseHue = h; baseSat = s }

var MAXF = 0.25 * PI2               // radians per pixel at the cap
var d1 = 1, d2 = 1                  // where each chirp hits the cap

function setCaps(ppd) {
  d1 = pow(MAXF * ppd / 40, 1 / 3)  // quartic: slope 40 d^3
  d2 = sqrt(MAXF * ppd / 30)        // cubic:   slope 30 d^2
}

var hOut = 0, vOut = 0
function chirp(d) {
  var a = abs(d), q, c
  if (a < d1) { q = a * a; q = 10 * q * q }
  else q = 10 * d1 * d1 * d1 * (d1 + 4 * (a - d1))
  if (a < d2) c = 10 * a * a * a
  else c = 10 * d2 * d2 * (d2 + 3 * (a - d2))
  if (d < 0) c = -c
  var w1 = sin(q + 2 * phase)       // quartic chirp, co-drifting
  var w2 = sin(c - phase)           // cubic chirp, counter-drifting
  var v = (w1 + w2) / 2
  hOut = baseHue + d * 0.07 + (w2 - 0.8) / hueDiv   // tint drifts across the focus
  v = min(v * 1.3, 1)
  vOut = v > 0 ? v * v : 0
}

var NB = 512                        // radial table, indexed by radius squared
var tH = array(NB), tV = array(NB)
var R2MAX = 0.5, focus = 0.3

export function beforeRender(delta) {
  phase = sin(time(0.183) * PI2) * PI2 * 2      // ~12 s band drift
  hueDiv = 2.5 + 6 * wave(time(0.35))           // colour spread breathes, ~23 s
  var s = sin(time(0.227) * PI2)                // ~15 s focal pan
  pan = sign(s) * pow(abs(s), 1 / panShape) * 0.5

  if (has2DMap()) {
    // the focal circle swings between the centre and the corners
    focus = 0.3 + pan * 0.44
    var gw = max(gridWidth(), 16)
    setCaps(gw / 4)                 // d = 4 * (r - focus), r = 0.5 at the edge
    for (var k = 0; k < NB; k++) {
      chirp((sqrt(k / (NB - 1) * R2MAX) - focus) * 4)
      tH[k] = hOut
      tV[k] = vOut
    }
  } else setCaps(pixelCount / 3)
}

export function render(index) {
  chirp((index / pixelCount - 0.5 - pan) * 3)
  hsv(hOut, baseSat, vOut)
}

export function render2D(index, x, y) {
  x -= 0.5
  y -= 0.5
  var k = floor((x * x + y * y) * ((NB - 1) / R2MAX))
  hsv(tH[k], baseSat, tV[k])
}
