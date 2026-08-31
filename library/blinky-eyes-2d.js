// name: Blinky Eyes 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Blinky Eyes 2D"; original source never consulted.

// A pair of cartoon eyes on black: thin white elliptical outlines with
// deep-blue ring-like irises. The irises dart to a random side and glide
// back; independently the ellipses squash shut and reopen (a blink) at
// randomized intervals. Both eyes are one eye: the x fold means the
// ellipse/iris math is written once. Tuned for ~20x10 and larger grids.

// --- controls -------------------------------------------------------------
var eyes = 2            // 1 or 2 eyes
var eyeWidthPct = 49    // eye WIDTH as a percentage of the panel width
var blinkRate = 30      // blinks per minute
var irisH = 0.66        // iris hue
var irisS = 1           // iris saturation

// --- derived geometry (recomputed in beforeRender) ------------------------
// An eye is WIDER THAN TALL: width/height = 1.28, so half-height is
// half-width / 1.28. hw below is the half-width in DISPLAY units; HALF_W is
// the same length expressed in the folded (x-doubled) space the renderer
// works in, so "Eye Width %" means the same on-screen size for 1 or 2 eyes.
var ASPECT = 1.28
var HALF_W = 0.48       // max ellipse half-width, in folded (x-doubled) units
var HALF_H = 0.1875     // max ellipse half-height, display units
var HALF_H_MIN = 0.03
var IRIS_R = 0.11       // iris radius, display units
var GAZE_AMP = 0.09     // max iris excursion, display units
var EDGE = 0.05         // outline-thickness threshold

// Gaze state machine: idle (centered) <-> moving (sine-hump glide out and
// back). Blink state machine: open <-> blinking (wave-envelope squash).
var gazeT = 0, gazeIdle = 0.7, gazeMoving = 0, gazeDir = 1
var MOVE_LEN = 0.35
var blinkT = 0, blinkOpen = 1.5, blinking = 0
var BLINK_LEN = 0.5

var gazeX = 0       // current iris x offset from eye center
var halfH = 0.1875  // current (animated) ellipse half-height

//# min=4 max=60 step=1 default=30
export function sliderBlinkRate(v) { blinkRate = v }

// 50% is the ceiling: two eyes each half the panel wide already meet at the
// midline, which is exactly where the original's do.
//# min=10 max=50 step=1 default=49
export function sliderEyeWidth(v) { eyeWidthPct = clamp(v, 10, 50) }

//# min=1 max=2 step=1 default=2
export function sliderEyes(v) { eyes = floor(v) }

export function hsvPickerIrisColor(h, s, v) { irisH = h; irisS = s }

export function beforeRender(delta) {
  var dt = delta / 1000

  // geometry from the controls
  var hw = eyeWidthPct * 0.005        // half-width in display units
  HALF_W = eyes == 2 ? hw + hw : hw   // ... in folded units
  HALF_H = hw / ASPECT
  HALF_H_MIN = HALF_H * 0.16
  IRIS_R = hw * 0.4583
  GAZE_AMP = hw * 0.375

  // gaze
  gazeT += dt
  if (gazeMoving) {
    if (gazeT >= MOVE_LEN) {
      gazeMoving = 0
      gazeT = 0
      gazeIdle = 0.5 + random(0.5)  // next idle gap: 0.5..1 s
      gazeX = 0
    } else {
      // smooth hump: out to one side and back in one continuous motion
      gazeX = gazeDir * GAZE_AMP * sin(PI * gazeT / MOVE_LEN)
    }
  } else if (gazeT >= gazeIdle) {
    gazeMoving = 1
    gazeT = 0
    gazeDir = random(1) < 0.5 ? -1 : 1  // coin flip
  }

  // blink. One blink every 60/blinkRate seconds on average: the eye stays
  // open for that gap minus the blink itself, jittered +/-33%.
  blinkT += dt
  if (blinking) {
    if (blinkT >= BLINK_LEN) {
      blinking = 0
      blinkT = 0
      var openBase = max(60 / blinkRate - BLINK_LEN, 0.15) * 0.6667
      blinkOpen = openBase + random(openBase)
      halfH = HALF_H
    } else {
      // wave envelope starting on its falling half: closes first, reopens
      var env = wave(0.25 + blinkT / BLINK_LEN)
      halfH = clamp(HALF_H * env, HALF_H_MIN, HALF_H)
    }
  } else {
    halfH = HALF_H
    if (blinkT >= blinkOpen) {
      blinking = 1
      blinkT = 0
    }
  }
}

export function render2D(index, x, y) {
  var px = x - 0.5
  var py = y - 0.5
  if (eyes == 2) {
    // scale x by two and fold each half toward its own eye center,
    // preserving direction so both irises look the same way
    px = px * 2
    if (px < 0) px += 0.5
    else px -= 0.5
  }

  // iris distance: x scaled back down so the iris stays circular
  var dx = eyes == 2 ? (px - gazeX * 2) * 0.5 : px - gazeX
  var d = hypot(dx, py)

  // ellipse metric: 1 exactly on the boundary, <1 inside
  var m = hypot(px / HALF_W, py / halfH)

  if (!blinking && d < IRIS_R) {
    // ring-like iris: dark center brightening quadratically toward the rim
    var r = d / IRIS_R
    hsv(irisH, irisS, r * r)
  } else if (m < 1) {
    // thin soft white outline: steep power law keeps only the boundary
    var b = max(m - EDGE, EDGE)
    var v = b * b * b
    rgb(v * v, v * v, v * v)  // ^6
  } else {
    rgb(0, 0, 0)
  }
}
