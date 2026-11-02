// name: Butterfly
// Clean-room reimplementation from the prose spec docs/pattern-specs/butterfly-2d.md
// — reimagined, not a replica.
// Butterflies rise in from below, flutter harder and harder, and lift off the
// top; each is reborn with its own wing outline, markings, hue and tilt. The
// wing (petal-lobe polar boundary + markings) is baked once per butterfly into
// a texture in wing space; each frame looks it up mirrored and flap-scaled for
// the cells in the butterfly's box, then draws body and antennae as strokes.
// A panel that shows one at a time relays two: the next one rises in from below
// as the last lifts off, so the panel is never left empty.

var NB = 4, TEXMAX = 6144, GOLDEN = 0.61803399
var UMAX = 0.68, VMAX = 0.47, VSPAN = 0.9    // wing space baked into the texture

var flapPeak = 3
//# min=0.5 max=8 step=0.1 default=3
export function sliderFlapSpeed(v) { flapPeak = clamp(v, 0.5, 8) }

var cycleSec = 8
//# min=3 max=40 step=1 default=8
export function sliderCycleSeconds(v) { cycleSec = clamp(v, 3, 40) }

// The first butterfly's hue; companions sit golden-ratio steps away.
var hueBase = 28 / 360
//# min=0 max=359 step=1 default=28
export function sliderWingHue(v) { hueBase = clamp(v, 0, 359) / 360 }

var pat = 0.7
//# min=0 max=100 step=1 default=70
export function sliderPatternPercent(v) { pat = clamp(v, 0, 100) / 100 }

// How many fly at once on panels 48 px and wider; smaller panels show one.
var count = 2
//# min=1 max=4 step=1 default=2
export function sliderCount(v) { count = clamp(floor(v), 1, 4) }

var tex = array(TEXMAX)
var TW = 1, TH = 1, TU = 1, TV = 1, eff = 0, gw0 = -1, ns = 0
var life = array(NB), flapPh = array(NB), open = array(NB)
var hx = array(NB), hy = array(NB), cx = array(NB), cy = array(NB), drift = array(NB)
var tc = array(NB), ts = array(NB), hue = array(NB), sz = 1
// marking classes: base, pale margin, eyespot, dark margin, margin dot,
// dark root, hindwing tone, pale bar, pale spot
var clsDh = [0, 0, 0, GOLDEN - 1, 0, 0, 0, GOLDEN - 1, 0, 0]
var clsS = [1, 1, 0.3, 1, 1, 0.1, 1, 1, 0.25, 0.2]
var clsV = [0, 0.9, 1, 1, 0.14, 1, 0.65, 0.9, 1, 1]
var dhC = array(10), sC = array(10), vC = array(10)

function lobe(th, dir, amp, wLo, wHi) {
  var q = (th - dir) / (th > dir ? wHi : wLo)
  return amp * exp(-q * q)
}

// a new butterfly in slot k: shape, markings, colour, tilt, home
function reborn(k) {
  var fd = 0.55 + random(0.3), fa = 0.32 + random(0.08), fw = 0.45 + random(0.15)
  var hd = -0.8 - random(0.3), ha = 0.24 + random(0.08), hw = 0.6 + random(0.2)
  var fsx = cos(fd) * fa * 0.93 + 0.015, fsy = sin(fd) * fa * 0.6 + 0.03
  var hsx = cos(hd) * ha * 0.93 + 0.015, hsy = sin(hd) * ha * 0.6 + 0.03
  var st = floor(random(3)), base = k * TW * TH
  for (var tr = 0; tr < TH; tr++) {
    var vv = VMAX - (tr + 0.5) / TV, dv = vv - 0.03
    for (var tcol = 0; tcol < TW; tcol++) {
      var uu = (tcol + 0.5) / TU, ex = (uu - 0.015) / 1.55
      var th = atan2(dv, ex), rr = hypot(ex, dv)
      var b = max(lobe(th, fd, fa, 0.8, fw), lobe(th, hd, ha, hw, 0.75))
      b = max(b * (1 + 0.035 * sin(th * 11)), 0.09)
      var wa = smoothstep(-0.02, 0.012, b - rr), depth = (b - rr) / b
      var sx = vv > 0.03 ? uu - fsx : uu - hsx, sy = vv > 0.03 ? vv - fsy : vv - hsy
      var spot = sx * sx + sy * sy < 0.0018, cl = 1
      if (st == 0) {
        if (depth < 0.12) cl = 2
        if (spot) cl = 3
      } else if (st == 1) {
        if (depth < 0.17) cl = frac(th * 2.6) < 0.4 && depth > 0.04 ? 5 : 4
        else if (depth > 0.7) cl = 6
      } else {
        if (vv < 0.03) cl = 7
        else if (depth > 0.42 && depth < 0.6) cl = 8
        if (depth < 0.07) cl = 4
        if (spot) cl = 9
      }
      tex[base + tr * TW + tcol] = wa > 0.01 ? cl + wa * 0.99 : 0
    }
  }
  var tl = (random(1) - 0.5) * 0.52
  tc[k] = cos(tl); ts[k] = sin(tl)
  hue[k] = frac(hueBase + k * GOLDEN + (random(1) - 0.5) * 0.12)
  drift[k] = (random(1) - 0.5) * 0.6
  // each slot keeps its own patch of the panel, jittered per life
  var j = eff == 3 ? k + 4 : k + (eff == 4 ? 8 : 0)
  hx[k] = eff > 1 ? clamp(slotX[j] + random(0.14) - 0.07, sz * 0.45, 1 - sz * 0.45) : 0.5
  hy[k] = eff > 1 ? slotY[j] + random(0.1) - 0.05 : 0.5
}
var slotX = [0.3, 0.7, 0, 0, 0.26, 0.74, 0.45, 0, 0.27, 0.73, 0.27, 0.73]
var slotY = [0.33, 0.67, 0, 0, 0.3, 0.42, 0.73, 0, 0.28, 0.3, 0.72, 0.7]

// size the flock and its textures for this panel
function layout() {
  gw0 = gridWidth()
  eff = gw0 >= 48 ? count : 1
  sz = eff > 1 ? 0.82 - eff * 0.08 : 1
  ns = max(eff, 2)                  // slots: a lone butterfly gets a relay partner
  var gw = max(gw0, 16), gh = max(gridHeight(), 16)
  TW = ceil(UMAX * gw * sz); TH = ceil(VSPAN * gh * sz)
  while (TW * TH * ns > TEXMAX) { TW = floor(TW * 0.9); TH = floor(TH * 0.9) }
  TU = TW / UMAX; TV = TH / VSPAN
  // open with the first one already in view (k / eff + a little)
  for (var k = 0; k < ns; k++) { life[k] = k / eff + 0.06; reborn(k) }
  if (eff == 1) life[1] = -1          // parked until the first one takes off
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  if (gridWidth() != gw0 || (gw0 >= 48 ? count : 1) != eff) layout()
  for (var i = 0; i < 10; i++) {
    dhC[i] = clsDh[i] * pat; sC[i] = mix(1, clsS[i], pat); vC[i] = mix(0.9, clsV[i], pat)
  }
  vC[0] = 0
  for (var k = 0; k < ns; k++) {
    if (life[k] < 0) continue
    var p = life[k] + dt / cycleSec
    if (eff == 1) {
      // relay: the partner is born as this one starts its climb out
      if (p >= 0.9 && life[1 - k] < 0) { life[1 - k] = 0; reborn(1 - k) }
      if (p >= 1) { life[k] = -1; continue }
    } else if (p >= 1) { p -= 1; reborn(k) }
    life[k] = p
    // lazy at rest, frantic just before takeoff; snaps shut, dwells open
    flapPh[k] = frac(flapPh[k] + flapPeak * (0.25 + 0.75 * p * p) * dt)
    var w = 1 - wave(flapPh[k])
    open[k] = 0.62 + 0.38 * (1 - w * w)
    var a = smoothstep(0, 0.1, p), d = easeInQuad(smoothstep(0.86, 1, p))
    cy[k] = mix(1 + sz * 0.5, hy[k], a) + (-sz * 0.55 - hy[k]) * d + 0.012 * sz * sin(flapPh[k] * PI2)
    cx[k] = hx[k] + drift[k] * d * sz
  }
}

// wing-space point -> world, for the strokes
function wxp(k, u, v) { return cx[k] + sz * (u * tc[k] + v * ts[k]) }
function wyp(k, u, v) { return cy[k] + sz * (u * ts[k] - v * tc[k]) }

function wingAt(k, lx, v) {
  var ti = floor(abs(lx) / open[k] * TU)
  var tj = floor((VMAX - v / (1.14 - 0.14 * open[k])) * TV)
  if (ti >= TW || tj < 0 || tj >= TH) return 0
  var code = tex[k * TW * TH + tj * TW + ti]
  if (code <= 0) return 0
  var cl = floor(code)
  hsv(hue[k] + dhC[cl], sC[cl], vC[cl] * (code - cl))
  return 1
}

function strokes(k) {
  var w = sz / max(gridWidth(), 24)
  hsv(hue[k], 0.3, 0.28)
  drawLine(wxp(k, 0, -0.29), wyp(k, 0, -0.29), wxp(k, 0, 0.2), wyp(k, 0, 0.2), max(0.05 * sz, w * 1.6), 0)
  fillCircle(wxp(k, 0, 0.25), wyp(k, 0, 0.25), max(0.045 * sz, w))
  hsv(hue[k], 0.25, 0.6)
  for (var e = -1; e <= 1; e += 2) {
    drawLine(wxp(k, 0.025 * e, 0.28), wyp(k, 0.025 * e, 0.28), wxp(k, 0.15 * e, 0.43), wyp(k, 0.15 * e, 0.43), w * 1.3, 2)
  }
}

export function renderBulk() {
  clear()
  var gw = gridWidth(), gh = gridHeight(), k
  if (gw < 2 || gh < 2) {
    // no grid: every pixel against every butterfly
    for (var i = 0; i < pixelCount; i++) {
      var x = pixelCoord(i, 0), y = pixelCoord(i, 1)
      for (k = 0; k < ns; k++) {
        if (life[k] < 0) continue
        var dx = x - cx[k], dy = y - cy[k]
        if (wingAt(k, (dx * tc[k] + dy * ts[k]) / sz, (dx * ts[k] - dy * tc[k]) / sz)) { setPixel(i); break }
      }
    }
    for (k = 0; k < ns; k++) if (life[k] >= 0) strokes(k)
    return
  }
  var is = 1 / sz, sx = 1 / (gw - 1), sy = 1 / (gh - 1)
  for (k = ns - 1; k >= 0; k--) {
    if (life[k] < 0) continue
    var ac = abs(tc[k]), as = abs(ts[k]), a = UMAX * open[k]
    var ex = sz * (a * ac + 0.45 * as), ey = sz * (a * as + 0.45 * ac)
    var c0 = max(0, ceil((cx[k] - ex) / sx)), c1 = min(gw - 1, floor((cx[k] + ex) / sx))
    var r0 = max(0, ceil((cy[k] - ey) / sy)), r1 = min(gh - 1, floor((cy[k] + ey) / sy))
    var dl = tc[k] * is * sx, dvv = ts[k] * is * sx
    for (var r = r0; r <= r1; r++) {
      var ddx = c0 * sx - cx[k], ddy = r * sy - cy[k]
      var lx = (ddx * tc[k] + ddy * ts[k]) * is, v = (ddx * ts[k] - ddy * tc[k]) * is
      for (var c = c0; c <= c1; c++) {
        if (wingAt(k, lx, v)) setPixel(gridIndex(c, r))
        lx += dl
        v += dvv
      }
    }
    strokes(k)
  }
}
