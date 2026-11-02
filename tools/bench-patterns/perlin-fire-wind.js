// name: perlin fire wind
// Clean-room reimplementation from a prose functional description of the
// community pattern "perlin fire wind"; original source never consulted.
// DELIBERATELY DEPARTS FROM THE ORIGINAL (2026-09-01 review): Jeremy forked
// this one — fidelity to the corpus pattern is waived, the review note was
// "add controls", and the brief is the same flame quality as its sibling
// perlin-fire plus visible wind. The vestigial read-only mode readout is gone;
// the flame model, palette and shared dials are kept identical to the sibling
// so the two read as a pair, with a wind-strength dial added on top.

// Same base-anchored noise fire as perlin-fire, blown sideways. The wind has a
// slowly reversing direction, gusts that pulse and die away, and a lull
// envelope, so the flame leans, snaps back and streams. The lean is weighted
// by height, so the base stays rooted while the tips are dragged downwind and
// the embers are carried furthest of all.

const WRAP = 256           // noise lattice repeat period (seamless looping)

// ---- controls (real units; the //# directive is what the UI sends) ----
var flameH = 0.85          // flame reach, fraction of the panel height
var flameW = 0.7           // flame width, fraction of the panel width
var detail = 3             // noise features per panel height
var riseSpeed = 1.1        // panel heights per second
var sparkAmt = 0.35        // ember density, 0..1
var windAmt = 0.45         // wind strength, 0..1

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

// Peak lean of the flame tips, as a percentage of the panel width.
//# min=0 max=100 step=1 default=45
export function sliderWind(v) { windAmt = v / 100 }

// Palette installed ONCE (see the sibling): an array literal inside
// beforeRender allocates a fresh arena entry every frame and arrays are never
// freed, so a per-frame setPalette exhausts the element budget.
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
var wind = 0               // signed wind this frame, panel widths at the tips
var breath = 1             // the fire surges and settles

export function beforeRender(delta) {
  setPerlinWrap(WRAP, WRAP, WRAP)
  var dt = delta / 1000
  riseOff = mod(riseOff + riseSpeed * detail * dt, WRAP)
  morphOff = mod(morphOff + 0.45 * dt, WRAP)
  emberOff = mod(emberOff + (0.5 + riseSpeed * 2.2) * detail * 2.5 * dt, WRAP)
  breath = 0.84 + 0.16 * wave(time(3.1 / 65.536)) + 0.08 * wave(time(1.13 / 65.536))

  // Wind = a slowly reversing direction, a gust that pulses (squared, so the
  // puffs are short and the lulls between them long) and a slower envelope
  // that makes whole windy spells come and go.
  var dir = 0.72 * sin(time(19 / 65.536) * PI2) + 0.28 * sin(time(7.7 / 65.536) * PI2)
  var gust = wave(time(5.1 / 65.536))
  gust = gust * gust
  var spell = 0.25 + 0.75 * wave(time(14 / 65.536))
  wind = windAmt * dir * (0.3 + 1.1 * gust) * spell
}

// How far downwind a point at height `hb` above the base is dragged, in panel
// widths. The exponent roots the base and drags the tips.
function lean(hb) {
  return wind * pow(hb, 1.4)
}

// Heat of the flame body — identical to the sibling's model. `u` is the
// horizontal offset from the (already leaned) flame axis in half-widths, `sx`
// the same offset in noise units, `hb` the height above the base.
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

  // Ember bed: the bottom rows always glow, which anchors the fire against
  // the wind. It flickers with the same noise, so it is coals, not a light bar.
  var bed = env * saturate((0.14 - hb) * 8) * (0.35 + 0.55 * n) * breath
  return max(h, bed)
}

// Sparse embers that break loose and drift up faster than the flame.
function emberAt(x, hb) {
  if (sparkAmt <= 0) return 0
  var s = perlin(x * detail * 9, hb * detail * 9 - emberOff, morphOff * 3 + 11, 3)
  var v = (s - (0.64 - sparkAmt * 0.27)) * 13
  if (v <= 0) return 0
  return saturate(v) * saturate(hb * 5) * saturate((1.15 - hb) * 1.8) *
         saturate(1.3 - abs(x - 0.5) * 2.6) * (0.3 + 0.7 * sparkAmt) * 0.9
}

export function render2D(index, x, y) {
  var hb = 1 - y                             // 0 at the bottom (base), 1 at top
  var off = lean(hb)
  var axis = 0.5 + off                       // the whole column leans downwind
  var u = (x - axis) / (flameW * 0.5)
  var sx = (x - axis) * detail * 1.7
  // Embers are lighter than the flame, so the wind carries them further.
  var h = flameAt(sx, u, hb) + emberAt(x - off * 1.9, hb)
  paint(clamp(h, 0, 0.999), clamp(h, 0, 1))
}

// 1D fallback: the strip becomes one flame column, index 0 at the base. There
// is no sideways to lean into, so the wind shows up as the gusts fanning the
// flame taller and hotter instead.
export function render(index) {
  var hb = index / max(pixelCount - 1, 1)
  var fan = 1 + abs(wind) * 0.5
  var h = flameAt(0, 0, hb / fan) * fan + emberAt(0.5, hb)
  paint(clamp(h, 0, 0.999), clamp(h, 0, 1))
}
