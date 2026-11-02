// name: Dot Globe
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// A planet of a few hundred glowing dots, spread evenly by the golden-angle spiral,
// spinning on a tilted, gently nodding axis. Near dots are big and bright, far ones
// fade to pinpricks. Earth lights continents (cached simplex per dot), ice caps and
// a day side under a thin blue rim; Neon bands it in a turning rainbow; Hologram
// shows land as bright cyan over a faint sea of dots.
// Every dot is one splat (max blend), so the cost is O(dots), not O(pixels).

var MAXD = 400
var GA = 2.399963                 // golden angle, radians

var dots = 300
var spin = 20                     // seconds per turn
var tilt = 23                     // axial tilt (a roll in the picture), degrees
var style = 0                     // 0 Earth, 1 Neon, 2 Hologram
var dotSize = 100                 // percent

//# min=100 max=400 step=10 default=300
export function sliderDots(v) { dots = clamp(floor(v), 100, MAXD); built = 0 }
//# min=4 max=60 step=1 default=20
export function sliderSpinSeconds(v) { spin = clamp(v, 4, 60) }
//# min=0 max=45 step=1 default=23
export function sliderTiltDegrees(v) { tilt = clamp(v, 0, 45) }
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }
//# min=50 max=200 step=5 default=100
export function sliderDotSizePercent(v) { dotSize = clamp(v, 50, 200) }

var px = array(MAXD), py = array(MAXD), pz = array(MAXD), land = array(MAXD)
var built = 0, A = 0, T = 0

// golden-angle sphere; land = a cached 3D simplex read at the dot's position
function build() {
  for (var i = 0; i < dots; i++) {
    var y = 1 - 2 * (i + 0.5) / dots, rr = sqrt(1 - y * y)
    px[i] = cos(i * GA) * rr; py[i] = y; pz[i] = sin(i * GA) * rr
    var n = simplex3(px[i] * 1.1, py[i] * 1.1, pz[i] * 1.1, 11) + 0.3 * simplex3(px[i] * 2.6, py[i] * 2.6, pz[i] * 2.6, 12)
    // 3 ice, 2 desert, 1 forest, 0 sea
    land[i] = abs(y) > 0.9 ? 3 : n > 0.1 ? (abs(y) < 0.45 && n < 0.32 ? 2 : 1) : 0
  }
  built = 1
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  if (!built) build()
  A = mod(A + dt / spin, 1)
  T = mod(T + dt, 3600)
}

export function renderBulk() {
  clear()
  var W = gridWidth() > 0 ? gridWidth() : 32, H = gridHeight() > 0 ? gridHeight() : 32
  var M = min(W, H), R = 0.44, rx = R * M / W, ry = R * M / H
  var ca = cos(A * PI2), sa = sin(A * PI2)
  var b = (14 + 6 * sin(T * PI2 / 17)) * PI / 180, cb = cos(b), sb = sin(b)   // nod
  var ct = cos(tilt * PI / 180), st = sin(tilt * PI / 180)
  var rad = sqrt(12.57 / dots) * R * 0.4 * dotSize * 0.01, rmin = 0.8 / M

  if (style == 0) {                 // thin atmosphere rim
    hsv(0.58, 0.8, 0.22)
    for (var k = 0; k < 40; k++) {
      var a0 = k * PI2 / 40, a1 = a0 + PI2 / 40
      drawLine(0.5 + cos(a0) * rx * 1.04, 0.5 + sin(a0) * ry * 1.04,
        0.5 + cos(a1) * rx * 1.04, 0.5 + sin(a1) * ry * 1.04, 1.4 / M, 2)
    }
  }

  for (var i = 0; i < dots; i++) {
    var x = px[i] * ca + pz[i] * sa, z = pz[i] * ca - px[i] * sa
    var y = py[i] * cb - z * sb
    z = py[i] * sb + z * cb
    if (z < -0.3) continue
    var x2 = x * ct - y * st
    y = x * st + y * ct
    x = x2
    var d = (z + 1) * 0.5           // 0 far .. 1 near
    var v = 0.15 + 0.85 * d * d, L = land[i]
    if (style == 0) {
      // lit from the upper left: a soft day side and a dim night side
      var lit = 0.4 + 0.6 * saturate(-0.45 * x + 0.5 * y + 0.74 * z), hh = hash(i)
      if (L == 3) hsv(0.58, 0.08, v * lit)
      else if (L == 2) hsv(0.09 + 0.03 * hh, 0.65, v * lit)
      else if (L == 1) hsv(0.26 + 0.07 * hh, 0.85, v * lit)
      else hsv(0.6, 0.9, v * lit * 0.9)
    } else if (style == 1) {
      hsv(i * GA / PI2 + py[i] * 0.2 + T * 0.05, 0.85, v)   // meridian rainbow
    } else {
      hsv(0.5, 0.6, v * (L > 0 ? 1 : 0.32))
    }
    splat(0.5 + x * rx, 0.5 - y * ry, max(rad * (0.55 + 0.6 * d), rmin), 2)
  }
}
