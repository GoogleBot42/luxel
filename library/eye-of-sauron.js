// name: Eye of Sauron
// Clean-room reimplementation from a prose functional description of the
// community pattern "Eye of Sauron"; original source never consulted. A wide
// almond of churning ridged-noise flame with a dark vertical slit pupil,
// filaments streaming outward, driven only by deterministic noise clocks.

var WRAP = 16            // integer noise wrap period the clocks sweep over

var angDensity = 8       // flame tendrils (integer; doubles as angular wrap)
var radDensity = 1
var dilation = 1.3
var slitness = 5.3
var PUPIL_DEPTH = 2      // how hard the pupil bites into the fire

var morphClock = 0       // slow reshape (third noise axis)
var radialClock = 0      // faster radial scroll -> outward streaming
var burnClock = 0        // seconds-scale clock: the flame licks and flickers

var SCALE = 3            // magnification; boundary radius = SCALE - 1
var YSTRETCH = 1.4       // vertical scaled ~40% more -> wide flattened almond

// Palette installed ONCE: an array literal inside beforeRender allocates a
// fresh arena entry every frame, and arrays are never freed (PB-faithful) —
// per-frame setPalette([...]) exhausts the element budget at frame ~426.
setPalette([
  0.00, 0, 0, 0,
  0.20, 0.6, 0, 0,
  0.55, 1, 0.35, 0,
  0.80, 1, 0.9, 0,
  1.00, 1, 1, 1
])

export function beforeRender(delta) {
  // both clocks sweep 0..WRAP so the noise tiles seamlessly in time
  morphClock = time(6.4) * WRAP    // ~7 min of unique noise
  radialClock = time(0.75) * WRAP  // ~49 s, subtracted -> outward streaming
  burnClock = time(0.09) * WRAP    // ~6 s -> per-frame flicker of the fire
  // tile noise around the full circle on the angular axis
  setPerlinWrap(angDensity, WRAP, WRAP)
}

// Bounds are declared in real units, so the handlers take the value straight.

// flame tendrils around the circle (integer — it is also the angular wrap)
//# min=2 max=18 step=1 default=8
export function sliderAngularDensity(v) {
  angDensity = max(2, floor(v))
}

// noise stretch along the radius: low = long streaming licks, high = fine grain
//# min=0.1 max=2 step=0.05 default=1
export function sliderRadialDensity(v) {
  radDensity = v
}

// pupil size, in transformed units (the eye's vertical half-extent is ~2.1)
//# min=0.5 max=2 step=0.05 default=1.3
export function sliderDilation(v) {
  dilation = v
}

// horizontal compression of the pupil: 1 = round, 9 = razor slit
//# min=1 max=9 step=0.1 default=5.3
export function sliderSlitness(v) {
  slitness = v
}

export function render2D(index, x, y) {
  // center on the eye and magnify (more vertically -> flattened almond)
  var cx = (x - 0.5) * SCALE
  var cy = (y - 0.5) * SCALE * YSTRETCH

  var radius = hypot(cx, cy)
  var angle = atan2(cy, cx) / PI2 + 0.5   // 0..1 around the circle

  // Ridged fractal noise: a few octaves, half gain, ridge offset just above
  // one — the offset is what folds each octave into a sharp bright crease, so
  // the iris reads as licking filaments instead of soft clouds.
  var n = perlinRidge(angle * angDensity, radius * radDensity - radialClock, morphClock, 2, 0.5, 1.05, 3)

  // Burning turbulence: the same ridged field at double frequency on a
  // seconds-scale clock, folded in multiplicatively. It keeps the filament
  // layout of the slow field but makes every lick flare and gutter, which is
  // what reads as fire rather than as a still noise texture.
  var lick = perlinRidge(angle * angDensity * 2, radius * radDensity * 2 - burnClock, burnClock, 2, 0.5, 1.05, 2)
  n = n * (0.5 + 1.5 * lick)

  // oval edge fade: square of inwardness from the outer boundary
  var boundary = SCALE - 1
  var inward = clamp(boundary - radius, 0, 1)
  n = n * inward * inward

  // pupil: a tall narrow dark slit (x weighted heavily); only ever subtracts
  var slit = hypot(cx * slitness, cy)
  n = n - max(0, dilation - slit) * PUPIL_DEPTH

  // White-hot corona hugging the slit. Applied AFTER the pupil subtraction and
  // as a pure multiply, so it scales the fire without moving the value's zero
  // crossing — the pupil keeps exactly the size and shape it had.
  var near = clamp(1 - slit / (dilation * 1.4), 0, 1)
  n = n * (1 + 2.2 * near * near)

  var v = clamp(n, 0, 1)
  paint(v, v)
}
