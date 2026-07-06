// name: Complements 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Complements 3D"; original source never consulted.

// A gradient between two complementary hues along one axis (z in 3D, y in
// 2D, strip position in 1D). The crossover zone is deliberately dimmed so
// the muddy RGB complement mix reads as a dark waistline instead of gray.
// The hue pair rotates once around the wheel every ~10 seconds.

var cycleSecs = 10       // full trip around the color wheel
var wheelOffset = 0.5    // 0.5 = complements; 1/3 or 2/3 would give triads
var dimDepth = 0.7       // attenuation at the midpoint (~two-thirds+)

var phase = 0
var colA = array(3)
var colB = array(3)

//# min=2 max=60 step=1 default=10
export function sliderCycleSeconds(v) {
  cycleSecs = max(2, v)
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderWheelOffset(v) {
  wheelOffset = v
}

//# min=0 max=0.95 step=0.01 default=0.7
export function sliderDimmingDepth(v) {
  dimDepth = v
}

export function beforeRender(delta) {
  phase += delta / 1000 / cycleSecs
  phase = frac(phase)
  // Endpoint colors depend only on the frame's phase — compute once here.
  hsv2rgb(phase, 1, 1, colA)
  hsv2rgb(phase + wheelOffset, 1, 1, colB)
}

// t = 0 shows color B, t = 1 shows color A, dark waistline in between.
function paintBlend(t) {
  var r = colB[0] + (colA[0] - colB[0]) * t
  var g = colB[1] + (colA[1] - colB[1]) * t
  var b = colB[2] + (colA[2] - colB[2]) * t

  var env
  if (t < 0.5) {
    // linear descent into the midpoint...
    env = 1 - dimDepth * t * 2
  } else {
    // ...slightly convex (quadratic) climb back out
    var u = (t - 0.5) * 2
    env = (1 - dimDepth) + dimDepth * u * u
  }
  rgb(r * env, g * env, b * env)
}

export function render3D(index, x, y, z) {
  paintBlend(z)          // every horizontal slice is uniform
}

export function render2D(index, x, y) {
  paintBlend(y)
}

export function render(index) {
  paintBlend(index / pixelCount)
}
