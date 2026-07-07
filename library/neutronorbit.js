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
var PERIOD = 6         // comet round trip, seconds
var CWIDTH = 0.1       // comet width, fraction of strip
var LO = 0.1           // travel range: one-tenth ...
var HI = 0.9           // ... to nine-tenths of the strip

var nucPos = 0.5
var nucHalf = 0.05
var nucBri = 0.8

// raised-cosine oscillation of a pulse center between LO and HI
function cometPos(phase) {
  return LO + (HI - LO) * (0.5 - 0.5 * cos(PI2 * phase))
}

// half-sine hump: zero at both edges, peak in the middle
function hump(p, c) {
  var d = abs(p - c)
  if (d >= CWIDTH / 2) return 0
  return sin(PI * (0.5 + d / CWIDTH))
}

export function beforeRender(delta) {
  tsec += delta / 1000
  if (tsec > 3600) tsec -= 3600

  var phase = tsec / PERIOD
  var pA = cometPos(phase)
  var pB = cometPos(phase + 1 / 3)
  var pC = cometPos(phase + 2 / 3)

  // peak-hold with exponential release, half-life ~0.1 s
  var decay = pow(0.5, delta / 100)
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
  nucHalf = 0.05 * (1 + 0.15 * sin(PI2 * tsec / 0.5))
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
