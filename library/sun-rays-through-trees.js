// name: Sun rays through trees
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sun rays through trees"; original source never consulted.

// A warm sun just above the top-center of the panel; bright shafts fan
// downward through drifting turbulence like sunlight through a forest
// canopy. Near-white at the core, the chosen body color further out,
// black between the shafts. Slow and ambient — no beats, no RNG; all
// motion comes from the coherent noise field.

// Sun position in map coordinates (just above the visible frame)
var SUN_X = 0.5
var SUN_Y = -0.12

// Angular noise wrap: a full turn of angle is mapped onto WRAP noise
// units and the lattice is told to tile there, hiding the seam where
// atan2 wraps — without this a hard vertical crack shows under the sun.
var WRAP = 6
setPerlinWrap(WRAP, 0, 0)

var CORE_R = 0.32       // sun core radius (order of the coordinate scale)
var CORE_IN = 0.08      // small inner offset (~quarter of the core radius)

// --- controls ---
var rayStrength = 0.75
//# min=0 max=1 step=0.01 default=0.75
export function sliderRayStrength(v) { rayStrength = v }

var rayCountA = 5
//# min=0 max=1 step=0.125 default=0.5
export function sliderRayCountA(v) { rayCountA = 1 + floor(v * 8.49) }

var rayCountB = 3
//# min=0 max=1 step=0.125 default=0.25
export function sliderRayCountB(v) { rayCountB = 1 + floor(v * 8.49) }

var baseHue = 0.09, baseSat = 1, baseVal = 1   // warm golden amber
export function hsvPickerBaseColor(h, s, v) {
  baseHue = h
  baseSat = s
  baseVal = v
}

var colorVar = 0.12
//# min=0 max=1 step=0.01 default=0.3
export function sliderColorVariation(v) { colorVar = v * 0.4 }

var master, drift

export function beforeRender(delta) {
  master = time(1.8) * 4      // noise "time" axis, ~2 minute loop
  drift = time(0.45)          // radial streaming, ~30 s loop
}

export function render2D(index, x, y) {
  // polar coordinates about the (off-screen) sun
  var dx = x - SUN_X
  var dy = y - SUN_Y
  var angle = atan2(dy, dx)
  var d = hypot(dx, dy)

  // angle scaled so a full turn spans exactly the wrapped noise period
  var a = (angle / PI2 + 0.5) * WRAP

  // 3-octave turbulence over (angle, drifting radius, slow time), inverted
  // so bright regions sit where the "foliage" is thin
  var n = perlinTurbulence(a, d * 3 - drift * WRAP, master, 2, 0.5, 3)
  var v = 1 - clamp(n, 0, 1)

  // flare shaping: keep only the top of the noise range (discrete flare
  // tongues) OR let brightness grow as distance*noise falls toward the
  // core (guaranteed solid bright sun disc)
  var tongues = smoothstep(0.68, 0.95, v)
  var core = saturate(1 - (d - CORE_IN) * (1 - v * 0.7) / CORE_R)
  var flare = max(tongues, core)   // saved: also drives the hue shimmer

  // rays filter: wave-of-a-wave over the angle — primary count phase-
  // modulated by a lower-amplitude secondary count so the shafts bend and
  // shimmer instead of reading as a static asterisk
  var turn = angle / PI2
  var rays = wave(turn * rayCountA + wave(turn * rayCountB) / 3)
  var shaped = flare * mix(1, rays, rayStrength)
  shaped = shaped * shaped   // square to deepen contrast

  // hue: signed shimmer around the base, centered so typical flare
  // levels shift near zero
  var h = baseHue + (flare - 0.55) * colorVar
  // saturation climbs steeply with distance and drops with brightness:
  // white-hot at the sun and in ray cores, re-saturating outward
  var s = baseSat * saturate(d * d * 9 - shaped)

  hsv(h, s, shaped * baseVal)
}
