// name: Radiant Bloom
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/radiant-pulse-3.md — reimagined, not a replica.
// Soft bands pulse out of and into the centre, morphing between concentric rings,
// straight beams and a spinning clover; bright cores bleach to pastel. One clock drives
// it all, so pulse speed, spin and in/out direction breathe and reverse together.
// Each pixel's angle and radius are tabled once and the angle-only terms come from a
// 512-entry table rebuilt per frame: no trig per pixel. Above 4,096 px a grid reads a
// 64-wide table by coordinate (2x2 cells on 128x128).

var TN = min(pixelCount, 4096), NB = 512
var A = array(TN), R = array(TN)  // per pixel: angle bin, radius
var SA = array(NB + 1), HA = array(NB + 1)
var mode = 0, cw = 0, ch = 0, TS = 0, key = -1, aspX = 1, aspY = 1, ab = 0

var master = 60                   // seconds per morph cycle
var cycles = 6                    // pulses per swing of the clock
var lobes = 3
var ringK = 7                     // peak ring density
var hueSpread = 0.3               // hue fan across angle and radius, turns
var t = 0, radK = 0

// Seconds for the master clock: one full rings -> beams -> rings morph and one hue lap.
//# min=10 max=600 step=5 default=60
export function sliderCycleSeconds(v) { master = clamp(v, 10, 600) }

// How many pulses sweep past during one swing of the clock.
//# min=1 max=30 step=1 default=6
export function sliderPulseCycles(v) { cycles = clamp(v, 1, 30) }

// Leaves in the spinning clover: 1 is a single sweeping beam.
//# min=1 max=8 step=1 default=3
export function sliderLobes(v) { lobes = clamp(floor(v), 1, 8) }

// Ring density at the peak of the swing; 0 leaves the lobes alone.
//# min=0 max=20 step=0.5 default=7
export function sliderRingDensity(v) { ringK = clamp(v, 0, 20) }

// How far the hue fans across angle and radius, percent of the colour wheel.
//# min=0 max=100 step=1 default=30
export function sliderHueSpreadPercent(v) { hueSpread = clamp(v, 0, 100) / 100 }

function polar(x, y) {             // -> radius; angle bin left in ab
  x = (x - 0.5) * aspX; y = (y - 0.5) * aspY
  ab = floor((atan2(y, x) / PI2 + 0.5) * NB)
  return hypot(x, y) * 2
}

export function beforeRender(delta) {
  var w = gridWidth(), h = gridHeight()
  if (w * 4096 + h != key) {
    key = w * 4096 + h
    var m = max(max(w, h), 1)
    aspX = w > 0 ? w / m : 1; aspY = h > 0 ? h / m : 1
    // 1: table per pixel; 2: a <= 4096-cell table read by coordinate; 0: no table
    mode = pixelCount <= TN ? 1 : (w > 0 ? 2 : 0)
    cw = min(w, 64); ch = min(h, floor(TN / max(cw, 1)))
    TS = mode == 1 ? pixelCount : (mode == 2 ? cw * ch : 0)
    for (var i = 0; i < TS; i++) {
      if (mode == 1) R[i] = polar(pixelCoord(i, 0), pixelCoord(i, 1))
      else R[i] = polar((i % cw + 0.5) / cw, (floor(i / cw) + 0.5) / ch)
      A[i] = ab
    }
  }
  t = mod(t + delta / 1000 / master, 1)
  var s = sin(t * PI2)
  var pulse = s * cycles, spin = s * 4
  radK = (wave(t) - 0.5) * ringK    // sign flips outward/inward; near 0 the beams win
  var k = PI2 * lobes / NB
  for (i = 0; i <= NB; i++) {
    SA[i] = pulse + sin(i * k + spin)
    HA[i] = triangle(i / NB) * hueSpread + t
  }
}

export function render2D(index, x, y) {
  var r, a
  if (mode == 1) { r = R[index]; a = A[index] }
  else if (mode) {
    var i = floor(y * ch * 0.9999) * cw + floor(x * cw * 0.9999)
    r = R[i]; a = A[i]
  } else { r = polar(x, y); a = ab }
  var v = triangle(SA[a] + r * radK)
  v = v * v
  hsv(HA[a] + r * hueSpread, 1.5 - v, v)
}
