// name: GlowFlow
// Clean-room reimplementation from the prose specs docs/pattern-specs/glowflow-3d-coord-transform-api-port.md, upward-waves-3d-using-accelerometer.md — reimagined, not a replica.
// A tank of glowing rainbow liquid whose surface stays level with real
// gravity (accelerometer, smoothed); with no sensor it sloshes gently on
// its own. Bubbles rise and pop at the surface — sound makes it fizz. Waves
// mode: rainbow bands climbing against gravity. On a panel each column (or
// row, when it is on its side) is one blit of a depth strip offset to the
// surface line; a strip is a dozen or so gradientRange runs between the depth
// profile's breakpoints; 3D maps evaluate it per pixel.

export var accelerometer = array(3)  // panel: x = right, y = down (remap per fixture)
export var energyAverage = 0

var L = 128                          // depth strip half-length, cells
var cH = array(256), cS = array(256), cV = array(256)
var NB = 32
var bx = array(NB), by = array(NB), bv = array(NB), bOn = array(NB)
var dx = 0, dy = 1, dz = 0           // smoothed "down"
var t = 0, slosh = 0, prevA = 0, fizzAcc = 0, avgE = 0.05, hue0 = 0
var s0 = 0, wAmp = 0.02, st = 0

var mode = 0
// 0 liquid · 1 waves (bands rising against gravity)
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

var tau = 0.3
// Time constant of the tilt filter, in seconds.
//# min=0.05 max=2 step=0.05 default=0.3
export function sliderTiltSmoothingS(v) { tau = clamp(v, 0.05, 2) }

var fillPct = 60
//# min=10 max=90 step=1 default=60
export function sliderFillPct(v) { fillPct = clamp(v, 10, 90) }

var soundOn = 1
export function toggleSoundFizz(v) { soundOn = v }

// A cube stood on a corner: with no sensor, "down" is its long diagonal.
var corner = 0
export function toggleCornerStand(v) { corner = v }

export function render(index) { hsv(0, 0, 0) }

// colour of a point o units below the surface (negative = in the air)
var oH = 0, oS = 0, oV = 0
function shadeAt(o) {
  if (mode == 1) {
    var ph = o * 5 - t * 0.9
    var f = ph - floor(ph)
    var b = f > 0.6 ? (1 - f) / 0.4 : f / 0.6
    b = b * b
    oH = hue0 + floor(ph) * 0.13
    oS = 1 - b * 0.3
    oV = 0.07 + b * (0.85 + slosh)
    return
  }
  if (o >= 0) {
    oH = hue0 + min(o * 1.4, 0.8)
    oS = o < 0.025 ? 0.35 : 0.92
    oV = 1 - min(o, 1) * 0.35
  } else {
    var g = max(1 + o * 2.5, 0)
    oH = 0.04 + hue0 * 0.2
    oS = 0.85
    oV = 0.08 + g * g * g * 0.75
  }
}

function fillStrip(step, n) {
  for (var k = 0; k < n * 2; k++) {
    shadeAt((k - n) * step)
    cH[k] = oH
    cS[k] = oS
    cV[k] = oV
  }
}

function spawn() {
  for (var i = 0; i < NB; i++) if (!bOn[i]) {
    // somewhere in the liquid, low down: start one panel-width deep
    var u = random(1) - 0.5
    bx[i] = 0.5 + u * -dy * 0.95 + dx * 0.45
    by[i] = 0.5 + u * dx * 0.95 + dy * 0.45
    bv[i] = 0.12 + random(0.12)
    bOn[i] = 1
    return
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t += dt
  if (t > 3600) t -= 3600
  hue0 = mod(hue0 + dt * 0.01, 1)
  var ax = accelerometer[0], ay = accelerometer[1], az = accelerometer[2]
  var tx, ty, tz
  if (has3DMap()) {
    var m = hypot3(ax, ay, az)
    if (m > 0.05) { tx = ax / m; ty = ay / m; tz = az / m }
    else if (corner) { tx = -0.577; ty = -0.577; tz = -0.577 }
    else { tx = 0.15 * sin(t * 0.7); ty = 0; tz = -1 }
  } else {
    var m2 = hypot(ax, ay)
    if (m2 > 0.05) { tx = ax / m2; ty = ay / m2 }
    else {
      var a = 0.16 * sin(t * 0.63) + 0.05 * sin(t * 1.7)
      tx = sin(a)
      ty = cos(a)
    }
    tz = 0
  }
  var k = 1 - exp(-dt / tau)
  dx += (tx - dx) * k
  dy += (ty - dy) * k
  dz += (tz - dz) * k
  var n = max(hypot3(dx, dy, dz), 0.001)
  dx /= n
  dy /= n
  dz /= n
  // sloshing: tilt changes kick the surface waves, which settle in ~2 s
  var ang = atan2(dx, dy)
  slosh = min(slosh * exp(-dt / 2) + abs(ang - prevA) * 0.6, 0.6)
  prevA = ang
  wAmp = 0.012 + slosh * 0.06
  s0 = 0.5 - fillPct / 100
  st += dt * (1.6 + slosh * 3)
  // bubbles: a gentle base rate, more with sound (auto-gained)
  var e = soundOn ? energyAverage : 0
  avgE += (e - avgE) * min(dt * 0.3, 1)
  var rate = 2.5 + clamp(e / max(avgE * 2, 0.03), 0, 1.5) * 30
  if (mode == 1) rate = 0
  fizzAcc += rate * dt
  while (fizzAcc >= 1) { spawn(); fizzAcc -= 1 }
  for (var i = 0; i < NB; i++) if (bOn[i]) {
    var w = sin((t + i) * 5) * 0.004
    bx[i] -= dx * bv[i] * dt - dy * w
    by[i] -= dy * bv[i] * dt + dx * w
    if ((bx[i] - 0.5) * dx + (by[i] - 0.5) * dy < s0 + 0.01) bOn[i] = 0
    if (bx[i] < -0.1 || bx[i] > 1.1 || by[i] < -0.1 || by[i] > 1.1) bOn[i] = 0
  }
}

function surf(u) { return wAmp * (0.6 * sin(u * 9 + st) + 0.4 * sin(u * 17 - st * 1.3)) }

function panel(gw, gh) {
  var cols = abs(dy) >= abs(dx)
  var len = cols ? gh : gw, other = cols ? gw : gh
  var n = min(len, L)
  fillStrip((cols ? dy : dx) / len, n)
  var off = mode == 1 ? 0 : s0
  for (var c = 0; c < other; c++) {
    var u = (c + 0.5) / other - 0.5
    var o = off - (cols ? dx : dy) * u - (mode == 1 ? 0 : surf(u))
    var at = clamp(round((o / (cols ? dy : dx) + 0.5) * len), 0, len) - n
    if (cols) blit(cH, cS, cV, 1, n * 2, c, at, 0)
    else blit(cH, cS, cV, n * 2, 1, at, c, 0)
  }
  if (mode == 1) return
  hsv(hue0 + 0.55, 0.25, 0.9)
  var r = 0.7 / max(gw, 16)
  for (var i = 0; i < NB; i++) if (bOn[i]) splat(bx[i], by[i], r * 2.2, 1)
}

// Strips (index 0 = the bottom of the tank): pixel i shows depth top - i*k. The
// depth profile is linear between breakpoints (the surface, the foam line, the
// hue and brightness limits; the air glow and the wave crests are curves, cut
// finer), so each span is one anti-aliased gradientRange, added over a cleared
// strip so the spans meet exactly and the surface line moves smoothly.
var BP = [1, 0.5714, 0.025, 0, -0.05, -0.1, -0.15, -0.2, -0.25, -0.3, -0.35, -0.4]
var cuts = array(80)
var sTop = 0, sK = 1

function span(o0, o1) {                 // depths o0 > o1, cut into `sub` linear runs
  var x0 = (sTop - o0) / sK + 0.5, x1 = (sTop - o1) / sK + 0.5
  var sub = mode == 1 ? clamp(ceil((x1 - x0) / 4), 1, 3) : 1
  var d = (o0 - o1) / sub
  for (var j = 0; j < sub; j++) {
    var a = o0 - j * d, b = a - d, e = d * 0.002
    shadeAt(a - e)
    var h0 = oH, s0_ = oS, v0 = oV
    shadeAt(b + e)
    gradientRange((sTop - a) / sK + 0.5, (sTop - b) / sK + 0.5, h0, s0_, v0, oH, oS, oV, 1)
  }
}

function strip() {
  var n = pixelCount
  sK = 1 / max(n - 1, 1)
  sTop = mode == 1 ? 0.5 : 0.5 - s0 - surf(0.3) * 2
  var hi = sTop + 0.5 * sK, lo = sTop - (n - 0.5) * sK   // the depths at the strip's two ends
  var nc = 0
  cuts[nc++] = hi
  if (mode == 1) {
    // band edges (a hue step, phase j) and crests (phase j + 0.6), top down
    var p = hi * 5 - t * 0.9, pl = lo * 5 - t * 0.9
    var q = floor(p) + 0.6, crest = 1
    if (q >= p) { q = floor(p); crest = 0 }
    while (q > pl && nc < 78) {
      cuts[nc++] = (q + t * 0.9) / 5
      q -= crest ? 0.6 : 0.4
      crest = 1 - crest
    }
  } else {
    for (var b = 0; b < BP.length; b++) if (BP[b] < hi && BP[b] > lo) cuts[nc++] = BP[b]
  }
  cuts[nc++] = lo
  clear()
  for (var c = 1; c < nc; c++) span(cuts[c - 1], cuts[c])
}

// 3D maps


function cloud() {
  for (var i = 0; i < pixelCount; i++) {
    var x = pixelCoord(i, 0)
    var o = (x - 0.5) * dx + (pixelCoord(i, 1) - 0.5) * dy + (pixelCoord(i, 2) - 0.5) * dz
    shadeAt(o - (mode == 1 ? 0 : s0 + surf(x)))
    hsv(oH, oS, oV)
    setPixel(i)
  }
}

export function renderBulk() {
  var gw = gridWidth()
  if (has3DMap()) cloud()
  else if (has2DMap() && gw > 0) { panel(gw, gridHeight()); return }
  else strip()
  // fizz: a few white sparks in the liquid, more with sound
  if (mode == 1) return
  var lit = 0.5 - s0, sparks = 1 + floor(fizzAcc * 3 + (soundOn ? energyAverage * 8 : 0))
  hsv(hue0 + 0.55, 0.2, 1)
  for (var j = 0; j < sparks; j++) {
    var at = random(pixelCount)
    if (has3DMap()) setPixel(at)
    else if (at < lit * pixelCount) splatIndex(floor(at), 1.5, 2)   // a white pixel with a faint halo
  }
}
