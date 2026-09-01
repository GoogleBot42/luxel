// name: Perlin fire
// Clean-room reimplementation from a prose functional description of the
// community pattern "Perlin fire"; original source never consulted.
// DELIBERATELY DEPARTS FROM THE ORIGINAL (2026-09-01 review): Jeremy forked
// this one — fidelity to the corpus pattern is waived and the brief is to make
// it a genuinely convincing fire. The original's four-flavour noise demo (and
// its mode dial) is replaced by one purpose-built flame model, and the dials
// are re-cut in real units. Its sibling perlin-fire-wind shares this model.

// A fire on a 2D matrix, anchored at the bottom edge. An ember bed always
// glows at the base; above it, vertically stretched turbulence thresholded
// against a height-dependent fuel ramp breaks into licking tongues that rise,
// thin out and die. The column wanders gently and breathes. Sparse embers
// break loose and drift up through the smoke. On a 1D strip the same heat
// profile runs along the strip as a single flame column.

const WRAP = 256           // noise lattice repeat period (seamless looping)

// ---- controls (real units; the //# directive is what the UI sends) ----
var flameH = 0.85          // flame reach, fraction of the panel height
var flameW = 0.7           // flame width, fraction of the panel width
var detail = 3             // noise features per panel height
var riseSpeed = 1.1        // panel heights per second
var sparkAmt = 0.35        // ember density, 0..1

//# min=20 max=100 step=1 default=85
export function sliderFlameHeight(v) { flameH = v / 100 }

//# min=15 max=100 step=1 default=70
export function sliderFlameWidth(v) { flameW = v / 100 }

//# min=1 max=8 step=0.5 default=3
export function sliderFlameDetail(v) { detail = v }

//# min=0 max=3 step=0.05 default=1.1
export function sliderRiseSpeed(v) { riseSpeed = v }

//# min=0 max=100 step=1 default=35
export function sliderEmbers(v) { sparkAmt = v / 100 }

// Palette installed ONCE: an array literal inside beforeRender allocates a
// fresh arena entry every frame, and arrays are never freed (PB-faithful) —
// per-frame setPalette([...]) exhausts the element budget at frame ~426.
setPalette([
  0.00, 0,    0,    0,       // black
  0.13, 0.28, 0.02, 0,       // smouldering ember
  0.34, 1,    0.08, 0,       // deep red
  0.58, 1,    0.40, 0.01,    // orange
  0.82, 1,    0.80, 0.10,    // yellow
  1.00, 1,    1,    0.90     // white-hot core
])

var riseOff = 0            // vertical scroll of the flame noise (noise units)
var morphOff = 0           // slow shape morph (third noise axis)
var emberOff = 0           // vertical scroll of the ember field
var sway = 0               // horizontal wander of the column axis
var breath = 1             // the fire surges and settles

export function beforeRender(delta) {
  setPerlinWrap(WRAP, WRAP, WRAP)
  var dt = delta / 1000
  // Offsets are wrapped at the lattice period, so every loop is seamless no
  // matter how the speed dial has been moved along the way.
  riseOff = mod(riseOff + riseSpeed * detail * dt, WRAP)
  morphOff = mod(morphOff + 0.45 * dt, WRAP)
  emberOff = mod(emberOff + (0.5 + riseSpeed * 2.2) * detail * 2.5 * dt, WRAP)
  sway = sin(time(7.3 / 65.536) * PI2) * 0.06 + sin(time(4.1 / 65.536) * PI2) * 0.03
  breath = 0.84 + 0.16 * wave(time(3.1 / 65.536)) + 0.08 * wave(time(1.13 / 65.536))
}

// Heat of the flame body. `u` is the horizontal offset from the flame axis in
// half-widths (0 on the axis, ±1 at the edge), `sx` the same offset in noise
// units, `hb` the height above the base (0 at the base, 1 at the top).
function flameAt(sx, u, hb) {
  var narrow = 1 - 0.45 * hb                 // the column tapers as it rises
  var q = u / narrow
  if (abs(q) >= 1) return 0
  var env = pow(1 - q * q, 0.75)             // soft window across the column

  // Vertically stretched turbulence: an abs-fractal sum makes filaments with
  // creases, not round blobs, and halving the vertical frequency stretches
  // them into tongues. Subtracting riseOff scrolls them upward.
  var n = perlinTurbulence(sx, hb * detail * 0.45 - riseOff, morphOff, 2.1, 0.55, 4)

  // Fuel burns out at the flame's reach, slightly faster than linear so the
  // tips taper instead of ending flat.
  var fuel = pow(saturate(1 - hb / flameH), 0.85)
  // Threshold the noise so the gaps between tongues really go out.
  var h = (n * 2.1 - 0.30) * env * fuel * breath

  // Ember bed: the bottom rows always glow, which anchors the fire. It
  // flickers with the same noise so it is a bed of coals, not a light bar.
  var bed = env * saturate((0.14 - hb) * 8) * (0.35 + 0.55 * n) * breath
  return max(h, bed)
}

// Sparse embers that break loose and drift up faster than the flame.
function emberAt(x, hb) {
  if (sparkAmt <= 0) return 0
  var s = perlin(x * detail * 9, hb * detail * 9 - emberOff, morphOff * 3 + 11, 3)
  var v = (s - (0.64 - sparkAmt * 0.27)) * 13
  if (v <= 0) return 0
  // they appear above the ember bed and cool off before the top of the panel
  return saturate(v) * saturate(hb * 5) * saturate((1.15 - hb) * 1.8) *
         saturate(1.3 - abs(x - 0.5) * 2.6) * (0.3 + 0.7 * sparkAmt) * 0.9
}

export function render2D(index, x, y) {
  var hb = 1 - y                             // 0 at the bottom (base), 1 at top
  var axis = 0.5 + sway * hb                 // the column leans as it rises
  var u = (x - axis) / (flameW * 0.5)
  var sx = (x - axis) * detail * 1.7
  var h = flameAt(sx, u, hb) + emberAt(x - sway * hb, hb)
  paint(clamp(h, 0, 0.999), clamp(h, 0, 1))
}

// 1D fallback: the strip becomes one flame column, index 0 at the base.
export function render(index) {
  var hb = index / max(pixelCount - 1, 1)
  var h = flameAt(0, 0, hb) + emberAt(0.5, hb)
  paint(clamp(h, 0, 0.999), clamp(h, 0, 1))
}
