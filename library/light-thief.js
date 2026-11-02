// name: Light Thief
// tags: both, holiday
// Curated original (hand-written showcase of the Luxel language/builtins).
// A string of holiday bulbs glows in the dark. A green glow creeps along it —
// sneaking, freezing, darting — and every bulb it reaches flickers twice and
// goes out. At the end of the string his glow warms to gold and a ring swells
// out from him, relighting every bulb it passes with an overshoot pop; the
// string shimmers, then the heist starts over. On a strip the bulbs sit along
// the strip; on a panel they hang from drooping garlands and the thief and
// ring are splats and lines. Only lit pixels are touched: no per-pixel loop.

var MAXL = 40
var lhue = array(MAXL), lbri = array(MAXL), lst = array(MAXL), lt = array(MAXL)
var lr = array(MAXL), lg = array(MAXL), lb = array(MAXL)   // resolved colour
var lu = array(MAXL), lx = array(MAXL), ly = array(MAXL)   // path pos, panel x/y
var cvt = array(3)

var STEAL_T = 0.45, RELIGHT_T = 0.6, HEART_T = 1.7, HOLD_T = 1.6, RING_W = 3
var SNEAK_NORM = 1.3727 // makes the sneak/dart profile average out to 1

var wantLights = 14
//# min=4 max=40 step=1 default=14
export function sliderLights(v) { wantLights = clamp(floor(v), 4, MAXL) }

var creepSeconds = 16   // seconds for the thief to cross the whole string
//# min=4 max=60 step=1 default=16
export function sliderCreepSeconds(v) { creepSeconds = clamp(v, 4, 60) }

var thiefHue = 105 / 360
//# min=0 max=360 step=1 default=105
export function sliderThiefHue(v) { thiefHue = clamp(v, 0, 360) / 360 }

var glow = 6            // thief glow radius, pixels
//# min=1 max=20 step=1 default=6
export function sliderGlowSize(v) { glow = clamp(floor(v), 1, 20) }

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

// --- path: a strip is 0..pixelCount; a panel is garlands joined at the ends
var nL = 0, panel = 0, gw = 0, gh = 0, strings = 1, total = 1, spanW = 0.92
var M = 0.04, Y0 = 0.12, Y1 = 0.5, SAG = 0.16
var px = 0, py = 0
function pathXY(u) {             // panel position of path length u
  var drop = Y1 - Y0, i = 0, v = u
  if (strings == 2 && u > spanW) {
    if (u < spanW + drop) { px = 1 - M; py = Y0 + u - spanW; return }
    i = 1
    v = u - spanW - drop
  }
  var a = clamp(v / spanW, 0, 1)
  px = i == 0 ? M + v : 1 - M - v
  var w = mod(a * (strings == 2 ? 2 : 3), 1)
  py = (i == 0 ? Y0 : Y1) + SAG * 4 * w * (1 - w)
}

function relayout() {
  var p2 = has2DMap() && gridWidth() > 0
  // a strip needs >= 2 px per bulb, a garland >= 3.5 px of wire per bulb
  var room = p2 ? (gridHeight() >= 24 ? 2 : 1) * floor(gridWidth() * 0.92 / 3.5) : floor(pixelCount / 2)
  var want = clamp(wantLights, 2, min(MAXL, room))
  if (want == nL && p2 == panel && gridWidth() == gw) return
  panel = p2
  gw = gridWidth()
  gh = gridHeight()
  nL = want
  if (panel) {
    strings = gh >= 24 ? 2 : 1
    Y0 = strings == 2 ? 0.12 : 0.25
    SAG = strings == 2 ? 0.16 : 0.22
    total = spanW * strings + (strings == 2 ? Y1 - Y0 : 0)
    var per = ceil(nL / strings)
    for (var k = 0; k < nL; k++) {
      var i = floor(k / per), j = k - i * per, cnt = min(per, nL - i * per)
      lu[k] = i * (spanW + Y1 - Y0) + (j + 0.5) / cnt * spanW
      pathXY(lu[k])
      lx[k] = px
      ly[k] = py + 1.6 / gh            // the bulb hangs just below the wire
    }
  } else {
    total = pixelCount
    for (var k = 0; k < nL; k++) lu[k] = (k + 0.5) * pixelCount / nL
  }
  for (var k = 0; k < nL; k++) lhue[k] = festiveHue(hash(k * 7.31 + nL))
  relightAll()
}

// a five-colour strand: red, gold, green, blue, warm amber
function festiveHue(u) {
  var pick = floor(u * 5)
  return pick == 0 ? 0 : pick == 1 ? 0.11 : pick == 2 ? 0.33 : pick == 3 ? 0.6 : 0.05
}
function relightAll() { for (var k = 0; k < nL; k++) { lst[k] = 0; lt[k] = 0; lbri[k] = 1 } }

// --- story state -----------------------------------------------------------
var phase = 0, gpos = -1, heartR = 0, holdT = 0, gBri = 1, ghue = 0.29, ringBri = 0
var gr = 0, gg = 0, gb = 0, ringR = 0, ringG = 0, ringB = 0, hx = 0, hy = 0, inited = 0
var bgR = 0.004, bgG = 0.006, bgB = 0.02

function margin() { return panel ? glow / max(gw, 1) + 0.05 : glow + 2 }
function bulbDist(k) {           // from the ring's centre, in path units
  if (!panel) return abs(lu[k] - hx)
  return hypot(lx[k] - hx, ly[k] - hy)
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  relayout()
  if (!inited) { inited = 1; gpos = -margin() }
  var reach = panel ? total / nL * 0.45 : max(min(total / nL * 0.42, 2.5), glow * 0.35)

  if (phase == 0) {
    var sneak = (0.25 + 1.75 * pow(wave(time(0.05)), 4)) * SNEAK_NORM
    gpos += (total + margin() * 2) / creepSeconds * sneak * dt
    gBri = 1
    ghue = thiefHue
    for (var k = 0; k < nL; k++) if (lst[k] == 0 && abs(gpos - lu[k]) < reach) { lst[k] = 1; lt[k] = 0 }
    if (gpos > total + (panel ? 0 : glow)) {
      phase = 1
      heartR = 0
      if (panel) { gpos = total; pathXY(total); hx = px; hy = py } else { hx = pixelCount - 1; gpos = pixelCount + glow * 0.5 }
    }
  } else if (phase == 1) {
    var far = panel ? 1.5 : pixelCount
    heartR += far / HEART_T * dt
    ghue = mix(thiefHue, 0.11, saturate(heartR / far))
    gBri = 1 + 0.9 * pow(wave(time(0.012)), 3)
    for (var k = 0; k < nL; k++) if (lst[k] == 2 && bulbDist(k) <= heartR) { lst[k] = 3; lt[k] = 0 }
    if (heartR > far + (panel ? 0.1 : RING_W * 2)) { phase = 2; holdT = 0 }
  } else {
    holdT += dt
    ghue = 0.11
    gBri = max(1 - holdT / (HOLD_T * 0.6), 0)
    if (holdT > HOLD_T) { phase = 0; gpos = -margin(); relightAll() }
  }

  for (var k = 0; k < nL; k++) {
    var st = lst[k], white = 0
    if (st == 0) lbri[k] = 0.82 + 0.18 * wave(time(0.03) + k * 0.13)
    else if (st == 1) {
      lt[k] += dt / STEAL_T
      // once dark the socket is restocked: it comes back another colour
      if (lt[k] >= 1) { lst[k] = 2; lbri[k] = 0; lhue[k] = festiveHue(random(1)) }
      else lbri[k] = (1 - lt[k]) * (0.4 + 0.6 * square(lt[k] * 7, 0.55))
    } else if (st == 2) lbri[k] = 0
    else {
      lt[k] += dt / RELIGHT_T
      if (lt[k] >= 1) { lst[k] = 0; lbri[k] = 1; lt[k] = 0 }
      else { var u = 1 - lt[k]; lbri[k] = 1 + 1.5 * u * u; white = u * u }
    }
    hsv2rgb(lhue[k], 1 - white * 0.85, 1, cvt)
    lr[k] = cvt[0]; lg[k] = cvt[1]; lb[k] = cvt[2]
  }
  hsv2rgb(ghue, phase == 0 ? 1 : 0.75, 1, cvt)
  gr = cvt[0] * gBri; gg = cvt[1] * gBri; gb = cvt[2] * gBri
  ringBri = phase == 1 ? saturate(1.15 - heartR / ((panel ? 1.5 : pixelCount) * 1.15)) : 0
  hsv2rgb(0.11, 0.55, 1, cvt)
  ringR = cvt[0] * ringBri; ringG = cvt[1] * ringBri; ringB = cvt[2] * ringBri
}

// --- strip: bulbs, thief and ring as additive splatIndex blobs -----------
// bump(): a soft (1 - d/r)^2 bump of colour (r, g, b) x v at pos, made of two
// linear splats (radius r and r/2, half weight each); v above 1 is laid on in
// passes so an overshoot still saturates towards white
function bump(pos, rad, r, g, b, v) {
  while (v > 0.002) {
    var k = min(v, 1) * 0.5
    rgb(r * k, g * k, b * k)
    splatIndex(pos, rad, 1)
    splatIndex(pos, max(rad * 0.5, 0.5), 1)
    v -= 1
  }
}

function drawStrip() {
  var bulbR = clamp(pixelCount / nL * 0.42, 0.8, 2.5)   // bulbs stay bulb-sized
  rgb(bgR, bgG, bgB)
  fill()
  for (var k = 0; k < nL; k++) bump(lu[k], bulbR, lr[k], lg[k], lb[k], lbri[k])
  bump(gpos, glow, gr, gg, gb, 0.75)
  var core = 0.7                                   // the thief's bright heart
  while (core > 0.002) {
    var c = min(core, 1)
    rgb(gr * c, gg * c, gb * c)
    splatIndex(gpos, 1.6, 1)
    core -= 1
  }
  if (ringBri > 0) {
    bump(hx - heartR, RING_W, ringR, ringG, ringB, 1)
    bump(hx + heartR, RING_W, ringR, ringG, ringB, 1)
  }
}

// --- panel: garland wire, hanging bulbs, the thief, the ring -------------
function drawPanel() {
  var pxw = 1 / max(gw, 1), br = gw < 24 ? 1.6 * pxw : max(2.3 * pxw, 0.06)
  fillGradient(0.64, 0.9, 0.09, 0.68, 0.85, 0.02, 2)        // night sky, top to bottom
  rgb(0.1, 0.11, 0.16)
  fillRect(0, 0.88, 1, 1)                                  // snow on the ground
  rgb(0.03, 0.06, 0.03)
  var steps = 24 * strings, du = total / steps
  pathXY(0)
  var ax = px, ay = py
  for (var i = 1; i <= steps; i++) {
    pathXY(i * du)
    if (abs(px - ax) > 0.0001) drawLine(ax, ay, px, py, 0.7 * pxw, 2)
    ax = px; ay = py
  }
  for (var k = 0; k < nL; k++) {
    var v = lbri[k]
    if (v <= 0.04) {                                       // an empty socket: dim glass
      rgb(0.03, 0.03, 0.04)
      splat(lx[k], ly[k], br, 2)
      continue
    }
    rgb(lr[k] * v * 0.2, lg[k] * v * 0.2, lb[k] * v * 0.2)
    splat(lx[k], ly[k], br * 2.6, 1)                       // light on the wall
    rgb(lr[k] * v, lg[k] * v, lb[k] * v)
    splat(lx[k], ly[k], br, 1)
  }
  pathXY(gpos)
  var tx = px, ty = py + 1.6 / gh
  if (phase < 2 || gBri > 0) {
    rgb(gr * 0.55, gg * 0.55, gb * 0.55)
    splat(tx, ty, glow * pxw, 1)
    rgb(gr, gg, gb)
    splat(tx, ty, 1.8 * pxw, 1)
    rgb(gBri, gBri * 0.9, 0.1 * gBri)                      // two beady eyes
    var ex = round(tx * (gw - 1)), ey = round(ty * (gh - 1)) - 1
    setPixel(gridIndex(ex - 1, ey))
    setPixel(gridIndex(ex + 1, ey))
  }
  if (ringBri > 0) {
    rgb(ringR, ringG, ringB)
    var segs = 28, a0 = 0
    for (var i = 1; i <= segs; i++) {
      var a1 = i * PI2 / segs
      drawLine(hx + heartR * cos(a0), hy + heartR * sin(a0), hx + heartR * cos(a1), hy + heartR * sin(a1), 2 * pxw, 1)
      a0 = a1
    }
  }
}

export function renderBulk() {
  if (panel) drawPanel()
  else drawStrip()
}
