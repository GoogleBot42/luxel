// name: Moon Phase
// tags: panel, clock, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// Tonight's moon, from the device clock: days since a known new moon, modulo
// the synodic month. A 32x32 sphere canvas holds the face (maria, rayed
// Tycho, limb darkening) once; each phase is two scaled arrays summed — the
// sun's direction dotted with the surface — and a live palette turns that
// into a soft terminator and bluish earthshine. paintCanvas draws it through
// a transform that fits the canvas to the disc, so the moon stays sharp on a
// big panel. Halo and twinkling stars on top. Cycle mode runs a whole month.

var mode = 0              // 0 tonight (clock), 1 cycle through a lunation
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1); dirty = 1 }

var cycleSec = 30         // Cycle mode: seconds per lunation
//# min=10 max=120 step=1 default=30
export function sliderCycleSeconds(v) { cycleSec = clamp(v, 10, 120) }

var sizePct = 72          // disc diameter, % of the panel's short side
//# min=40 max=95 step=1 default=72
export function sliderMoonSizePercent(v) { sizePct = clamp(v, 40, 95); dirty = 1 }

var south = 0             // southern hemisphere: the lit side flips
//# default=0
export function toggleSouthernHemisphere(on) { south = on ? 1 : 0; dirty = 1 }

var SYN = 29.530589, C = 32
var U = array(C * C), Z = array(C * C), A = array(C * C), L = array(C * C), T = array(C * C)
var pal = array(36)
setPalette(pal)

// the near side, roughly as seen from the north: dark maria (u right, v down)
var MU = [-0.32, 0.18, 0.4, 0.7, -0.62, -0.15, 0.58, 0.4, -0.05]
var MV = [-0.45, -0.4, -0.08, -0.3, 0.02, 0.36, 0.17, 0.33, -0.1]
var MR = [0.3, 0.17, 0.2, 0.12, 0.36, 0.18, 0.14, 0.1, 0.12]
function buildFace() {
  for (var r = 0; r < C; r++) for (var c = 0; c < C; c++) {
    var i = r * C + c, u = (c + 0.5) / C * 2 - 1, v = (r + 0.5) / C * 2 - 1
    var d2 = u * u + v * v, rim = 1 - 2.2 / C
    U[i] = u
    if (d2 >= 1) { Z[i] = 0; A[i] = 0; continue }   // the border cells stay 0: paintCanvas clamps to them
    var z = sqrt(1 - d2), f = 0
    Z[i] = z
    var wu = u + 0.12 * simplex2(u * 2.5, v * 2.5, 1), wv = v + 0.12 * simplex2(u * 2.5, v * 2.5, 2)
    for (var m = 0; m < 9; m++) {                   // maria as metaballs: neighbours merge
      var q = 1 - ((wu - MU[m]) * (wu - MU[m]) + (wv - MV[m]) * (wv - MV[m])) / (MR[m] * MR[m] * 1.6)
      if (q > 0) f += q * q
    }
    var a = 1 - 0.42 * smoothstep(0.2, 0.55, f + 0.12 * simplex2(u * 6, v * 6, 3))
    var ty = hypot(u + 0.1, v - 0.72)                 // Tycho and its rays
    if (ty < 0.07) a = 1.15
    else if (ty < 0.9) a += 0.2 * max(0, cos(atan2(v - 0.72, u + 0.1) * 11)) * (0.9 - ty)
    if (hypot(u + 0.3, v + 0.15) < 0.06) a = 1.1      // Copernicus
    a = a * (0.9 + 0.1 * simplex2(u * 9, v * 9, 20)) * (0.84 + 0.16 * z)
    A[i] = a * clamp((rim - sqrt(d2)) * C * 0.5 + 0.5, 0, 1)
  }
}
buildFace()

function stop(k, pos, r, g, b) { pal[k * 4] = pos; pal[k * 4 + 1] = r; pal[k * 4 + 2] = g; pal[k * 4 + 3] = b }
// position = 0.48 lit wrapped: 0..0.48 the sunlit side, 0.52..1 the night side
function buildPalette(earth) {
  var er = 0.08 * earth, eg = 0.1 * earth, eb = 0.16 * earth
  stop(0, 0, 0.4, 0.37, 0.32)
  stop(1, 0.06, 0.85, 0.8, 0.7)
  stop(2, 0.2, 1, 0.96, 0.86)
  stop(3, 0.49, 1, 0.97, 0.9)
  stop(4, 0.51, er, eg, eb)
  stop(5, 0.93, er, eg, eb)
  stop(6, 0.97, 0.1 + er, 0.09 + eg, 0.08 + eb)
  stop(7, 0.995, 0.36, 0.33, 0.29)
  stop(8, 1, 0.4, 0.37, 0.32)
}

var ph = 0, lastPh = -1, dirty = 1, cyc = 0, t = 0, gw = -1, gh = 0, rx = 0.36, ry = 0.36
var CUM = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
function livePhase() {     // days since the new moon of 2000-01-06 18:14 UTC
  var y = clockYear(), m = clockMonth(), d = clockDay()
  if (y < 2000) return -1
  var dy = y - 2000, leap = (y % 4 == 0) ? 1 : 0
  var days = dy * 365 + floor((dy + 3) / 4) + CUM[clamp(m - 1, 0, 11)] + (m > 2 ? leap : 0) + d - 6
  days += (clockHour() + clockMinute() / 60 - 18.23) / 24
  return mod(days, SYN) / SYN
}

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  t = mod(t + dt, 3600)
  var w = gridWidth(), h = gridHeight()
  if (w != gw || h != gh) { gw = w; gh = h; dirty = 1 }
  var live = mode == 0 ? livePhase() : -1
  if (live < 0) { cyc = mod(cyc + dt / cycleSec, 1); ph = cyc }
  else ph = live
  if (abs(ph - lastPh) > 0.0012) dirty = 1
}

function drawMoon() {
  var s = min(gw > 0 ? gw : 1, gh > 0 ? gh : 1), k = sizePct / 200
  rx = gw > 0 ? k * s / gw : k
  ry = gh > 0 ? k * s / gh : k
  var a = ph * PI2, sn = sin(a) * (south ? -1 : 1), cs = cos(a)
  arrayMix(L, U, 1); arrayScale(L, sn * 0.48)      // L = 0.48 (sun . surface): never 0.5
  arrayMix(T, Z, 1); arrayScale(T, -cs * 0.48)
  arrayAdd(L, T)
  buildPalette(0.6 + 0.4 * cs)                      // earthshine strongest near new moon
  resetTransform()
  translate(rx - 0.5, ry - 0.5)
  scale(0.5 / rx, 0.5 / ry)
  paintCanvas(L, C, C, A, 1)
  resetTransform()
  var lit = (1 - cs) / 2
  rgb(0.1 * lit + 0.008, 0.1 * lit + 0.01, 0.12 * lit + 0.018)
  splat(0.5, 0.5, rx * 1.7, 1)
  lastPh = ph
  dirty = 0
}

function stars() {
  var w = gw > 0 ? gw : 32, hh = gh > 0 ? gh : 32, n = clamp(floor(w * hh / 100), 6, 60)
  var wash = 1 - 0.6 * (1 - cos(ph * PI2)) / 2
  for (var i = 0; i < n; i++) {
    var x = hash(i * 1.7 + 0.3), y = hash(i * 2.9 + 5.1)
    var dx = (x - 0.5) / rx, dy = (y - 0.5) / ry
    if (dx * dx + dy * dy < 1.35) continue           // not behind the moon
    var b = (0.25 + 0.6 * hash(i * 4.3)) * wash * (0.6 + 0.4 * sin(t * (1 + hash(i) * 2.5) + i))
    rgb(b * 0.9, b * 0.93, b)
    setPixel(gridIndex(floor(x * w), floor(y * hh)))
  }
}

export function renderBulk() {
  if (dirty) drawMoon()
  stars()
}
