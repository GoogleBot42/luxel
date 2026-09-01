// name: Coronal Ejection 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "Coronal Mass Ejection 2D" (no source consulted).
//
// FORK: deliberately departs from the original per the 2026-09-01 review.
// The port had settled into a slow "brain coral" tangle penned into the inner
// half of the grid, its Mirror toggle only doubled the lobe count, and there
// was nothing dramatic about it. Rebuilt around the same polar-noise idea but
// with the radial noise frequency dropped so features stretch into rays that
// reach the frame corners, a real angular fold for Mirror, an eruption event
// system that throws prominences clear off the disc, and a blackbody color
// ramp so the figure is white at the core and ember-red at the tips.
//
// The stylized sibling of "Coronal Mass Ejection": same engine, but the arm
// count is quantized, the arms are sheared into a pinwheel, and the whole
// figure can be folded into a symmetric flower.

var TWIST = 1.5                   // shear that curls the arms into a pinwheel
var RSCALE = 0.9                  // radial detail: LOW, so features stretch into
                                  // rays instead of closing into rings
var CORE = 0.16                   // core radius, fraction of the half-width
var MAXERUPT = 4                  // simultaneous prominences

// ---- controls (real units) -------------------------------------------------

var density = 6                   // arms around the disc
var detail = 3                    // turbulence octaves
var cutoff = 0.6                  // flare threshold, 0 floods / 1 leaves the core
var eruptRate = 18                // prominences per minute
var speed = 1                     // churn rate, x realtime
var baseHue = 0                   // plasma hue, degrees
var hueCycle = 20                 // seconds per full hue revolution, 0 = fixed
var mirror = 0

//# min=1 max=12 step=1 default=6
export function sliderDensity(v) { density = clamp(floor(v), 1, 12) }
export function showNumberDensity() { return density }

//# min=1 max=4 step=1 default=3
export function sliderDetail(v) { detail = clamp(floor(v), 1, 4) }
export function showNumberDetail() { return detail }

//# min=0 max=1 step=0.01 default=0.6
export function sliderCutoff(v) { cutoff = clamp(v, 0, 1) }

//# min=0 max=60 step=1 default=18
export function sliderEruptions(v) { eruptRate = max(0, v) }

//# min=0.1 max=4 step=0.1 default=1
export function sliderSpeed(v) { speed = max(0.02, v) }

//# min=0 max=360 step=1 default=0
export function sliderHue(v) { baseHue = v }

//# min=0 max=60 step=1 default=20
export function sliderHueCycleSeconds(v) { hueCycle = max(0, v) }

// Fold the disc about the vertical axis: the arms and the prominences both
// come out as a mirror-symmetric flower.
export function toggleMirror(v) { mirror = v }

// Let one go on demand.
export function triggerErupt() { spawnErupt() }

// ---- state -----------------------------------------------------------------

var eAng = array(MAXERUPT)        // launch angle, turns (folded space if mirrored)
var eRad = array(MAXERUPT)        // current height, normalized radius
var eVel = array(MAXERUPT)        // climb rate, radii per second
var eWid = array(MAXERUPT)        // angular half-width, turns
var eLife = array(MAXERUPT)       // 1 -> 0
var eDecay = array(MAXERUPT)      // life lost per second

var eruptTimer = 0.5
var churn = 0                     // radial stream phase
var shape = 0                     // shape-evolution phase
var huePhase = 0

var lobes = 6                     // = density, resolved per frame
var cut = 0.55                    // flare threshold, from cutoff
var hueTurns = 0

function spawnErupt() {
  var s = -1
  for (var i = 0; i < MAXERUPT; i++) if (eLife[i] <= 0) { s = i; break }
  if (s < 0) return 0
  eAng[s] = random(1)
  eRad[s] = CORE + 0.02
  eVel[s] = 0.3 + random(0.3)
  eWid[s] = 0.05 + random(0.06)
  eLife[s] = 1
  eDecay[s] = 1 / (1.4 + random(1.2))
  return 1
}

// ---- frame -----------------------------------------------------------------

export function beforeRender(delta) {
  var dt = min(delta, 60) * 0.001
  var st = dt * speed

  churn += st * 0.9               // arms stream outward
  if (churn > 4096) churn -= 4096
  shape += st * 0.5               // and reshape as they go
  if (shape > 4096) shape -= 4096

  if (hueCycle > 0) huePhase = mod(huePhase + dt / hueCycle, 1)
  hueTurns = baseHue / 360 + huePhase

  // Cutoff spans the whole useful range: at 0 the field floods and the dark
  // lanes between arms become the negative space, at 1 only the core survives.
  cut = 0.28 + cutoff * 0.68

  // the angle axis is scaled to `lobes` noise periods and wrapped at `lobes`,
  // so the turbulence tiles seamlessly around the circle while still carrying
  // one arm per period
  lobes = density
  setPerlinWrap(lobes, 0, 0)

  if (eruptRate > 0) {
    eruptTimer -= dt
    if (eruptTimer <= 0) {
      spawnErupt()
      eruptTimer = (60 / eruptRate) * (0.55 + random(0.9))
    }
  } else {
    eruptTimer = 1
  }

  for (var i = 0; i < MAXERUPT; i++) {
    if (eLife[i] <= 0) continue
    eRad[i] += eVel[i] * st
    eWid[i] += 0.05 * st          // the plume fans out as it climbs
    eLife[i] -= eDecay[i] * st
    if (eRad[i] > 1.6) eLife[i] = 0
  }
}

export function render2D(index, x, y) {
  var dx = x - 0.5
  var dy = y - 0.5
  var r = hypot(dx, dy) * 2       // 1 at the mid-edge, ~1.41 in the corners
  var a = atan2(dy, dx) / PI2 + 0.5

  // Mirror folds the turn about the vertical axis. The fold is continuous and
  // still spans exactly one wrap period, so the noise stays seamless.
  var ang = mirror ? abs(a - 0.5) * 2 : a

  var n = 1 - perlinTurbulence(ang * lobes + r * TWIST, r * RSCALE - churn, shape, 2, 0.5, detail)

  // ridges only, over a falloff gentle enough that a strong arm still reaches
  // the frame corner
  var flare = smoothstep(cut, cut + 0.22, n) * saturate(1.3 - r * 0.5)

  // core: a solid white-hot disc whose edge is chewed by the same noise, so it
  // bulges outward wherever an arm is rooted
  var ce = CORE * (0.55 + n * 0.9)
  var core = saturate(1 + (ce - r) / (0.35 * ce))

  var v = max(flare, core)

  // prominences: a radially stretched blob per event, roughened by the same
  // noise field so it tears rather than glows
  for (var i = 0; i < MAXERUPT; i++) {
    var lf = eLife[i]
    if (lf <= 0) continue
    var da = abs(ang - eAng[i])
    if (!mirror && da > 0.5) da = 1 - da
    var u = da / eWid[i]
    if (u > 1) continue
    var dr = r - eRad[i]
    var w = dr < 0 ? dr * 2.2 : dr * 5      // long tail below, sharp head above
    var q = u * u + w * w
    if (q >= 1) continue
    var e = (1 - q) * (1 - q) * lf * (0.28 + n * 1.5) * 1.8
    if (e > v) v = e
  }

  v = v * v                       // deepen contrast: the field goes properly black

  // blackbody ramp: white core, bright shoulders, deep tips -- the hue rides
  // the brightness instead of flooding the panel with one flat tone
  var hue = hueTurns + (v - 0.3) * 0.13
  var sat = min(saturate(r * 2.8), saturate(1.18 - v * 0.8))
  hsv(hue, sat, v)
}
