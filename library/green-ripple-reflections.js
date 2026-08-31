// name: green ripple reflections
// Clean-room reimplementation from a prose functional description of the
// community pattern "green ripple reflections"; original source never
// consulted.

// Moonlight on water: three interfering waves slide along the strip on
// close-but-incommensurate clocks. Where the green wave is weak, bright
// crests desaturate to white — specular-looking glints. Brightness is
// capped around half to stay moody.

var t1, t2, t3

// Tunables, initialized to the constants the port shipped with so the
// untouched render is unchanged; the sliders re-express them in real units.
var i1 = 0.0305       // time() intervals for the three wave clocks:
var i2 = 0.0458       //   ~2 s, ~3 s (1.5x) and ~2.5 s (1.25x)
var i3 = 0.0381
var c1 = 5            // spatial cycles of the main ripple ...
var c2 = 3            //   ... the counter-ripple (0.6x) ...
var c3 = 1.5          //   ... and the swaying triangle (0.3x)
var waterHue = 1 / 3  // hue of the water, as a position on the wheel
var glint = 1         // how far bright crests wash out to white, 0..1

// Period of the main ripple, in seconds. The other two clocks stay locked
// to it at 1.5x and 1.25x, which is what makes the beat drift.
//# min=0.3 max=10 step=0.1 default=2
export function sliderWavePeriod(v) {
  var base = max(v, 0.3) / 65.536
  i1 = base
  i2 = base * 1.5
  i3 = base * 1.25
}

// Number of ripple crests along the strip. The counter-ripple and the
// swaying wave keep their 0.6x / 0.3x relationship to it.
//# min=1 max=12 step=1 default=5
export function sliderRippleCount(v) {
  c1 = clamp(floor(v), 1, 12)
  c2 = c1 * 0.6
  c3 = c1 * 0.3
}

// Water color, as a position on the color wheel (0.33 = green,
// 0.5 = cyan, 0.62 = blue).
//# min=0 max=1 step=0.01 default=0.33
export function sliderWaterHue(v) {
  waterHue = clamp(v, 0, 1)
}

// How strongly crests desaturate into white specular glints:
// 0 = pure saturated color everywhere, 1 = the original moonlight look.
//# min=0 max=1 step=0.05 default=1
export function sliderGlint(v) {
  glint = clamp(v, 0, 1)
}

export function beforeRender(delta) {
  // ~2 s, ~3 s, ~2.5 s sawtooths, each scaled to a full circle of phase.
  t1 = time(i1) * PI2
  t2 = time(i2) * PI2
  t3 = time(i3) * PI2
}

export function render(index) {
  var p = index / pixelCount

  // 1) ~5 spatial cycles drifting one way, squared: non-negative, sharpened
  //    crests, doubled apparent frequency. Also drives saturation below.
  var w1 = sin(p * c1 * PI2 - t1)
  w1 = w1 * w1

  // 2) ~3 spatial cycles drifting the other way; left signed.
  var w2 = sin(p * c2 * PI2 + t2)

  // 3) ~1.5-cycle triangle whose phase sways back and forth sinusoidally.
  var w3 = triangle(mod(p * c3 + 0.3 * sin(t3), 1))

  // Average, square (folds negative troughs into faint glow, deepens
  // contrast), and halve to cap overall brightness.
  var v = (w1 + w2 + w3) / 3
  v = v * v / 2

  // Green where w1 is strong; washes to white where it is weak, so glints
  // made by the other waves land white exactly where the green wave "isn't".
  hsv(waterHue, 1 - glint * (1 - w1), v)
}
