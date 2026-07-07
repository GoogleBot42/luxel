// name: Animated Asterisks 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Animated Asterisks 2D"; original source never consulted.

// N full-span segments through the center, evenly fanned over a half-turn,
// rotating together. Each arm gets its own hue from an evenly spaced,
// drifting rainbow; arms fade linearly to black at their edges. Arm width
// optionally "breathes" on a slow triangle wave.

var MAX_LINES = 24
var lineCos = array(MAX_LINES)
var lineSin = array(MAX_LINES)
var lineHue = array(MAX_LINES)

var HALF_LEN = 0.75   // half segment length: spans the whole unit square

var numLines = 6
//# min=0 max=1 step=0.01 default=0.22
export function sliderNumberOfLines(v) {
  numLines = floor(1 + v * (MAX_LINES - 1))
}

// Width in unit-square terms: hairline up to matrix-flooding.
// Max per-arm width shrinks as more arms are added.
var widthSetting = 0.5
//# min=0 max=1 step=0.01 default=0.5
export function sliderLineWidth(v) {
  widthSetting = v
}

var animateWidth = 1
//# min=0 max=1 step=1 default=1
export function sliderAnimateWidth(v) {
  animateWidth = v > 0.03   // on except at the very bottom of travel
}

// Rotation: inverted + quadratic-eased, ~10x range, never stops.
// time() interval 0.03 => one revolution ~2 s.
var rotInterval = 0.032
//# min=0 max=1 step=0.01 default=0.7
export function sliderRotationSpeed(v) {
  rotInterval = 0.06 - 0.054 * v * v   // 0.06 (slow) .. 0.006 (fast)
}

// Hue drift: same inverted/eased shape, sub-second up to tens of seconds.
var hueInterval = 0.2
//# min=0 max=1 step=0.01 default=0.6
export function sliderColorSpeed(v) {
  hueInterval = 0.35 - 0.34 * v * v    // ~23 s .. ~0.65 s per hue cycle
}

function widthFromSetting(v) {
  return 0.003 + v * v * (0.9 / numLines)
}

var halfWidth = 0.05

export function beforeRender(delta) {
  var angle = time(rotInterval) * PI2   // continuous rotation
  var hueBase = time(hueInterval)       // shared hue drift

  if (animateWidth) {
    // triangle wave, ~one minute per thick-thin-thick cycle
    halfWidth = widthFromSetting(triangle(time(0.9)))
  } else {
    halfWidth = widthFromSetting(widthSetting)
  }

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
