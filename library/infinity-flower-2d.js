// name: Infinity Flower 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Infinity Flower 2D"; original source never consulted.

// An endlessly regenerating flower: glowing center disc plus golden-angle
// (phyllotaxis) petals. Every couple of seconds the petals spin, the center
// fades out, and a freshly randomized "species" appears.

var GOLDEN_ANGLE = 2.39996           // radians between successive petals
var GOLDEN_CONJ = 0.61803            // golden-ratio conjugate (hue offset)
var MAX_PETALS = 12

var HOLD_MS = 1600                   // steady display time per species
var SPIN_MS = 1000                   // spin/fade transition time
var SPIN_RATE = 0.045                // rad per ms (time-scaled spin)

var petalAngle = array(MAX_PETALS)
var petalHue = array(MAX_PETALS)

var petalCount = 5
var petalLen = 0.25
var widthFactor = 1.5
var centerHue = 0
var centerR = 0.05
var colorVariant = 0
var centerFade = 1

// State machine: 0 = steady hold, 1 = spin/fade transition
var mode = 0
var timer = 0
var spinDir = 1

function makeSpecies() {
  petalCount = 2 + floor(random(10.99))            // 2..12 petals
  petalLen = 0.5 * (1 / 3 + random(1 / 3))         // 1/3..2/3 of half-display
  widthFactor = 1 + random(1)                      // 1x..2x around medium
  var baseHue = random(1)
  centerHue = baseHue + GOLDEN_CONJ                // contrasting center color
  centerR = random(1) < 0.3 ? 0.062 : 0.048        // biased coin flip on size
  colorVariant = random(1) < 0.5                   // radius-graded petals?
  var altTint = random(0.09)                       // every-other-petal tint

  // Phyllotaxis: each petal a golden angle past the previous.
  var a = random(PI2)
  for (var i = 0; i < petalCount; i++) {
    petalAngle[i] = mod(a, PI2)
    petalHue[i] = baseHue + (i % 2 ? altTint : 0)
    a += GOLDEN_ANGLE
  }
}

makeSpecies()

export function beforeRender(delta) {
  timer += delta
  if (mode == 0) {
    // steady hold
    if (timer > HOLD_MS) {
      mode = 1
      timer = 0
      spinDir = random(1) < 0.5 ? -1 : 1   // fair coin flip on direction
    }
  } else {
    // transition: spin the petals, fade the center
    var step = spinDir * SPIN_RATE * delta
    for (var i = 0; i < petalCount; i++) {
      petalAngle[i] = mod(petalAngle[i] + step, PI2)
    }
    centerFade = max(0, 1 - timer / SPIN_MS)
    if (timer > SPIN_MS) {
      makeSpecies()
      centerFade = 1
      mode = 0
      timer = 0
    }
  }
}

export function render2D(index, x, y) {
  var px = x - 0.5
  var py = y - 0.5
  var r = hypot(px, py)

  if (r > petalLen) {
    rgb(0, 0, 0)
    return
  }

  if (r < centerR) {
    // center disc: bright core, soft edge, scaled by the transition fade
    var u = r / centerR
    hsv(centerHue, 1, centerFade * (1 - u * u))
    return
  }

  var ang = atan2(py, px)
  if (ang < 0) ang += PI2

  // fractional distance along the petal, 0 at disc edge .. 1 at tip
  var rr = (r - centerR) / (petalLen - centerR)
  // smooth hump peaking mid-petal: leaf-like pointed-tip outline
  var hump = sin(rr * PI)
  // base angular tolerance shrinks with radius (constant drawn width)
  var w = 0.026 / r * widthFactor * hump

  for (var i = 0; i < petalCount; i++) {
    var da = ang - petalAngle[i]
    da = abs(mod(da + PI, PI2) - PI)   // wrapped angular difference
    if (da < w) {
      // centerline dim, edges brighter: outline-ish petals
      var b = clamp(da / w, 0.12, 1)
      var h = petalHue[i]
      if (colorVariant) h += rr * 0.22   // gradient along petal length
      hsv(h, 1 - 0.3 * b, b)
      return
    }
  }
  rgb(0, 0, 0)
}
