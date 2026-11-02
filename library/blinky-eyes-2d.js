// name: Blinky Eyes
// tags: panel, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/blinky-eyes-2d.md
// — reimagined, not a replica.
// A pair of cartoon eyes sized to the panel: white sclera, a coloured iris
// with a dark limbal ring, pupil and specular glint. The gaze holds, then
// darts anywhere in the eye (up, down, sideways) with an occasional quick
// glance away and back; the upper lid rests a little low, follows the gaze
// down and sweeps shut to blink. All shapes are aspect-correct, anti-aliased
// ellipse and circle tests in render2D — no roots, no trig per pixel.

var blinkRate = 20
//# min=4 max=60 step=1 default=20
export function sliderBlinkRate(v) { blinkRate = clamp(v, 4, 60) }

var sizePct = 70
//# min=40 max=95 step=1 default=70
export function sliderEyeSize(v) { sizePct = clamp(v, 40, 95) }

var eyes = 2
//# min=1 max=2 step=1 default=2
export function sliderEyes(v) { eyes = clamp(floor(v), 1, 2) }

var sclera = 1
//# default=1
export function toggleWhiteSclera(on) { sclera = on }

// Iris hue and saturation; the picker's brightness is ignored.
var irisH = 0.58, irisS = 0.85
export function hsvPickerIrisColor(h, s, v) { irisH = h; irisS = s }

var asp = 1, eh = 0.35, ew = 0.25, ecx = 0, pu = 0.1
var gx = 0, gy = 0, fromX = 0, fromY = 0, toX = 0, toY = 0
var moveT = 1, holdT = 1, glance = 0
var lid = 0.12, blinkT = -1, nextBlink = 2, twice = 0
var icx = 0, icy = 0, ir = 0.1, ir2 = 0.01, gl2 = 0, gcx = 0, gcy = 0

function aim(x, y) { fromX = gx; fromY = gy; toX = x; toY = y; moveT = 0 }

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var gw = gridWidth(), gh = gridHeight()
  asp = gw > 0 && gh > 0 ? gw / gh : 1
  pu = 1 / max(gh, 8)                       // one pixel, in panel heights
  eh = sizePct / 200
  ew = eyes == 2 ? min(eh * 0.78, asp / 4 - 0.035) : min(eh * 0.85, asp * 0.45)
  ecx = eyes == 2 ? ew + 0.035 : 0

  // gaze: hold, then dart (a fast eased saccade); a glance returns by itself
  holdT -= dt
  if (holdT <= 0 && moveT >= 1) {
    if (glance) { aim(0, 0); glance = 0; holdT = 0.6 + random(1.5) }
    else if (random(1) < 0.2) { aim(random(1) < 0.5 ? -1 : 1, random(0.6) - 0.3); glance = 1; holdT = 0.35 }
    else if (random(1) < 0.3) { aim(0, 0); holdT = 0.8 + random(1.8) }
    else {
      var a = random(PI2), r = 0.4 + random(0.6)
      aim(cos(a) * r, sin(a) * r * 0.8)
      holdT = 0.5 + random(1.8)
    }
  }
  moveT = min(moveT + dt / 0.14, 1)
  var e = easeOutCubic(moveT)
  gx = mix(fromX, toX, e)
  gy = mix(fromY, toY, e)

  // lid: rests a little low, follows a downward look, sweeps shut to blink
  var rest = 0.12 + max(gy, 0) * 0.16 - max(-gy, 0) * 0.06
  nextBlink -= dt
  if (blinkT < 0 && nextBlink <= 0) {
    blinkT = 0
    twice = random(1) < 0.2
    nextBlink = (60 / blinkRate) * (0.6 + random(0.8))
  }
  var c = 0
  if (blinkT >= 0) {
    blinkT += dt
    c = blinkT < 0.07 ? easeInQuad(blinkT / 0.07) : blinkT < 0.11 ? 1 : 1 - easeOutQuad(min((blinkT - 0.11) / 0.16, 1))
    if (blinkT > 0.27) { blinkT = twice ? 0 : -1; twice = 0 }
  }
  lid = mix(rest, 1, c)

  // iris geometry, in panel heights relative to the eye centre
  ir = ew * 0.58
  ir2 = ir * ir
  icx = gx * (ew - ir * 0.8)
  icy = gy * (eh - ir * 0.9)
  gcx = icx - ir * 0.36
  gcy = icy - ir * 0.38
  gl2 = max(ir * 0.2, pu * 0.9)
  gl2 = gl2 * gl2
  // per-frame constants for render2D
  iew = 1 / ew
  ieh = 1 / eh
  iir2 = 1 / ir2
  rimK = 0.5 * min(ew, eh) / pu
  lidA = -1 + 2 * lid
  lidB = 0.7 * lid * (1 - lid)
  lidK = eh / pu
  scl = sclera ? 0.88 : 0
  rimV = sclera ? 0.45 : 0.85
}
var iew = 1, ieh = 1, iir2 = 1, rimK = 1, lidA = 0, lidB = 0, lidK = 1, scl = 0.88, rimV = 0.45

export function render2D(index, x, y) {
  var X = (x - 0.5) * asp, Y = y - 0.5
  var px = eyes == 2 ? (X < 0 ? X + ecx : X - ecx) : X
  var ux = px * iew, uy = Y * ieh
  var m2 = ux * ux + uy * uy
  if (m2 > 1.35) { rgb(0, 0, 0); return }
  // distance to the rim in pixels (1 - m ~ (1 - m2) / 2 near it)
  var rim = (1 - m2) * rimK
  var under = (uy - lidA - lidB * (1 - ux * ux)) * lidK   // pixels below the lid edge
  var v = 0.05, h = 0, s = 0                               // 0.05: the closed lid
  if (under > -0.5) {
    var dx = px - icx, dy = Y - icy, d2 = dx * dx + dy * dy
    if (d2 < ir2) {
      var q = d2 * iir2
      h = irisH
      s = irisS
      v = q < 0.2 ? 0.02 : 1 - 0.45 * q - 0.45 * clamp(q * 2.5 - 1.5, 0, 1)   // pupil, limbal ring
      dx = px - gcx
      dy = Y - gcy
      if (dx * dx + dy * dy < gl2) { s = 0; v = 1 }
    } else {
      h = 0.6
      s = 0.05
      v = scl * (0.72 + 0.28 * clamp(under * 0.33, 0, 1))   // shadow under the lid
    }
    if (under < 0.5) v *= under + 0.5
  }
  if (rim < 0.5) v *= clamp(rim + 0.5, 0, 1)
  // the rim and the lid's lash line on top
  var o = clamp(1.3 - abs(rim - 0.3), 0, 1) * rimV
  if (lid > 0.02 && rim > -0.5) o = max(o, clamp(1.2 - abs(under), 0, 1) * 0.9)
  if (o > v) { s = 0; v = o }
  hsv(h, s, v)
}
