// name: Easing Library v1.01
// Clean-room reimplementation from a prose functional description of the
// community pattern "Easing Library v1.01"; original source never consulted.

// A library of the thirty standard easings.net curves (in/out/inOut of ten
// families), plus a bundled demo: each curve gets a ~5 second dwell while a
// progress value ping-pongs 0 -> 1 -> 0. 1D shows a white dot moving with the
// curve's character; 2D plots the curve as a rainbow graph with a white
// output marker and a faint gray identity diagonal.

// ---------- the library ----------

function easeInSine(t) { return 1 - cos(t * PI / 2) }
function easeOutSine(t) { return sin(t * PI / 2) }
function easeInOutSine(t) { return (1 - cos(PI * t)) / 2 }

function easeInQuadratic(t) { return t * t }
function easeOutQuadratic(t) { return 1 - (1 - t) * (1 - t) }
function easeInOutQuadratic(t) {
  if (t < 0.5) return 2 * t * t
  var u = -2 * t + 2
  return 1 - u * u / 2
}

function easeInCubic(t) { return t * t * t }
function easeOutCubic(t) { var u = 1 - t; return 1 - u * u * u }
function easeInOutCubic(t) {
  if (t < 0.5) return 4 * t * t * t
  var u = -2 * t + 2
  return 1 - u * u * u / 2
}

function easeInQuart(t) { return t * t * t * t }
function easeOutQuart(t) { var u = 1 - t; return 1 - u * u * u * u }
function easeInOutQuart(t) {
  if (t < 0.5) return 8 * t * t * t * t
  var u = -2 * t + 2
  return 1 - u * u * u * u / 2
}

function easeInQuint(t) { return t * t * t * t * t }
function easeOutQuint(t) { var u = 1 - t; return 1 - u * u * u * u * u }
function easeInOutQuint(t) {
  if (t < 0.5) return 16 * t * t * t * t * t
  var u = -2 * t + 2
  return 1 - u * u * u * u * u / 2
}

function easeInExpo(t) {
  if (t == 0) return 0
  return pow(2, 10 * t - 10)
}
function easeOutExpo(t) {
  if (t == 1) return 1
  return 1 - pow(2, -10 * t)
}
function easeInOutExpo(t) {
  if (t == 0) return 0
  if (t == 1) return 1
  if (t < 0.5) return pow(2, 20 * t - 10) / 2
  return (2 - pow(2, -20 * t + 10)) / 2
}

function easeInCirc(t) { return 1 - sqrt(1 - t * t) }
function easeOutCirc(t) { var u = t - 1; return sqrt(1 - u * u) }
function easeInOutCirc(t) {
  if (t < 0.5) return (1 - sqrt(1 - 4 * t * t)) / 2
  var u = -2 * t + 2
  return (sqrt(1 - u * u) + 1) / 2
}

// back: deliberately overshoots slightly past both ends
function easeInBack(t) {
  var c1 = 1.70158, c3 = c1 + 1
  return c3 * t * t * t - c1 * t * t
}
function easeOutBack(t) {
  var c1 = 1.70158, c3 = c1 + 1
  var u = t - 1
  return 1 + c3 * u * u * u + c1 * u * u
}
function easeInOutBack(t) {
  var c2 = 1.70158 * 1.525
  if (t < 0.5) {
    var u = 2 * t
    return u * u * ((c2 + 1) * u - c2) / 2
  }
  var v = 2 * t - 2
  return (v * v * ((c2 + 1) * v + c2) + 2) / 2
}

// elastic: deliberately oscillates past both ends
function easeInElastic(t) {
  if (t == 0) return 0
  if (t == 1) return 1
  var c4 = PI2 / 3
  return -pow(2, 10 * t - 10) * sin((t * 10 - 10.75) * c4)
}
function easeOutElastic(t) {
  if (t == 0) return 0
  if (t == 1) return 1
  var c4 = PI2 / 3
  return pow(2, -10 * t) * sin((t * 10 - 0.75) * c4) + 1
}
function easeInOutElastic(t) {
  if (t == 0) return 0
  if (t == 1) return 1
  var c5 = PI2 / 4.5
  if (t < 0.5) return -pow(2, 20 * t - 10) * sin((20 * t - 11.125) * c5) / 2
  return pow(2, -20 * t + 10) * sin((20 * t - 11.125) * c5) / 2 + 1
}

// bounce: piecewise parabolic, like a ball settling
function easeOutBounce(t) {
  var n1 = 7.5625, d1 = 2.75
  if (t < 1 / d1) return n1 * t * t
  if (t < 2 / d1) { t -= 1.5 / d1; return n1 * t * t + 0.75 }
  if (t < 2.5 / d1) { t -= 2.25 / d1; return n1 * t * t + 0.9375 }
  t -= 2.625 / d1
  return n1 * t * t + 0.984375
}
function easeInBounce(t) { return 1 - easeOutBounce(1 - t) }
function easeInOutBounce(t) {
  if (t < 0.5) return (1 - easeOutBounce(1 - 2 * t)) / 2
  return (1 + easeOutBounce(2 * t - 1)) / 2
}

// ---------- demo plumbing ----------

var NUM_CURVES = 30

function evalCurve(i, t) {
  if (i == 0) return easeInSine(t)
  if (i == 1) return easeOutSine(t)
  if (i == 2) return easeInOutSine(t)
  if (i == 3) return easeInQuadratic(t)
  if (i == 4) return easeOutQuadratic(t)
  if (i == 5) return easeInOutQuadratic(t)
  if (i == 6) return easeInCubic(t)
  if (i == 7) return easeOutCubic(t)
  if (i == 8) return easeInOutCubic(t)
  if (i == 9) return easeInQuart(t)
  if (i == 10) return easeOutQuart(t)
  if (i == 11) return easeInOutQuart(t)
  if (i == 12) return easeInQuint(t)
  if (i == 13) return easeOutQuint(t)
  if (i == 14) return easeInOutQuint(t)
  if (i == 15) return easeInExpo(t)
  if (i == 16) return easeOutExpo(t)
  if (i == 17) return easeInOutExpo(t)
  if (i == 18) return easeInCirc(t)
  if (i == 19) return easeOutCirc(t)
  if (i == 20) return easeInOutCirc(t)
  // back family: scaled/offset in the demo so the overshoot stays on-screen
  if (i == 21) return easeInBack(t) * 0.7 + 0.15
  if (i == 22) return easeOutBack(t) * 0.7 + 0.15
  if (i == 23) return easeInOutBack(t) * 0.7 + 0.15
  if (i == 24) return easeInElastic(t)
  if (i == 25) return easeOutElastic(t)
  if (i == 26) return easeInOutElastic(t)
  if (i == 27) return easeInBounce(t)
  if (i == 28) return easeOutBounce(t)
  return easeInOutBounce(t)
}

var DWELL = 5 // seconds per curve
var elapsed = 0
var curveIndex = 0
var progress = 0 // ping-pong 0..1..0 within each dwell
var eased = 0
export var easedMin = 1
export var easedMax = 0

export function beforeRender(delta) {
  elapsed += delta / 1000
  if (elapsed >= DWELL * NUM_CURVES) {
    elapsed -= DWELL * NUM_CURVES
    easedMin = 1
    easedMax = 0
  }
  curveIndex = floor(elapsed / DWELL)
  var p = (elapsed - curveIndex * DWELL) / DWELL
  progress = p < 0.5 ? p * 2 : 2 - p * 2
  eased = evalCurve(curveIndex, progress)
  if (eased < easedMin) easedMin = eased
  if (eased > easedMax) easedMax = eased
}

export function render(index) {
  // single white dot at the eased position
  if (abs(index / pixelCount - eased) < 1 / pixelCount) {
    rgb(1, 1, 1)
  } else {
    rgb(0, 0, 0)
  }
}

export function render2D(index, x, y) {
  var tol = 1 / sqrt(pixelCount) // roughly one pixel row/column

  // faithful leftover from the original: the 1D dot rule applied by index
  if (abs(index / pixelCount - eased) < 1 / pixelCount) {
    rgb(1, 1, 1)
    return
  }

  // white output marker near mid-height, tracking the eased value horizontally
  if (abs(y - 0.5) < tol && abs(x - eased) < tol / 2) {
    rgb(1, 1, 1)
    return
  }

  // the curve itself: y (flipped so up = larger) vs curve(x), rainbow by value
  var v = evalCurve(curveIndex, x)
  if (abs((1 - y) - v) < tol) {
    hsv(v, 1, 1)
    return
  }

  // faint gray identity diagonal for reference
  if (abs((1 - y) - x) < tol / 2) {
    rgb(0.1, 0.1, 0.1)
    return
  }

  rgb(0, 0, 0)
}
