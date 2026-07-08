// name: Eye of Sauron
// Clean-room reimplementation from a prose functional description of the
// community pattern "Eye of Sauron"; original source never consulted. A wide
// almond of churning ridged-noise flame with a dark vertical slit pupil,
// filaments streaming outward, driven only by deterministic noise clocks.

var WRAP = 16            // integer noise wrap period the clocks sweep over

var angDensity = 8       // flame tendrils (integer; doubles as angular wrap)
var radDensity = 1
var dilation = 0.35
var slitness = 3

var morphClock = 0       // slow reshape (third noise axis)
var radialClock = 0      // faster radial scroll -> outward streaming

var SCALE = 3            // magnification; boundary radius = SCALE - 1
var YSTRETCH = 1.4       // vertical scaled ~40% more -> wide flattened almond

export function beforeRender(delta) {
  // both clocks sweep 0..WRAP so the noise tiles seamlessly in time
  morphClock = time(6.4) * WRAP    // ~7 min of unique noise
  radialClock = time(3.0) * WRAP   // ~3 min, subtracted -> streaming
  // tile noise around the full circle on the angular axis
  setPerlinWrap(angDensity, WRAP, WRAP)
  setPalette([
    0.00, 0, 0, 0,
    0.20, 0.6, 0, 0,
    0.55, 1, 0.35, 0,
    0.80, 1, 0.9, 0,
    1.00, 1, 1, 1
  ])
}

//# min=2 max=18 step=1 default=8
export function sliderAngularDensity(v) {
  angDensity = floor(2 + v * 16)
}

//# min=0.1 max=2 step=0.05 default=1
export function sliderRadialDensity(v) {
  radDensity = 0.1 + v * 1.9
}

//# min=0.15 max=0.6 step=0.01 default=0.35
export function sliderDilation(v) {
  dilation = 0.15 + v * 0.45
}

//# min=1 max=6 step=0.1 default=3
export function sliderSlitness(v) {
  slitness = 1 + v * 5
}

export function render2D(index, x, y) {
  // center on the eye and magnify (more vertically -> flattened almond)
  var cx = (x - 0.5) * SCALE
  var cy = (y - 0.5) * SCALE * YSTRETCH

  var radius = hypot(cx, cy)
  var angle = atan2(cy, cx) / PI2 + 0.5   // 0..1 around the circle

  var n = perlinRidge(angle * angDensity, radius * radDensity - radialClock, morphClock)

  // oval edge fade: square of inwardness from the outer boundary
  var boundary = SCALE - 1
  var inward = clamp(boundary - radius, 0, 1)
  n = n * inward * inward

  // pupil: a tall narrow dark slit (x weighted heavily); only ever subtracts
  var slit = hypot(cx * slitness, cy)
  n = n - max(0, dilation - slit)

  var v = clamp(n, 0, 1)
  paint(v, v)
}
