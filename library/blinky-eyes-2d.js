// name: Blinky Eyes 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Blinky Eyes 2D"; original source never consulted.

// A pair of cartoon eyes on black: thin white elliptical outlines with
// deep-blue ring-like irises. The irises dart to a random side and glide
// back; independently the ellipses squash shut and reopen (a blink) at
// randomized intervals. Both eyes are one eye: the x fold means the
// ellipse/iris math is written once. Tuned for ~20x10 and larger grids.

var EYES = 2         // 1 or 2 (source-level constant)
var HALF_W = 0.36    // max ellipse half-width, in folded (x-doubled) units
var HALF_H = 0.25    // max ellipse half-height
var HALF_H_MIN = 0.03
var IRIS_R = 0.11    // iris radius, display units
var GAZE_AMP = 0.09  // max iris excursion, display units
var EDGE = 0.05      // outline-thickness threshold

// Gaze state machine: idle (centered) <-> moving (sine-hump glide out and
// back). Blink state machine: open <-> blinking (wave-envelope squash).
var gazeT = 0, gazeIdle = 0.7, gazeMoving = 0, gazeDir = 1
var MOVE_LEN = 0.35
var blinkT = 0, blinkOpen = 1.5, blinking = 0
var BLINK_LEN = 0.5

var gazeX = 0    // current iris x offset from eye center
var halfH = 0.25 // current (animated) ellipse half-height

export function beforeRender(delta) {
  var dt = delta / 1000

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

  // blink
  blinkT += dt
  if (blinking) {
    if (blinkT >= BLINK_LEN) {
      blinking = 0
      blinkT = 0
      blinkOpen = 1 + random(1)  // next open interval: 1..2 s
      halfH = HALF_H
    } else {
      // wave envelope starting on its falling half: closes first, reopens
      var env = wave(0.25 + blinkT / BLINK_LEN)
      halfH = clamp(HALF_H * env, HALF_H_MIN, HALF_H)
    }
  } else if (blinkT >= blinkOpen) {
    blinking = 1
    blinkT = 0
  }
}

export function render2D(index, x, y) {
  var px = x - 0.5
  var py = y - 0.5
  if (EYES == 2) {
    // scale x by two and fold each half toward its own eye center,
    // preserving direction so both irises look the same way
    px = px * 2
    if (px < 0) px += 0.5
    else px -= 0.5
  }

  // iris distance: x scaled back down so the iris stays circular
  var dx = EYES == 2 ? (px - gazeX * 2) * 0.5 : px - gazeX
  var d = hypot(dx, py)

  // ellipse metric: 1 exactly on the boundary, <1 inside
  var m = hypot(px / HALF_W, py / halfH)

  if (!blinking && d < IRIS_R) {
    // ring-like iris: deep blue, dark center brightening quadratically
    var r = d / IRIS_R
    hsv(0.66, 1, r * r)
  } else if (m < 1) {
    // thin soft white outline: steep power law keeps only the boundary
    var b = max(m - EDGE, EDGE)
    var v = b * b * b
    rgb(v * v, v * v, v * v)  // ^6
  } else {
    rgb(0, 0, 0)
  }
}
