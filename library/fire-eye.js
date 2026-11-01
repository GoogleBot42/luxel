// name: Fire Eye
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/eye-of-sauron.md, eye-of-sauron-with-movement.md — reimagined, not a replica.
// A wide almond of churning ridged flame with a black slit pupil: filaments stream
// outward and flare, white-hot where they lap the slit. Gaze lets the pupil dart
// about in eased saccades — quick twitches, and big sweeps after a long stare.
// Both flame fields are built once per frame on a 64-angle x 24-radius polar
// canvas, read bilinearly per panel cell through a packed (angle, radius) table;
// the pupil is analytic over its own bounding box; one paintCanvas paints it.
// The table's angle is taken about the pupil (re-packed only while it moves), so
// the filaments always radiate from wherever the eye is looking.

var NA = 64, NR = 24, NA1 = NA + 1
var CAP = 64
var WRAP = 16                     // noise wrap the clocks sweep over
var EDGE = 2                      // almond rim, in eye units
var YSTRETCH = 1.4                // vertical squeeze -> a wide almond

var tendrils = 8                  // flame filaments around the eye
var radialDetail = 1              // noise cells per eye unit along the radius
var pupil = 62                    // pupil height, percent of the eye height
var slitRatio = 5.3               // pupil height : width
var gaze = 0                      // how far the eye darts, percent

//# min=2 max=18 step=1 default=8
export function sliderTendrils(v) { tendrils = clamp(floor(v), 2, 18) }
//# min=0.2 max=2 step=0.05 default=1
export function sliderRadialDetail(v) { radialDetail = clamp(v, 0.2, 2) }
//# min=20 max=90 step=1 default=62
export function sliderPupilPercent(v) { pupil = clamp(v, 20, 90) }
//# min=1 max=9 step=0.1 default=5.3
export function sliderSlitRatio(v) { slitRatio = clamp(v, 1, 9) }
//# min=0 max=100 step=1 default=0
export function sliderGazePercent(v) { gaze = clamp(v, 0, 100) }

setPalette([0, 0, 0, 0, 0.2, 0.6, 0, 0, 0.55, 1, 0.35, 0, 0.8, 1, 0.9, 0, 1, 1, 1, 1])

var pol = array(NA1 * NR)         // polar flame canvas, 0..1
var tab = array(CAP * CAP)        // per panel cell: angle x*4096 + radius y, packed
var cv = array(CAP * CAP)         // per panel cell value: palette position AND brightness

var CW = 0, CH = 0, M = 1
var morph = 0, flow = 0, burn = 0
var gx = 0, gy = 0, tx = 0, ty = 0, dwell = 0.5, prevDwell = 0.5
var tabGx = -9, tabGy = -9

// angle about the pupil, radius about the eye's centre (the almond stays put)
function buildTable() {
  for (var r = 0; r < CH; r++) {
    var ey = (r - CH * 0.5 + 0.5) / M * 3 * YSTRETCH
    for (var c = 0; c < CW; c++) {
      var ex = (c - CW * 0.5 + 0.5) / M * 3
      var a = atan2(ey - gy, ex - gx) / PI2 + 0.5
      var rr = min(hypot(ex, ey), EDGE) * (NR - 1) / EDGE
      tab[r * CW + c] = floor((a * NA + 0.5) / NA1 * 4096) + (rr + 0.5) / NR
    }
  }
  tabGx = gx; tabGy = gy
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var w = gridWidth() > 0 ? min(gridWidth(), CAP) : 32
  var h = gridHeight() > 0 ? min(gridHeight(), CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; M = min(w, h); tabGx = -9 }

  morph = time(6.4) * WRAP        // minutes of slow reshaping
  flow = time(0.75) * WRAP        // outward streaming
  burn = time(0.09) * WRAP        // seconds-scale flicker

  // saccades: long stares earn big jumps, rapid twitches stay local
  dwell -= dt
  if (dwell <= 0) {
    var reach = clamp(prevDwell * 0.45, 0.06, 0.9) * gaze * 0.01
    tx = (random(2) - 1) * reach
    ty = (random(2) - 1) * reach * 0.5
    prevDwell = 0.05 + random(1.95)
    dwell = prevDwell
  }
  var k = 1 - pow(0.55, delta / 16)
  gx += (tx - gx) * k
  gy += (ty - gy) * k
  if (abs(tx - gx) + abs(ty - gy) < 0.004) { gx = tx; gy = ty }
  if (gx != tabGx || gy != tabGy) buildTable()

  // the flame: a slow ridged field times a fast double-frequency one, faded
  // to the almond rim
  setPerlinWrap(tendrils, WRAP, WRAP)
  var du = tendrils / NA, dR = EDGE / (NR - 1)
  for (var j = 0; j < NR; j++) {
    var r = j * dR, row = j * NA1
    var rin = clamp(EDGE - r, 0, 1)
    rin = rin * rin
    var ry = r * radialDetail - flow, ry2 = 2 * r * radialDetail - burn
    for (var i = 0; i < NA; i++) {
      var n = perlinRidge(i * du, ry, morph, 2, 0.5, 1.05, 3)
      n = n * (0.6 + 1.8 * perlinRidge(i * du * 2, ry2, burn, 2, 0.5, 1.05, 1))
      pol[row + i] = min(n * rin, 1)
    }
    pol[row + NA] = pol[row]
  }
}

export function renderBulk() {
  var n = CW * CH
  for (var i = 0; i < n; i++) {
    var p = tab[i]
    cv[i] = canvasGet(pol, NA1, floor(p) * 0.000244140625, frac(p))
  }

  // the slit: bites into the fire, then a white-hot rim hugs it
  var dil = pupil * 0.021, ring = dil * 1.4
  var cx0 = CW * 0.5 - 0.5 + gx / 3 * M, cy0 = CH * 0.5 - 0.5 + gy / (3 * YSTRETCH) * M
  var hw = ring / slitRatio / 3 * M + 1, hh = ring / (3 * YSTRETCH) * M + 1
  var c0 = max(0, floor(cx0 - hw)), c1 = min(CW - 1, ceil(cx0 + hw))
  var r0 = max(0, floor(cy0 - hh)), r1 = min(CH - 1, ceil(cy0 + hh))
  var sx = 3 / M * slitRatio, sy = 3 * YSTRETCH / M
  for (var r = r0; r <= r1; r++) {
    var dy = (r - cy0) * sy
    for (var c = c0; c <= c1; c++) {
      var slit = hypot((c - cx0) * sx, dy)
      if (slit >= ring) continue
      var k = r * CW + c
      var v = cv[k] - max(0, dil - slit) * 2
      var near = 1 - slit / ring
      cv[k] = clamp(v * (1 + 2.2 * near * near), 0, 1)
    }
  }
  paintCanvas(cv, CW, CH, cv)
}
