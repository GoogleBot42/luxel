// name: RGBclock 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGBclock 2D"; original source never consulted.

// Analog clock face: red second, green minute, blue hour hands plus a white
// sub-second hand, drawn as angular gradients raised to a variable sharpness
// power. All blending is additive RGB, so overlaps make secondary colors.
// Two mode selectors: a radial filter (rings/bands/rays) and a hand mode
// (gradient/threshold/pulse/beam-shot).

// --- controls (globals hold the defaults if a slider is never moved) ---
var radialMode = 3   // rays
var handMode = 0     // gradient
var sharpBase = 8
var strengthK = 1.1
var breathe = 0.3
var periodMs = 4000
var zoom = 1
var subBright = 0.5

export function sliderRadiusMode(v) {
  //# min=0 max=1 step=0.34 default=1
  radialMode = floor(v * 3.999)
}
export function sliderHandMode(v) {
  //# min=0 max=1 step=0.34 default=0
  handMode = floor(v * 3.999)
}
export function sliderSharpness(v) {
  //# min=0 max=1 step=0.01 default=0.45
  sharpBase = 1 + v * v * 40 // squared response, up to a few dozen
}
export function sliderStrength(v) {
  //# min=0 max=1 step=0.01 default=0.3
  strengthK = 1.05 + v * v // squared, up to ~double the base constant
}
export function sliderBreathe(v) {
  //# min=0 max=1 step=0.01 default=0.3
  breathe = v // leftmost disables the sharpness oscillation
}
export function sliderSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.05
  periodMs = 1000 + v * 59000 // ~1 s at left, ~1 min at right
}
export function sliderDistance(v) {
  //# min=0 max=1 step=0.01 default=0
  zoom = 1 + v * 4
}
export function sliderSubSecondBrightness(v) {
  //# min=0 max=1 step=0.01 default=0.7
  subBright = v * v
}

// --- smooth time-of-day ---
var lastSec = -1
var secFrac = 0
var phase = 0
var angW = 0, angS = 0, angM = 0, angH = 0
var sharp = 8

export function beforeRender(delta) {
  var s = clockSecond()
  secFrac += delta / 1000
  if (s != lastSec) {
    lastSec = s
    secFrac = delta / 2000 // jitter compensation: restart at ~half a frame
  }
  if (secFrac > 4) secFrac = frac(secFrac) // keep bounded with no time source

  var smoothSec = s + min(secFrac, 1)
  var smoothMin = clockMinute() + smoothSec / 60
  var smoothHour = clockHour() % 12 + smoothMin / 60

  angW = frac(secFrac)      // sub-second hand: one turn per second
  angS = smoothSec / 60
  angM = smoothMin / 60
  angH = smoothHour / 12

  // looping animation phase (breathing + animated hand modes)
  phase += delta / periodMs
  phase = frac(phase)

  // effective sharpness breathes around the base
  sharp = sharpBase + sin(phase * PI2) * breathe * sharpBase * 0.8
  if (sharp < 0.5) sharp = 0.5
}

// angular triangle-distance: 0 on the hand, 1 opposite it
function angDist(a, hand) {
  return abs(frac(a - hand + 1.5) - 0.5) * 2
}

export function render2D(index, x, y) {
  var dx = x - 0.5
  var dy = y - 0.5
  var r = hypot(dx, dy) * 2 * zoom // ~0..1 to the edge, more at corners
  // angle in turns, zero at twelve o'clock, clockwise
  var a = atan2(dx, -dy) / PI2
  if (a < 0) a += 1

  // --- radial filters per hand: sub-second (w), second, minute, hour ---
  var fw, fs, fm, fh
  if (radialMode == 0) {
    // equidistant rings: same spacing, phase-offset per hand, overdriven
    fw = min(1, triangle(r * 2) * 1.5)
    fs = min(1, triangle(r * 2 + 0.25) * 1.5)
    fm = min(1, triangle(r * 2 + 0.5) * 1.5)
    fh = min(1, triangle(r * 2 + 0.75) * 1.5)
  } else if (radialMode == 1) {
    // clustered rings: different frequency per hand; hour hugs the center,
    // sub-second confined to a narrow inner band
    fw = clamp(1 - r * 6, 0, 1)
    fs = min(1, triangle(r * 3) * 1.5)
    fm = min(1, triangle(r * 2 + 0.3) * 1.5)
    fh = clamp(1 - r * 2, 0, 1)
  } else if (radialMode == 2) {
    // hard concentric bands, inner to outer: sub-second, hour, minute, second
    fw = r < 0.12
    fh = r >= 0.12 && r < 0.35
    fm = r >= 0.35 && r < 0.6
    fs = r >= 0.6 && r < 0.9
  } else {
    // rays: no radial shaping
    fw = 1; fs = 1; fm = 1; fh = 1
  }

  var dw = angDist(a, angW)
  var ds = angDist(a, angS)
  var dm = angDist(a, angM)
  var dh = angDist(a, angH)

  // base gradients: (strength - triangle distance) * radial filter,
  // clamped, then raised to the sharpness power
  var gw = pow(clamp((strengthK - dw) * fw, 0, 1), sharp)
  var gs = pow(clamp((strengthK - ds) * fs, 0, 1), sharp)
  var gm = pow(clamp((strengthK - dm) * fm, 0, 1), sharp)
  var gh = pow(clamp((strengthK - dh) * fh, 0, 1), sharp)

  if (handMode == 1) {
    // threshold: hard-edged hands
    gw = gw > 0.5
    gs = gs > 0.5
    gm = gm > 0.5
    gh = gh > 0.5
  } else if (handMode == 2) {
    // pulse: ripples of brightness roll outward along each hand
    gw = clamp(gw + (triangle(dw - phase) - 0.5) * 0.35 * fw, 0, 1)
    gs = clamp(gs + (triangle(ds - phase) - 0.5) * 0.35 * fs, 0, 1)
    gm = clamp(gm + (triangle(dm - phase) - 0.5) * 0.35 * fm, 0, 1)
    gh = clamp(gh + (triangle(dh - phase) - 0.5) * 0.35 * fh, 0, 1)
  } else if (handMode == 3) {
    // beam shot: discrete packets fly outward; sub-second stays thresholded
    gw = gw > 0.5
    gs *= triangle(triangle(ds - phase))
    gm *= triangle(triangle(dm - phase))
    gh *= triangle(triangle(dh - phase))
  }

  // additive composite: white sub-second on top of pure R/G/B hands
  var wv = clamp(gw, 0, 1) * subBright
  rgb(clamp(gs + wv, 0, 1), clamp(gm + wv, 0, 1), clamp(gh + wv, 0, 1))
}
