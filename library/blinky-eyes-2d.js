// name: Blinky Eyes 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Blinky Eyes 2D"; original source never consulted.

// A pair of cartoon eyes: thin white elliptical outlines with deep-blue
// ring-shaped irises. The irises dart to a random side and glide back
// every half-to-one second; independently the eyes blink shut (the
// ellipse's vertical semi-axis squashes to a slit) every one-to-two
// seconds. Both eyes are one eye: the x fold mirrors a single ellipse.

const EYE_CX = 0.25     // eye centers at x = 0.25 / 0.75 (after fold)
const EYE_W = 0.14      // ellipse half-width
const EYE_H = 0.10      // ellipse half-height, fully open
const IRIS_R = 0.055    // iris radius
const OUT_TH = 0.05     // outline-thickness threshold
const GAZE_EXC = 0.06   // max iris excursion (~ quarter eye-width)
const MOVE_T = 0.35     // gaze glide duration, s
const BLINK_T = 0.5     // blink duration, s

// gaze state machine: idle (centered) <-> moving (sine hump excursion)
var gazeT = 0
var gazeMoving = 0
var gazeWait = 0.7
var gazeDir = 1
var gazeOff = 0

// blink state machine: open <-> blinking (animated half-height)
var blinkT = 0
var blinking = 0
var openWait = 1.5
var halfH = EYE_H

export function beforeRender(delta) {
  var dt = delta / 1000

  gazeT += dt
  if (!gazeMoving) {
    gazeOff = 0
    if (gazeT > gazeWait) {
      gazeMoving = 1
      gazeT = 0
      gazeDir = random(1) < 0.5 ? -1 : 1   // coin-flip direction
    }
  } else {
    var p = gazeT / MOVE_T
    if (p >= 1) {
      gazeMoving = 0
      gazeT = 0
      gazeOff = 0
      gazeWait = 0.5 + random(0.5)         // next idle gap: 0.5..1 s
    } else {
      gazeOff = gazeDir * GAZE_EXC * sin(PI * p)  // out and back, smoothly
    }
  }

  blinkT += dt
  if (!blinking) {
    halfH = EYE_H
    if (blinkT > openWait) {
      blinking = 1
      blinkT = 0
    }
  } else {
    var q = blinkT / BLINK_T
    if (q >= 1) {
      blinking = 0
      blinkT = 0
      halfH = EYE_H
      openWait = 1 + random(1)             // next open stretch: 1..2 s
    } else {
      // cosine envelope: shuts fast, holds a slit, reopens
      halfH = clamp(EYE_H * (0.5 + 0.5 * cos(PI2 * q)), 0.015, EYE_H)
    }
  }
}

export function render2D(index, x, y) {
  var px = x - 0.5
  var py = y - 0.5
  var sgn = px < 0 ? -1 : 1
  var ex = abs(px) - EYE_CX          // fold: both eyes share one ellipse

  // ellipse metric: 1 on the boundary, <1 inside; blink squashes halfH
  var m = hypot(ex / EYE_W, py / halfH)
  // iris distance; un-mirror the gaze so both irises look the same way
  var di = hypot(ex - gazeOff * sgn, py)

  if (!blinking && di < IRIS_R) {
    // deep blue iris, dark center brightening quadratically to the rim
    var q = di / IRIS_R
    hsv(0.65, 1, 0.1 + 0.9 * q * q)
  } else if (m < 1) {
    // steep power law: only pixels hugging the boundary glow white
    var b = pow(max(m - OUT_TH, OUT_TH), 6)
    hsv(0, 0, b)
  } else {
    rgb(0, 0, 0)
  }
}
