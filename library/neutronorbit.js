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

// Peak-hold one comet into its trail buffer over just the pixels its hump can
// reach -- `cwidth` of the strip, 10 % by default. The bounds are widened a
// pixel each way and hump()'s own `d >= cwidth / 2` guard keeps the edges
// exact; everywhere else hump() is 0 and max(v, 0) is v, so the release that
// already ran over the whole array is the whole answer.
function holdHump(buf, c) {
  var half = cwidth / 2
  var lo = floor((c - half) * pixelCount) - 1
  var hi = ceil((c + half) * pixelCount) + 1
  if (lo < 0) lo = 0
  if (hi > pixelCount - 1) hi = pixelCount - 1
  for (var i = lo; i <= hi; i++) {
    var v = hump(i / pixelCount, c)
    if (v > buf[i]) buf[i] = v
  }
}

export function beforeRender(delta) {
  tsec += delta / 1000
  if (tsec > 3600) tsec -= 3600

  var phase = tsec / period
  var pA = cometPos(phase)
  var pB = cometPos(phase + 1 / 3)
  var pC = cometPos(phase + 2 / 3)

  // peak-hold with exponential release, half-life ~0.1 s. The release is a
  // whole-array multiply, so it is one native `feedback` call rather than
  // pixelCount interpreted iterations, and the hold only walks each comet's
  // own window.
  var decay = pow(0.5, delta / trailMs)
  feedback(trailA, decay)
  feedback(trailB, decay)
  feedback(trailC, decay)
  holdHump(trailA, pA)
  holdHump(trailB, pB)
  holdHump(trailC, pC)

  // nucleus: pinned near midpoint with a tiny wobble at the comet period,
  // width breathing about twice a second, brightness throbbing fast + shallow
  nucPos = 0.5 + 0.02 * sin(PI2 * phase)
  nucHalf = nucSize * (1 + 0.15 * sin(PI2 * tsec / 0.5))
  nucBri = 0.8 + 0.1 * sin(PI2 * tsec / 0.6)   // swings 0.7 .. 0.9
}

// The read-out the old per-pixel `render` did, as one frame entry instead of
// pixelCount of them. Deliberately NOT `fillRGB`: the three trails are
// persistent state and the read-out is destructive (it maxes them together and
// squares), so a channel trio to fill would be three more `pixelCount` arrays
// -- and this pattern is already three, against a 10,236-element budget. That
// would drop its pixel ceiling from ~3,411 to ~1,705 for nothing: measured
// side by side, `clear()` + a `setPixel` loop over the LIT pixels only and
// `fillRGB` over three prebuilt channel buffers are the same speed here
// (1.98x vs 2.00x at 1024 px), because both skip the dark pixels and this
// pattern's frame is almost never more than a fifth lit.
//
// `clear`, `rgb` and `setPixel` are all index-space, so this is identical with
// a map and without one -- on a bare strip exactly the old `render(index)`
// loop, and on a matrix the atom still runs along the pixel index.
export function renderFrame() {
  clear()
  var i
  for (i = 0; i < pixelCount; i++) {
    var a = trailA[i]
    var b = trailB[i]
    var c = trailC[i]
    if (a <= 0 && b <= 0 && c <= 0) continue
    var r = max(a, b)
    if (c * 0.95 > r) r = c * 0.95
    var g = a * 0.5
    if (c * 0.35 > g) g = c * 0.35
    var bl = b * 0.55
    if (c * 0.35 > bl) bl = c * 0.35
    rgb(r * r, g * g, bl * bl)
    setPixel(i)
  }
  if (nucHalf > 0) {
    var nlo = floor((nucPos - nucHalf) * pixelCount) - 1
    var nhi = ceil((nucPos + nucHalf) * pixelCount) + 1
    if (nlo < 0) nlo = 0
    if (nhi > pixelCount - 1) nhi = pixelCount - 1
    for (i = nlo; i <= nhi; i++) {
      var dn = abs(i / pixelCount - nucPos)
      if (dn >= nucHalf) continue
      var n = (1 - dn / nucHalf) * nucBri
      var a2 = trailA[i]
      var b2 = trailB[i]
      var c2 = trailC[i]
      var r2 = max(max(a2, b2), max(c2 * 0.95, n))
      var g2 = max(max(a2 * 0.5, 0), max(c2 * 0.35, n))
      var bl2 = max(max(0, b2 * 0.55), max(c2 * 0.35, n))
      rgb(r2 * r2, g2 * g2, bl2 * bl2)
      setPixel(i)
    }
  }
}
