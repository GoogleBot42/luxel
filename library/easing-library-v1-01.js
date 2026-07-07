// name: Easing Library v1.01
// Clean-room reimplementation from a prose functional description of the
// community pattern "Easing Library v1.01"; original source never consulted.
//
// A library of the 30 canonical easings.net curves (in/out/inOut of sine,
// quad, cubic, quart, quint, expo, circ, back, elastic, bounce), plus the
// bundled demo that cycles through them: 1D shows a white dot at the eased
// position; 2D plots the curve in rainbow with a white output marker and a
// faint gray identity diagonal.

// ---- library ----------------------------------------------------------

function bounceOut(t) {
  var n1 = 7.5625
  var d1 = 2.75
  if (t < 1 / d1) return n1 * t * t
  if (t < 2 / d1) { t = t - 1.5 / d1; return n1 * t * t + 0.75 }
  if (t < 2.5 / d1) { t = t - 2.25 / d1; return n1 * t * t + 0.9375 }
  t = t - 2.625 / d1
  return n1 * t * t + 0.984375
}

// ease-in of family f (0 sine, 1 quad, 2 cubic, 3 quart, 4 quint,
// 5 expo, 6 circ, 7 back, 8 elastic, 9 bounce)
function easeInF(f, t) {
  if (f == 0) return 1 - cos(t * PI / 2)
  if (f == 1) return t * t
  if (f == 2) return t * t * t
  if (f == 3) return t * t * t * t
  if (f == 4) return t * t * t * t * t
  if (f == 5) {                          // expo: exact endpoints special-cased
    if (t <= 0) return 0
    if (t >= 1) return 1
    return pow(2, 10 * t - 10)
  }
  if (f == 6) return 1 - sqrt(1 - t * t) // circ
  if (f == 7) return 2.70158 * t * t * t - 1.70158 * t * t  // back (overshoots)
  if (f == 8) {                          // elastic (oscillates past the ends)
    if (t <= 0) return 0
    if (t >= 1) return 1
    return -pow(2, 10 * t - 10) * sin((t * 10 - 10.75) * PI2 / 3)
  }
  return 1 - bounceOut(1 - t)            // bounce
}

// curve i (0..29) at t: i = family*3 + variant (0 in, 1 out, 2 inOut).
// out/inOut derive from ease-in by reflection, which reproduces the
// canonical formulations exactly — except inOutBack and inOutElastic,
// which use their own canonical constants and are special-cased.
function easeAt(i, t) {
  var f = floor(i / 3)
  var v = i - f * 3
  if (v == 2 && f == 7) {                // inOutBack, c2 = 1.70158 * 1.525
    if (t < 0.5) {
      var u = 2 * t
      return u * u * (3.5949095 * u - 2.5949095) / 2
    }
    var w = 2 * t - 2
    return (w * w * (3.5949095 * w + 2.5949095) + 2) / 2
  }
  if (v == 2 && f == 8) {                // inOutElastic, c5 = 2*PI/4.5
    if (t <= 0) return 0
    if (t >= 1) return 1
    var c5 = PI2 / 4.5
    if (t < 0.5) return -(pow(2, 20 * t - 10) * sin((20 * t - 11.125) * c5)) / 2
    return pow(2, -20 * t + 10) * sin((20 * t - 11.125) * c5) / 2 + 1
  }
  if (v == 0) return easeInF(f, t)
  if (v == 1) return 1 - easeInF(f, 1 - t)
  if (t < 0.5) return easeInF(f, 2 * t) / 2
  return 1 - easeInF(f, 2 - 2 * t) / 2
}

// ---- demo -------------------------------------------------------------

var DWELL = 5000                  // ms per curve
var elapsed = 0
var curveIndex = 0
var progress = 0
var eased = 0
var tol2 = 0.0625

// demo evaluation: the back family is scaled/offset to stay on-screen
// (elastic is shown raw, off-screen excursions just vanish)
function demoEase(i, t) {
  var v = easeAt(i, t)
  if (floor(i / 3) == 7) v = v * 0.8 + 0.1
  return v
}

export function beforeRender(delta) {
  elapsed += delta
  while (elapsed >= DWELL) {
    elapsed -= DWELL
    curveIndex = (curveIndex + 1) % 30
  }
  // ping-pong progress: 0 -> 1 over the first half of the dwell, back down
  progress = triangle(elapsed / DWELL)
  eased = demoEase(curveIndex, progress)
  tol2 = 1 / sqrt(pixelCount)     // ~one pixel-row on a square matrix
}

export function render(index) {
  if (abs(index / pixelCount - eased) < 1 / pixelCount) {
    rgb(1, 1, 1)                  // white dot sweeping with the curve's character
  } else {
    rgb(0, 0, 0)
  }
}

export function render2D(index, x, y) {
  // 1D-style dot by pixel index (faithful leftover from the original)
  if (abs(index / pixelCount - eased) < 1 / pixelCount) {
    rgb(1, 1, 1)
    return
  }
  // the curve itself, hue = eased value (rainbow sweep along the plot)
  var fy = demoEase(curveIndex, x)
  if (abs(1 - y - fy) < tol2) {
    hsv(fy, 1, 1)
    return
  }
  // white output marker near mid-height tracking the eased position
  if (abs(y - 0.5) < tol2 / 2 && abs(x - eased) < tol2) {
    rgb(1, 1, 1)
    return
  }
  // faint gray identity diagonal for reference
  if (abs(x - y) < tol2 / 2) {
    rgb(0.2, 0.2, 0.2)
    return
  }
  rgb(0, 0, 0)
}
