// name: opposites
// Clean-room reimplementation from a prose functional description of the
// community pattern "opposites"; original source never consulted.

// Two swells travel in opposite directions along the strip; a third wave is
// warped by their sum. Brightness is the squared product of all three (each
// lifted slightly off zero), so light appears only where they coincide.
// Hue folds into a narrow band, half of which is kicked to the complementary
// side of the wheel, and the whole palette drifts with the faster clock.

// Tunables — the top-level values are the constants the port shipped with, so
// an untouched pattern renders exactly as before.
var band = 0.34          // width of the folded hue band (fraction of the wheel)
var lift = 0.1           // keeps the wave product from pinning to zero
var iA = 0.09            // time() interval of the fast swell (~5.9 s)
var iB = 0.18            // ... and of the slow one (twice as long)
var ratio = 2            // how many times slower the second swell is
var mirror = 0           // 1 = fold both swells about the strip's midpoint

// Seconds for the fast swell to travel the strip once. The slow swell keeps
// its ratio to this one.
//# min=1 max=30 step=0.5 default=6
export function sliderCycleSeconds(v) {
  iA = max(v, 0.5) / 65.536
  iB = iA * ratio
}

// How many times slower the opposing swell runs; 1 makes the two symmetric.
//# min=1 max=6 step=0.5 default=2
export function sliderSlowWaveRatio(v) {
  ratio = clamp(v, 1, 8)
  iB = iA * ratio
}

// Width of the folded hue band as a percentage of the color wheel: small
// values give two tight complementary clusters, large ones a full rainbow.
//# min=2 max=100 step=1 default=34
export function sliderHueSpreadPercent(v) { band = clamp(v, 1, 100) / 100 }

// How much the dark gaps between blobs glow, as a percentage: 0 leaves them
// black and the blobs sparse, higher values fill the strip in.
//# min=0 max=50 step=1 default=10
export function sliderGlowPercent(v) { lift = clamp(v, 0, 100) / 100 }

// Fold the swells about the middle of the strip, so both ends mirror.
//# default=0
export function toggleMirror(v) { mirror = v }

export function beforeRender(delta) {
  tA = time(iA)          // ~5.9 s cycle
  tB = time(iB)          // ~11.8 s cycle, twice as slow
}

export function render(index) {
  var p = index / pixelCount
  if (mirror) p = abs(p * 2 - 1)

  var w1 = wave(tA + p)              // swell travelling one way
  var w2 = wave(tB - p)              // slower swell, opposite way
  var w3 = wave(frac(p + w1 + w2))   // composite, warped by the first two

  // Fold into a narrow band; the lower half jumps to the far side of the
  // wheel, producing two near-complementary hue clusters.
  var h = w3 % band
  if (h < band / 2) h += 0.5
  h += tA                            // whole palette drifts; hsv() wraps hue

  var v = (w1 + lift) * (w2 + lift) * (w3 + lift)
  v = v * v                          // deepen darks, sharpen the blobs

  hsv(h, 1, v)
}
