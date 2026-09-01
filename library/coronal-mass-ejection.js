// name: Coronal Mass Ejection
// Clean-room reimplementation from a prose functional description of the
// community pattern "Coronal Mass Ejection"; original source never consulted.
//
// FORK: deliberately departs from the original per the 2026-09-01 review.
// The original exposes no dials at all and this port had drifted into placid
// concentric rings; both are fixed here. The polar-noise architecture stays --
// turbulence sampled in (angle, radius) space, streamed outward, one max()
// per pixel -- but the angle axis now carries several noise periods so the
// field breaks into RAYS instead of rings, a small event system throws real
// eruptions off the limb, and the color ramps white -> gold -> ember red with
// distance instead of flooding the panel with one flat hue.
//
// A white-hot star at the panel center throws ragged plasma tongues outward
// over a mostly black field. Every so often the limb lets go and a bright
// prominence tears away, fanning out as it climbs past the frame edge.
//
// Sibling of "Coronal Ejection 2D", which drives the same engine as a
// stylized, mirrorable flower; this one is the naturalistic star.

var LOBES = 6                     // noise periods around the circle -> tongues
var TWIST = 0.8                   // shear that leans the tongues as they rise
var RSCALE = 0.9                  // radial detail: LOW, so features stretch into
                                  // rays instead of closing into rings
var MAXERUPT = 4                  // simultaneous prominences

// ---- controls (real units) -------------------------------------------------

var eruptRate = 14                // prominences per minute
var intensity = 50                // how much of the corona lights up, percent
var baseHue = 18                  // plasma hue, degrees
var hueCycle = 0                  // seconds per full hue revolution, 0 = fixed
var speed = 1                     // churn rate, x realtime
var coreSize = 18                 // core radius, percent of the half-width

//# min=0 max=60 step=1 default=14
export function sliderEruptions(v) { eruptRate = max(0, v) }

//# min=0 max=100 step=1 default=50
export function sliderIntensity(v) { intensity = clamp(v, 0, 100) }

//# min=0 max=360 step=1 default=18
export function sliderHue(v) { baseHue = v }

//# min=0 max=60 step=1 default=0
export function sliderHueCycleSeconds(v) { hueCycle = max(0, v) }

//# min=0.1 max=4 step=0.1 default=1
export function sliderSpeed(v) { speed = max(0.02, v) }

//# min=2 max=60 step=1 default=18
export function sliderCoreSize(v) { coreSize = clamp(v, 1, 80) }

// Let one go on demand.
export function triggerErupt() { spawnErupt() }

// ---- state -----------------------------------------------------------------

var eAng = array(MAXERUPT)        // launch angle, turns
var eRad = array(MAXERUPT)        // current height, normalized radius
var eVel = array(MAXERUPT)        // climb rate, radii per second
var eWid = array(MAXERUPT)        // angular half-width, turns
var eLife = array(MAXERUPT)       // 1 -> 0
var eDecay = array(MAXERUPT)      // life lost per second

var eruptTimer = 0.6
var churn = 0                     // radial stream phase
var shape = 0                     // shape-evolution phase
var huePhase = 0
var wrapped = 0

var hueTurns = 0.05               // baseHue in turns, recomputed per frame
var cut = 0.55                    // flare threshold, from intensity
var coreR = 0.18                  // core radius in normalized units

function spawnErupt() {
  var s = -1
  for (var i = 0; i < MAXERUPT; i++) if (eLife[i] <= 0) { s = i; break }
  if (s < 0) return 0
  eAng[s] = random(1)
  eRad[s] = coreR + 0.02
  eVel[s] = 0.30 + random(0.30)
  eWid[s] = 0.06 + random(0.06)
  eLife[s] = 1
  eDecay[s] = 1 / (1.4 + random(1.2))
  return 1
}

// ---- frame -----------------------------------------------------------------

export function beforeRender(delta) {
  var dt = min(delta, 60) * 0.001
  var st = dt * speed

  if (!wrapped) {
    // the angle axis is scaled to LOBES periods and wrapped at LOBES, so the
    // turbulence tiles seamlessly around the circle while still carrying
    // several tongues; the other two axes never repeat
    setPerlinWrap(LOBES, 0, 0)
    wrapped = 1
  }

  churn += st * 0.9               // tongues stream outward
  if (churn > 4096) churn -= 4096
  shape += st * 0.5               // and reshape as they go
  if (shape > 4096) shape -= 4096

  if (hueCycle > 0) huePhase = mod(huePhase + dt / hueCycle, 1)
  hueTurns = baseHue / 360 + huePhase

  // Intensity is coverage: at 0 only the ridge tips survive, at 100 the whole
  // corona lights and the dark lanes between tongues become the negative space.
  cut = 0.95 - intensity * 0.0055
  coreR = coreSize * 0.01

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

  // TWIST shears the angle with height, so a tongue leans as it rises
  var n = 1 - perlinTurbulence(a * LOBES + r * TWIST, r * RSCALE - churn, shape, 2, 0.5, 3)

  // ridges only, over a falloff gentle enough that a strong tongue still
  // reaches the frame corner
  var flare = smoothstep(cut, cut + 0.22, n) * saturate(1.3 - r * 0.5)

  // core: a solid white-hot disc whose edge is chewed by the same noise, so it
  // bulges outward wherever a tongue is rooted
  var ce = coreR * (0.55 + n * 0.9)
  var core = saturate(1 + (ce - r) / (0.35 * ce))

  var v = max(flare, core)

  // prominences: a radially stretched blob per event, roughened by the same
  // noise field so it tears rather than glows
  for (var i = 0; i < MAXERUPT; i++) {
    var lf = eLife[i]
    if (lf <= 0) continue
    var da = abs(a - eAng[i])
    if (da > 0.5) da = 1 - da
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

  // blackbody ramp: white core, gold shoulders, ember red at the cold tips --
  // the hue rides the brightness instead of flooding the panel with one tone
  var hue = hueTurns + (v - 0.3) * 0.13
  var sat = min(saturate(r * 2.8), saturate(1.18 - v * 0.8))
  hsv(hue, sat, v)
}
