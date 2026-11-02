// name: Polar mapping helper 2D / 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Polar mapping helper 2D / 3D"; original source never
// consulted.

// Diagnostic utility for maps built in polar/spherical coordinates,
// normalized to the unit range: x = radius from origin, y = azimuth
// (0..1 = one full turn), z = polar angle from the top (0.5 = equator).
// Cycles through four test modes (~6 s each): radar sweep, axes + expanding
// shell, index rainbow, octants. Overlaid on every mode: a white pixel
// chasing through index order once per mode period, an RGB identification
// blink on pixel 0, and a red triangle pulse on the last pixel.
// Fallbacks: 2D pins the polar angle to the equator; 1D uses radius =
// index fraction, angle zero — so it runs on any layout.

var SECONDS_PER_MODE = 6
var WIDTH = 0.125   // default proximity half-width; enlarge for low pixel counts

var mode = 0        // current mode number
var modeP = 0       // 0..1 progress through the current mode
var chaseIdx = 0
var blinkT = 0

// linear nearness of two scalars, zero beyond half-width w (default WIDTH)
function near(a, b, w) {
  if (w == 0) w = WIDTH
  return max(0, 1 - abs(a - b) / w)
}

// wrap-aware angular nearness (angles in turns): a triangle wave of the
// difference gives the wrapped distance, so nearness works across 1 -> 0
function nearAngle(a, b, w) {
  if (w == 0) w = WIDTH
  var d = triangle(a - b) * 0.5   // wrapped distance, 0..0.5 turn
  return max(0, 1 - d / w)
}

// mode 1: red radar beam sweeping once around per mode period
function modeRadar(index, r, az, po) {
  var n = nearAngle(az, modeP, 0.08)
  hsv(0, 1, n * n)
}

// mode 2: RGB axis cones + an expanding shell tinted by axis proximity
function modeAxes(index, r, az, po) {
  var sh = near(r, modeP, 0)
  sh = sh * sh
  if (sh > 0.1) {
    var w = WIDTH * 4   // much wider tolerance for the shell's tint sectors
    rgb(sh * nearAngle(az, 0, w), sh * nearAngle(az, 0.25, w), sh * near(po, 0, w))
  } else {
    var eq = near(po, 0.5, 0)
    var ra = nearAngle(az, 0, 0) * eq      // +x: azimuth 0, equatorial
    var ga = nearAngle(az, 0.25, 0) * eq   // +y: quarter turn, equatorial
    var ba = near(po, 0, 0)                // +z: near the top pole
    rgb(ra * ra, ga * ga, ba * ba)
  }
}

// mode 3: mapper-style rainbow by index with a bright "you are here" window
function modeRainbow(index, r, az, po) {
  var p = index / pixelCount
  var n = near(p, modeP, 0)
  hsv(p + modeP, 1, max(0.15, n * n))
}

// mode 4: octants — brightness capped low (one octant is fully white)
function modeOctants(index, r, az, po) {
  rgb(az > 0.5 ? 0.25 : 0,
      az > 0.25 && az < 0.75 ? 0.25 : 0,
      po > 0.5 ? 0.25 : 0)
}

var modeFns = array(4)
modeFns[0] = modeRadar
modeFns[1] = modeAxes
modeFns[2] = modeRainbow
modeFns[3] = modeOctants

export function beforeRender(delta) {
  var cyc = time(4 * SECONDS_PER_MODE / 65.536)
  mode = min(floor(cyc * 4), 3)
  // mode = 1   // uncomment to freeze the cycle on one mode
  modeP = frac(cyc * 4)
  chaseIdx = floor(modeP * pixelCount)
  blinkT = time(2 / 65.536)   // independent ~2 s identification clock
}

export function render3D(index, x, y, z) {
  modeFns[mode](index, x, y, z)

  // white chase through index order, one pass per mode period
  // (faded-tail variant: v = near(index / pixelCount, modeP, 0.05); if (v > 0) rgb(v, v, v))
  if (index == chaseIdx) rgb(1, 1, 1)

  // pixel 0: white, then separated R/G/B flashes, then dark — spots
  // channel-order misconfiguration at a glance
  if (index == 0) {
    if (blinkT < 0.35) rgb(1, 1, 1)
    else if (blinkT >= 0.45 && blinkT < 0.52) rgb(1, 0, 0)
    else if (blinkT >= 0.6 && blinkT < 0.67) rgb(0, 1, 0)
    else if (blinkT >= 0.75 && blinkT < 0.82) rgb(0, 0, 1)
    else rgb(0, 0, 0)
  }

  // last pixel: red triangle pulse, several cycles per blink period
  if (index == pixelCount - 1) rgb(triangle(blinkT * 4), 0, 0)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0.5)   // pin the polar angle to the equator
}

export function render(index) {
  render2D(index, index / pixelCount, 0)
}
