// name: opposites
// Clean-room reimplementation from a prose functional description of the
// community pattern "opposites"; original source never consulted.

// Two swells travel in opposite directions along the strip; a third wave is
// warped by their sum. Brightness is the squared product of all three (each
// lifted slightly off zero), so light appears only where they coincide.
// Hue folds into a narrow band, half of which is kicked to the complementary
// side of the wheel, and the whole palette drifts with the faster clock.

var BAND = 0.34          // width of the folded hue band
var LIFT = 0.1           // keeps the wave product from pinning to zero

export function beforeRender(delta) {
  tA = time(0.09)        // ~5.9 s cycle
  tB = time(0.18)        // ~11.8 s cycle, twice as slow
}

export function render(index) {
  var p = index / pixelCount

  var w1 = wave(tA + p)              // swell travelling one way
  var w2 = wave(tB - p)              // slower swell, opposite way
  var w3 = wave(frac(p + w1 + w2))   // composite, warped by the first two

  // Fold into a narrow band; the lower half jumps to the far side of the
  // wheel, producing two near-complementary hue clusters.
  var h = w3 % BAND
  if (h < BAND / 2) h += 0.5
  h += tA                            // whole palette drifts; hsv() wraps hue

  var v = (w1 + LIFT) * (w2 + LIFT) * (w3 + LIFT)
  v = v * v                          // deepen darks, sharpen the blobs

  hsv(h, 1, v)
}
