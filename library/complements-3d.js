// name: Complements 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Complements 3D"; original source never consulted.
//
// A gradient between a hue and its complement along one axis (z in 3D,
// y in 2D, strip position in 1D). The crossover zone is dimmed so the
// muddy RGB complement mix reads as a dark waistline instead of gray.
// The hue pair rotates around the color wheel about once every ten seconds.

var cycleSec = 10     // seconds for one full trip around the wheel
var wheelOffset = 0.5 // 0.5 = complements; 1/3 or 2/3 would give triadic pairs
var dimDepth = 0.7    // attenuation at the midpoint (~two-thirds to three-quarters)

var phase = 0
var colA = array(3)
var colB = array(3)

//# min=0 max=1 step=0.01 default=0.25
export function sliderCycleTime(v) {
  // 2.5 s .. ~32 s per wheel rotation
  cycleSec = 2.5 + v * 30
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderWheelOffset(v) {
  wheelOffset = v
}

//# min=0 max=1 step=0.01 default=0.7
export function sliderDimDepth(v) {
  dimDepth = v
}

export function beforeRender(delta) {
  phase += delta / (cycleSec * 1000)
  phase = frac(phase)
  // The endpoint colors depend only on the frame's phase — convert once
  // per frame, not per pixel.
  hsv2rgb(phase, 1, 1, colA)
  hsv2rgb(frac(phase + wheelOffset), 1, 1, colB)
}

// c is the blend-axis coordinate, 0..1
function shade(c) {
  var r = mix(colB[0], colA[0], c)
  var g = mix(colB[1], colA[1], c)
  var b = mix(colB[2], colA[2], c)
  var v
  if (c < 0.5) {
    // linear ramp down toward the middle
    v = 1 - dimDepth * c * 2
  } else {
    // slightly convex (quadratic) climb back up
    var u = (c - 0.5) * 2
    v = 1 - dimDepth * (1 - u * u)
  }
  rgb(r * v, g * v, b * v)
}

export function render3D(index, x, y, z) {
  shade(z) // every horizontal slice is uniform
}

export function render2D(index, x, y) {
  shade(y)
}

export function render(index) {
  shade(index / pixelCount)
}
