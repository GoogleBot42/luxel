// name: neutronorbit
// Clean-room reimplementation from a prose functional description of the
// community pattern "neutronorbit"; original source never consulted.

// An "atom": three warm comets (orange, magenta-pink, coral) sweep back and
// forth sinusoidally with short fading tails, staggered a third of a cycle
// apart, while a white nucleus throbs and breathes at the strip's center.
// Channels combine by per-layer max, so crossings flash toward white.

var PERIOD = 6         // comet round trip, seconds
var COMET_W = 0.05     // comet half-width (fraction of strip)
var TRAIL_HL = 0.1     // trail half-life, seconds

var t = 0
var trailA = array(pixelCount)
var trailB = array(pixelCount)
var trailC = array(pixelCount)

var posA = 0
var posB = 0
var posC = 0
var nPos = 0.5
var nWidth = 0.05
var nBri = 0.8

export function beforeRender(delta) {
  var dt = delta / 1000
  t += dt
  if (t > 3600) t -= 3600

  // raised-cosine sweep between ~0.1 and ~0.9, one-third-cycle stagger
  var ph = t / PERIOD
  posA = 0.5 - 0.4 * cos(ph * PI2)
  posB = 0.5 - 0.4 * cos((ph + 1 / 3) * PI2)
  posC = 0.5 - 0.4 * cos((ph + 2 / 3) * PI2)

  // peak-hold with exponential release draws the tails
  var decay = pow(0.5, dt / TRAIL_HL)
  for (var i = 0; i < pixelCount; i++) {
    var p = i / pixelCount
    trailA[i] = max(trailA[i] * decay, hump(p, posA))
    trailB[i] = max(trailB[i] * decay, hump(p, posB))
    trailC[i] = max(trailC[i] * decay, hump(p, posC))
  }

  // nucleus: pinned mid-strip with a tiny slow wobble, fast shallow throb,
  // and a subtle ~twice-per-second width breathing
  nPos = 0.5 + 0.02 * sin(t * PI2 / PERIOD)
  nWidth = 0.05 * (1 + 0.2 * sin(t * PI2 / 0.5))
  nBri = 0.8 + 0.1 * sin(t * PI2 / 0.7)
}

// half-sine pulse profile: zero at both edges, peak in the middle
function hump(p, center) {
  var d = abs(p - center)
  if (d >= COMET_W) return 0
  return sin((1 - d / COMET_W) * PI / 2)
}

export function render(index) {
  var p = index / pixelCount
  var a = trailA[index]
  var b = trailB[index]
  var c = trailC[index]

  // nucleus: triangle profile, no trail
  var nd = abs(p - nPos)
  var nuc = nd < nWidth ? (1 - nd / nWidth) * nBri : 0

  // per-channel max of the tinted layers
  // A orange: strong red, moderate green, no blue
  // B magenta: strong red, no green, medium blue
  // C coral:  strong red, light equal green + blue
  // nucleus:  white
  var r = max(max(a, b), max(c, nuc))
  var g = max(max(a * 0.55, 0), max(c * 0.35, nuc))
  var bl = max(max(0, b * 0.5), max(c * 0.35, nuc))

  rgb(r * r, g * g, bl * bl)   // gamma-style squaring
}
