// name: Red-Green XY 2D Sweep
// Clean-room reimplementation from a prose functional description of the
// community pattern "Red-Green XY 2D Sweep"; original source never consulted.

// Mapping diagnostic: a narrow red band sweeps left-to-right, then a narrow
// green band sweeps top-to-bottom, alternating forever (~3 s per sweep).

var phase, sweep

export function beforeRender(delta) {
  var t = time(0.09)              // ~5.9 s full cycle, two sweeps
  phase = t < 0.5 ? 0 : 1         // 0 = horizontal/red, 1 = vertical/green
  sweep = frac(t * 2)             // rescaled to a full 0..1 run per phase
}

function band(c) {
  // smooth bump of (half coordinate - clock), raised to a high power so the
  // broad sinusoid collapses into one narrow bright line
  return pow(wave((c - sweep) / 2 + 0.25), 20)
}

export function render2D(index, x, y) {
  if (phase == 0) {
    hsv(0, 1, band(x))            // red, sweeping in x
  } else {
    hsv(1 / 3, 1, band(y))        // green, sweeping in y
  }
}

// 1D adaptation: normalized index stands in for both axes
export function render(index) {
  render2D(index, index / pixelCount, index / pixelCount)
}
