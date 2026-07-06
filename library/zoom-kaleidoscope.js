// name: zoom kaleidoscope
// Clean-room reimplementation from a prose functional description of the
// community pattern "zoom kaleidoscope"; original source never consulted.

// An endlessly zooming kaleidoscopic interference lattice: Chebyshev
// (square) and Manhattan (diamond) ring fields, both advanced by a shared
// phase, are combined with a bitwise XOR of their 16.16 fixed-point
// representations — the load-bearing trick that yields a fractal moiré that
// reads as an infinite zoom. Ridged perlin noise dresses the lattice in
// shimmering texture; hue creeps around the wheel; optional wander, rocking
// rotation, and radial spokes.

var lastV = array(pixelCount)   // per-pixel brightness smoothing state
var lastH = array(pixelCount)   // per-pixel hue smoothing state

var rotateOn = 0
var moveOn = 0
var spokes = 0
var zoomScale = 1.5     // matches slider default 0.5
var blend = 0.5         // matches slider default 0.5
var conLo = 0.125       // matches slider default 0.5
var conHi = 0.89

export function toggleRotate(v) { rotateOn = v }
export function toggleMoveAround(v) { moveOn = v }

// quantized whole spoke count, none up to eight
//# min=0 max=1 step=0.01 default=0
export function sliderExtraGeometry(v) { spokes = floor(v * 8.99) }

// really zoom amount: higher = smaller scale factor = more magnified;
// a tiny floor keeps it from degenerate zero scale
//# min=0 max=1 step=0.01 default=0.5
export function sliderZoomSpeed(v) { zoomScale = max(0.03, (1 - v) * 3) }

// top = tiny blend coefficient (heavy trails), bottom = near-instant
//# min=0 max=1 step=0.01 default=0.5
export function sliderSmooth(v) { blend = max(0.02, 1 - v) }

// squared response; higher narrows the smoothstep window
//# min=0 max=1 step=0.01 default=0.5
export function sliderContrast(v) {
  var c = v * v
  conLo = c * 0.5
  conHi = 1 - c * 0.45
}

var ringPhase = 0
var driftA = 0
var driftB = 0
var hueT = 0

export function beforeRender(delta) {
  // several-minute time base multiplied up so the rings visibly flow
  ringPhase = time(4) * 40
  // two very slowly drifting noise-sampling axes
  driftA = time(8) * 25
  driftB = time(5.3) * 25
  // global hue lap on the order of a minute and a half
  hueT = time(1.5)

  resetTransform()
  translate(-0.5, -0.5)          // center of the mapped area = origin
  if (moveOn) {
    // organic meander: sines of slowly-evolving perlin noise, at most
    // about a third of the display
    var wt = time(2) * 10
    translate(0.3 * sin(PI2 * perlin(wt, 1.7, 0, 8)),
              0.3 * sin(PI2 * perlin(3.1, wt, 0, 8)))
  }
  if (rotateOn) {
    // slow rock back and forth, a full turn out and back over ~a minute
    rotate(PI2 * triangle(time(0.9)))
  }
  scale(zoomScale, zoomScale)
}

export function render2D(index, x, y) {
  // two ring families around the origin: concentric squares and diamonds
  var cheb = max(abs(x), abs(y))
  var manh = abs(x) + abs(y) + 0.07

  // advance both by the shared phase, then XOR the raw 16.16 bits and fold
  var f = triangle((cheb - ringPhase) ^ (manh - ringPhase))

  // hue from the field plus the slow rotation; light fixed smoothing, then
  // folded so it wraps smoothly
  var h = f / 2 + hueT
  lastH[index] = lastH[index] + (h - lastH[index]) * 0.1
  var hue = triangle(lastH[index])

  // pie-slice spokes sculpt brightness only (added after hue is computed)
  if (spokes > 0) {
    f = f + triangle(spokes * atan2(y, x) / PI2)
  }

  // ridged noise texture: the field is one axis, the slow drifts the others
  var v = perlinRidge(f * 2, driftA, driftB, 2, 0.8, 1, 3)

  // user-set temporal smoothing, contrast window, then ~4th power blacks
  lastV[index] = lastV[index] + (v - lastV[index]) * blend
  v = smoothstep(conLo, conHi, lastV[index])
  v = v * v
  hsv(hue, 1, v * v)
}
