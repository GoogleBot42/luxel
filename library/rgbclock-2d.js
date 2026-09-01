// name: RGBclock 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGBclock 2D"; original source never consulted.
//
// Analog clock: red second / green minute / blue hour hands as additive
// angular gradients (overlaps blend to CMY), plus a white sub-second hand.
// Four radial modes x four hand modes; sharpness can "breathe" with an
// animation phase whose period the speed slider sets.

// control state (defaults match the //# bounds)
var radModeV = 0
var handModeV = 0
var sharpV = 0.6
var strengthV = 0
var breatheV = 0
var speedV = 0.3
var zoomV = 0
var subV = 0.25

// derived per frame
var radMode = 0
var handMode = 0
var sharpEff = 15.4
var strength = 1.05
var zoom = 1
var subBright = 0.0625
var animPhase = 0

// smooth hand angles (turns, 0 = twelve o'clock)
var angleW = 0
var angleS = 0
var angleM = 0
var angleH = 0

var lastSec = -1
var secFrac = 0

export function beforeRender(delta) {
  radMode = floor(radModeV * 3.99)
  handMode = floor(handModeV * 3.99)

  // jitter-compensated fractional second: accumulate delta, resync to
  // ~half a frame whenever the wall-clock second flips
  var sec = clockSecond()
  secFrac += delta / 1000
  if (sec != lastSec) {
    lastSec = sec
    secFrac = delta / 2000
  }
  // The accumulator is NOT clamped to one second: between clock ticks it
  // free-runs, so the sweep keeps turning even when the time source is
  // coarse, stalled or absent entirely (an unsynced device still shows a
  // moving clock instead of a frozen face).
  var fs = secFrac
  var smoothSec = sec + fs
  var smoothMin = clockMinute() + smoothSec / 60
  var smoothHour = clockHour() % 12 + smoothMin / 60

  angleW = fs                    // sub-second hand: one turn per second
  angleS = smoothSec / 60
  angleM = smoothMin / 60
  angleH = smoothHour / 12

  // animation phase: ~1 s period at the left, ~1 min at the right
  var period = 1000 + speedV * speedV * 59000
  animPhase += delta / period
  animPhase = mod(animPhase, 1)

  var baseSharp = 1 + sharpV * sharpV * 40          // squared response, up to a few dozen
  sharpEff = max(0.5, baseSharp * (1 + breatheV * (wave(animPhase) - 0.5)))
  strength = 1.05 + strengthV * strengthV           // up to ~doubling the base constant
  zoom = 1 + zoomV * 4
  subBright = subV * subV
}

// gradient for one hand: (strength - angular triangle distance), clamped,
// times the radial filter, raised to the effective sharpness power
function handVal(pixAng, hand, f, isSub) {
  var g = clamp(strength - triangle(pixAng - hand), 0, 1) * f
  if (handMode == 2) {
    // pulse: ripple term added before the power is applied
    g = clamp(g + triangle(pixAng - hand - animPhase) * 0.25 * f, 0, 1)
  }
  var v = pow(g, sharpEff)
  if (handMode == 1) v = v >= 0.5              // threshold: hard-edged hands
  if (handMode == 3) {
    if (isSub) {
      v = v >= 0.5                             // sub-second stays thresholded
    } else {
      // beam shot: multiply by a doubly-folded triangle so bright packets
      // shoot outward from each hand every cycle
      v = v * triangle(triangle(pixAng - hand - animPhase))
    }
  }
  return v
}

export function render2D(index, x, y) {
  var dx = x - 0.5
  var dy = y - 0.5
  // angle in turns, 0 at twelve o'clock, clockwise positive
  var ang = atan2(dx, -dy) / PI2
  if (ang < 0) ang += 1
  var r = sqrt(dx * dx + dy * dy) * 2 * zoom

  // radial filters per hand (sub-second, second, minute, hour)
  var fW = 1
  var fS = 1
  var fM = 1
  var fH = 1
  if (radMode == 0) {
    // equidistant rings, phase-offset per hand -> interleaved arcs;
    // over-driven then clamped so the crests are solid. One triangle period
    // spans the canvas, so each hand rides a SINGLE ring crest rather than
    // three (zoom multiplies r and packs in more).
    fW = min(1, triangle(r) * 1.5)
    fS = min(1, triangle(r + 0.25) * 1.5)
    fM = min(1, triangle(r + 0.5) * 1.5)
    fH = min(1, triangle(r + 0.75) * 1.5)
  } else if (radMode == 1) {
    // clustered rings at differing frequencies; hour strong at center,
    // sub-second confined to a narrow band near the center
    fW = clamp(1 - abs(r - 0.1) * 8, 0, 1)
    fS = min(1, triangle(r * 1.5) * 1.3)
    fM = min(1, triangle(r * 0.85) * 1.3)
    fH = clamp(1 - r * 2, 0, 1)
  } else if (radMode == 2) {
    // hard annuli: sub-second and hour inner, minute mid, second outer
    fW = r < 0.18
    fH = r >= 0.06 && r < 0.38
    fM = r >= 0.42 && r < 0.68
    fS = r >= 0.72 && r < 1
  }
  // radMode 3: rays — no radial shaping

  var w = clamp(handVal(ang, angleW, fW, 1), 0, 1) * subBright
  var rv = clamp(handVal(ang, angleS, fS, 0) + w, 0, 1)
  var gv = clamp(handVal(ang, angleM, fM, 0) + w, 0, 1)
  var bv = clamp(handVal(ang, angleH, fH, 0) + w, 0, 1)
  rgb(rv, gv, bv)
}

//# min=0 max=1 step=0.25 default=0
export function sliderRadiusMode(v) { radModeV = v }

//# min=0 max=1 step=0.25 default=0
export function sliderHandMode(v) { handModeV = v }

//# min=0 max=1 step=0.01 default=0.6
export function sliderSharpness(v) { sharpV = v }

//# min=0 max=1 step=0.01 default=0
export function sliderStrength(v) { strengthV = v }

//# min=0 max=1 step=0.01 default=0
export function sliderBreathe(v) { breatheV = v }

//# min=0 max=1 step=0.01 default=0.3
export function sliderSpeed(v) { speedV = v }

//# min=0 max=1 step=0.01 default=0
export function sliderDistance(v) { zoomV = v }

//# min=0 max=1 step=0.01 default=0.25
export function sliderSubSecondBrightness(v) { subV = v }
