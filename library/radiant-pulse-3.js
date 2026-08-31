// name: radiant pulse 3
// Clean-room reimplementation from a prose functional description of the
// community pattern "radiant pulse 3"; original source never consulted.

// Slow pulses radiate outward/inward around the layout center, morphing
// between concentric rings, straight beams, and a rotating three-lobed
// clover. Every timescale derives from ONE ~5-minute clock: its sine is
// multiplied by large factors, so the seconds-scale pulsing, the lobe
// rotation, and the in/out direction flips all breathe together.

var t, pulse, lobePhase, radK

// Tunables — the top-level values are the constants the port shipped with, so
// an untouched pattern renders exactly as before.
var master = 4.6         // time() interval of the master clock (~300 s)
var cycles = 20          // pulse cycles per swing of the master clock
var lobes = 3            // leaves in the rotating clover
var ringK = 7            // peak ring density (phase cycles per unit radius,
                         //   swinging between -ringK/2 and +ringK/2)
var hueSpread = 0.2      // how far hue fans over angle and radius

// Seconds for the master clock — every other timescale is locked to it, so
// this is the one "speed" dial.
//# min=10 max=600 step=10 default=300
export function sliderCycleSeconds(v) { master = max(v, 5) / 65.536 }

// How many pulses sweep past during one swing of the master clock.
//# min=1 max=60 step=1 default=20
export function sliderPulseCycles(v) { cycles = clamp(v, 0, 100) }

// Leaves in the rotating clover: 1 is a single sweeping beam, 3 the original.
// (Above ~6 the lobes alias into hash on a small grid.)
//# min=1 max=6 step=1 default=3
export function sliderLobes(v) { lobes = clamp(floor(v), 1, 12) }

// Ring density at the peak of the swing — how many bright rings fit between
// the center and the edge. 0 removes the rings and leaves the lobes alone.
//# min=0 max=20 step=0.5 default=7
export function sliderRingDensity(v) { ringK = clamp(v, 0, 40) }

// How far the hue fans across angle and radius, as a percentage of the color
// wheel; 0 paints everything a single slowly drifting color.
//# min=0 max=100 step=1 default=20
export function sliderHueSpreadPercent(v) { hueSpread = clamp(v, 0, 100) / 100 }

export function beforeRender(delta) {
  t = time(master)                    // master clock, ~300 s period
  var s = sin(t * PI2)
  pulse = s * cycles                  // ~20 pulse cycles per swing
  lobePhase = s * 15                  // spins the three-leaf form
  radK = (wave(t) - 0.5) * ringK      // ring density, roughly -3.5 .. +3.5;
                                      // sign flips outward/inward, near zero
                                      // the lobes/beams dominate
}

export function render3D(index, x, y, z) {
  // Center the planar coordinates; depth is deliberately ignored, so every
  // horizontal layer of a true 3D map shows the same image.
  x -= 0.5
  y -= 0.5
  var r = hypot(x, y)
  var ang = atan2(x, y)               // swapped args just rotate "angle zero"

  // Sum three phase terms, wrap, triangle: unbounded phase becomes soft
  // repeating bands; squaring shapes them into pulses with dark gaps.
  var ph = pulse + sin(ang * lobes + lobePhase) + r * radK
  var v = triangle(mod(ph, 1))
  v = v * v

  // Bright cores desaturate toward pastel/white; dim regions stay rich.
  var s = 1.5 - v

  // Hue fans gently with direction and distance, and the whole palette
  // cycles once around the wheel per master-clock period.
  var h = triangle(ang / PI2) * hueSpread + r * hueSpread + t

  hsv(h, s, v)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}
