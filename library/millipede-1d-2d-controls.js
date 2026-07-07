// name: millipede 1d/2d controls
// Clean-room reimplementation from a prose functional description of the
// community pattern "millipede 1d/2d controls"; original source never
// consulted.

// Rippling millipede-leg waves: a rainbow hue ramp chopped into repeating
// sawtooth segments ("legs") with a traveling sine brightness wave riding
// the color structure. In 2D the strip position becomes the angle around a
// movable center, optionally split into concentric phase-staggered rings.

var legs = 10              // number of leg segments (1..20)
var speedN = 30            // quantized speed (1..60)
var centerX = 0.5
var centerY = 0.5
export var tiers = 3       // concentric rings in 2D (1..5)

var t1 = 0                 // primary clock (unused directly; kept for feel)
var t2 = 0                 // slower clock, half the rate

//# min=0 max=1 step=0.05 default=0.45
export function sliderLegs(v) {
  legs = floor(1 + v * 19.99)
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  speedN = floor(1 + v * 59.99)     // ~60:1 span, quantized to integers
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderPositionX(v) {
  centerX = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderPositionY(v) {
  centerY = v
}

//# min=0 max=1 step=0.2 default=0.5
export function sliderTiers(v) {
  tiers = floor(1 + v * 4.99)
}

export function beforeRender(delta) {
  // periods inversely proportional to speed; mid-slider ≈ 1 s wave cycle
  t1 = time(0.45 / speedN)
  t2 = time(0.9 / speedN)           // half the rate (twice the period)
}

// shared shading: pos is 0..1 along the strip (or around the circle),
// phaseOff staggers the brightness wave (used by the 2D ring tiers)
function shade(pos, phaseOff) {
  // drifting sawtooth segments: half the leg count, wrapped at one-half,
  // keeps the hue excursion in a tasteful sub-range
  var h = pos + mod((pos + t2) * legs / 2, 0.5)
  var v = wave(h + t2 + phaseOff)
  v = v * v                          // gamma-ish squaring; troughs go black
  hsv(h, 1, v)
}

export function render(index) {
  shade(index / pixelCount, 0)
}

export function render2D(index, x, y) {
  var dx = x - centerX
  var dy = y - centerY
  var r = hypot(dx, dy)
  var a = atan2(dy, dx) / PI2        // -0.5..0.5 around the center
  // quantized radius gives each concentric band a fixed phase offset
  var ring = floor(r * 1.5 * tiers) / tiers
  shade(mod(a, 1), ring)
}
