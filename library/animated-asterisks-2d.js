// name: Animated Asterisks 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Animated Asterisks 2D"; original source never consulted.

// N full-span segments through the center, evenly fanned over a half-turn,
// rotating together. Each arm gets its own hue from an evenly spaced,
// drifting rainbow; arms fade linearly to black at their edges.
//
// Every control carries a //# directive, so the UI sends REAL units (arms,
// panel widths, revolutions per second) and the handlers use them directly
// rather than rescaling a 0..1 knob.

var MAX_LINES = 24
var lineCos = array(MAX_LINES)
var lineSin = array(MAX_LINES)
var lineHue = array(MAX_LINES)

var HALF_LEN = 0.75   // half segment length: spans the whole unit square

// Whole arms, in arms.
var numLines = 6
//# min=1 max=24 step=1 default=6
export function sliderNumberOfLines(v) {
  numLines = clamp(floor(v), 1, MAX_LINES)
}

// Arm thickness as a fraction of the panel width: 0.02 is roughly a
// one-pixel line on a 64-wide matrix, 0.5 floods it. halfWidth is the
// half-thickness the renderer compares distances against.
var halfWidth = 0.01
//# min=0.01 max=0.5 step=0.005 default=0.02
export function sliderLineWidth(v) {
  halfWidth = max(0.002, v / 2)
}

// Rotation in revolutions per second (time() period = interval * 65.536 s),
// so perceived speed tracks the slider linearly.
var rotInterval = 1 / (65.536 * 0.48)
//# min=0.02 max=3 step=0.01 default=0.48
export function sliderRotationSpeed(v) {
  rotInterval = 1 / (65.536 * max(v, 0.005))
}

// Hue drift in full colour cycles per second, same linear treatment.
var hueInterval = 1 / (65.536 * 0.08)
//# min=0.01 max=1 step=0.01 default=0.08
export function sliderColorSpeed(v) {
  hueInterval = 1 / (65.536 * max(v, 0.005))
}

export function beforeRender(delta) {
  var angle = time(rotInterval) * PI2   // continuous rotation
  var hueBase = time(hueInterval)       // shared hue drift

  for (var i = 0; i < numLines; i++) {
    // Arms spread over a half-turn (each spans the display through center).
    var a = angle + PI * i / numLines
    lineCos[i] = cos(a)
    lineSin[i] = sin(a)
    lineHue[i] = hueBase + i / numLines
  }
}

export function render2D(index, x, y) {
  for (var i = 0; i < numLines; i++) {
    var c = lineCos[i]
    var s = lineSin[i]
    // Segment: (0.5, 0.5) +/- HALF_LEN * (c, s). Work from the center.
    var px = x - 0.5
    var py = y - 0.5

    // Projection of the pixel onto the arm direction.
    var t = px * c + py * s
    var d
    if (t > HALF_LEN) {
      // beyond the far endpoint: distance to that endpoint
      var ex = px - HALF_LEN * c
      var ey = py - HALF_LEN * s
      d = hypot(ex, ey)
    } else if (t < -HALF_LEN) {
      var fx = px + HALF_LEN * c
      var fy = py + HALF_LEN * s
      d = hypot(fx, fy)
    } else {
      // perpendicular distance to the infinite line
      d = abs(px * s - py * c)
    }

    if (d < halfWidth) {
      // first matching arm wins; linear falloff to the edge
      hsv(lineHue[i], 1, 1 - d / halfWidth)
      return
    }
  }
  rgb(0, 0, 0)
}
