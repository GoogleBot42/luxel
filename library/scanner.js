// name: Scanner
// Clean-room reimplementation from a prose functional description of the
// community pattern "Scanner"; original source never consulted.
//
// Doubled KITT/Cylon scanner: the strip splits at its midpoint into two
// halves, each with a bright lead dot bouncing between its ends and leaving
// a fading comet trail (brightness cubed at output for crisp heads). Starts
// mirrored from the outer ends. Rainbow mode freezes a double rainbow along
// the strip; otherwise one hue slowly sweeps the wheel.

var mid = floor((pixelCount - 1) / 2)
var bright = array(pixelCount + 1)   // +1 = overflow guard for the fill walk

var lo = array(2)
var hi = array(2)
var pos = array(2)      // lead position, pixel units (float)
var dir = array(2)      // +1 / -1
var prevIdx = array(2)  // last frame's integer lead position

lo[0] = 0
hi[0] = mid
lo[1] = mid + 1
hi[1] = pixelCount - 1
pos[0] = 0
dir[0] = 1
prevIdx[0] = 0
pos[1] = pixelCount - 1
dir[1] = -1
prevIdx[1] = pixelCount - 1

var halfLen = mid + 1

// control state (defaults match the slider defaults below)
var speedPxMs = (0.3 + 1.8 * 0.4) * halfLen / 1000  // ~1 s per one-way sweep
var decayPerMs = 0.016 / (1 + 15 * 0.5)
var periodMs = 8000 + (1 - 0.5) * 96000
var rainbow = 1
var huePhase = 0
var gHue = 0            // initialized so frame one never sees an undefined hue

//# min=0 max=1 step=0.01 default=0.4
export function sliderSpeed(v) {
  // ~7x band between slowest and fastest; full slider = fastest
  speedPxMs = (0.3 + 1.8 * v) * halfLen / 1000
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderTrailLength(v) {
  // inverse map to decay rate, ~16x band; full slider = longest trails
  decayPerMs = 0.016 / (1 + 15 * v)
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderColorShift(v) {
  // inverse map to hue cycle period, ~13x band; full slider = fastest cycling
  periodMs = 8000 + (1 - v) * 96000
}

export function toggleRainbow(v) {
  rainbow = v
}

export function beforeRender(delta) {
  // single-hue mode: triangle sweep through the wheel (recomputed per frame)
  huePhase = frac(huePhase + delta / periodMs)
  gHue = triangle(huePhase)

  for (var h = 0; h < 2; h++) {
    // advance, then bounce off this half's boundaries
    pos[h] += dir[h] * delta * speedPxMs
    if (pos[h] > hi[h]) { pos[h] = hi[h]; dir[h] = -1 }
    if (pos[h] < lo[h]) { pos[h] = lo[h]; dir[h] = 1 }

    // gap fill: paint every pixel crossed since last frame so fast leads
    // never leave dotted trails
    var ni = floor(pos[h])
    var pi = prevIdx[h]
    var step = ni >= pi ? 1 : -1
    for (var i = pi; i != ni + step; i += step) bright[i] = 1
    prevIdx[h] = ni
  }

  // linear decay, clamped at zero (trail-length control = decay rate)
  for (var i = 0; i < pixelCount; i++) {
    bright[i] = max(0, bright[i] - delta * decayPerMs)
  }
}

export function render(index) {
  var b = bright[index]
  // double rainbow along the strip (one full wheel per half), or shared hue
  var h = rainbow ? index / pixelCount * 2 : gHue
  hsv(h, 1, b * b * b)
}
