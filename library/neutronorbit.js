// name: neutronorbit
// Clean-room reimplementation from a prose functional description of the
// community pattern "neutronorbit"; original source never consulted.

// An "atom" on a strip: three warm comets (orange, magenta-pink, coral) sweep
// back and forth sinusoidally with a shared period, staggered by a third of a
// cycle, each dragging a fast-fading tail. A white nucleus throbs at the
// center, breathing slightly in width and quivering around the midpoint.
// Channels combine by per-channel max, so crossings flash toward white.

var trailA = array(pixelCount)
var trailB = array(pixelCount)
var trailC = array(pixelCount)

var tsec = 0
var LO = 0.1           // travel range: one-tenth ...
var HI = 0.9           // ... to nine-tenths of the strip

// Tunables — the top-level values are exactly the constants the port shipped
// with, so an untouched pattern renders as before; the controls below just
// re-express them in real units.
var period = 6         // comet round trip, seconds
var cwidth = 0.1       // comet width, fraction of strip
var trailMs = 100      // trail half-life, milliseconds
var nucSize = 0.05     // nucleus half-width, fraction of strip

var nucPos = 0.5
var nucHalf = 0.05
var nucBri = 0.8

// Seconds for a comet to complete one full round trip along the strip.
//# min=1 max=20 step=0.5 default=6
export function sliderOrbitSeconds(v) { period = max(v, 0.5) }

// Comet width as a percentage of the strip.
//# min=2 max=40 step=1 default=10
export function sliderCometWidthPercent(v) { cwidth = clamp(v, 1, 60) / 100 }

// How long a comet tail takes to fade to half brightness, in milliseconds.
//# min=10 max=1000 step=10 default=100
export function sliderTrailFadeMs(v) { trailMs = max(v, 5) }

// Nucleus half-width as a percentage of the strip; 0 removes it entirely.
//# min=0 max=20 step=0.5 default=5
export function sliderNucleusSizePercent(v) { nucSize = clamp(v, 0, 40) / 100 }

// raised-cosine oscillation of a pulse center between LO and HI
function cometPos(phase) {
  return LO + (HI - LO) * (0.5 - 0.5 * cos(PI2 * phase))
}

// half-sine hump: zero at both edges, peak in the middle
function hump(p, c) {
  var d = abs(p - c)
  if (d >= cwidth / 2) return 0
  return sin(PI * (0.5 + d / cwidth))
}

export function beforeRender(delta) {
  tsec += delta / 1000
  if (tsec > 3600) tsec -= 3600

  var phase = tsec / period
  var pA = cometPos(phase)
  var pB = cometPos(phase + 1 / 3)
  var pC = cometPos(phase + 2 / 3)

  // peak-hold with exponential release, half-life ~0.1 s
  var decay = pow(0.5, delta / trailMs)
  var i
  for (i = 0; i < pixelCount; i++) {
    var p = i / pixelCount
    trailA[i] = max(trailA[i] * decay, hump(p, pA))
    trailB[i] = max(trailB[i] * decay, hump(p, pB))
    trailC[i] = max(trailC[i] * decay, hump(p, pC))
  }

  // nucleus: pinned near midpoint with a tiny wobble at the comet period,
  // width breathing about twice a second, brightness throbbing fast + shallow
  nucPos = 0.5 + 0.02 * sin(PI2 * phase)
  nucHalf = nucSize * (1 + 0.15 * sin(PI2 * tsec / 0.5))
  nucBri = 0.8 + 0.1 * sin(PI2 * tsec / 0.6)   // swings 0.7 .. 0.9
}

export function render(index) {
  var a = trailA[index]
  var b = trailB[index]
  var c = trailC[index]

  // nucleus: triangle profile (linear ramp up then down), no trail
  var d = abs(index / pixelCount - nucPos)
  var n = d < nucHalf ? (1 - d / nucHalf) * nucBri : 0

  // per-channel max across the four tinted layers
  var r = max(max(a,        b),        max(c * 0.95, n))   // all comets red-heavy
  var g = max(max(a * 0.5,  0),        max(c * 0.35, n))   // orange + coral only
  var bl = max(max(0,       b * 0.55), max(c * 0.35, n))   // pink + coral only

  rgb(r * r, g * g, bl * bl)   // gamma-style shaping
}
