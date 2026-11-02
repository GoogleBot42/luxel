// name: Geometry Morph
// Clean-room reimplementation from the prose spec docs/pattern-specs/geometry-morphing-demo-2d.md — reimagined, not a replica.
// A spinning figure melts through circle, cross, star, square, triangle and
// hexagon by blending signed distances, painted in contour bands that follow
// whatever silhouette the morph is passing through. Quad shows four figures,
// each a step apart in the sequence, counter-rotating (panels of 48 px and up).
// The SDF is evaluated on a 32x32 field per frame; pixels sample it bilinearly.

var size = 0.38
var filled = 1
var lineWidth = 0.05
var holdS = 1
var morphS = 1
var spinS = 7
var palette = 0
var quad = 0

//# min=0.05 max=0.6 step=0.01 default=0.38
export function sliderSize(v) { size = clamp(v, 0.05, 0.6) }

//# default=1
export function toggleFilled(v) { filled = v > 0.5 }

//# min=0.01 max=0.2 step=0.005 default=0.05
export function sliderLineWidth(v) { lineWidth = clamp(v, 0.01, 0.2) }

//# min=0.2 max=8 step=0.1 default=1
export function sliderHoldS(v) { holdS = clamp(v, 0.2, 8) }

//# min=0.2 max=8 step=0.1 default=1
export function sliderMorphS(v) { morphS = clamp(v, 0.2, 8) }

// Seconds per full turn.
//# min=2 max=60 step=1 default=7
export function sliderSpinS(v) { spinS = clamp(v, 2, 60) }

// 0 rainbow contours · 1 OKLCH contours (even brightness)
//# min=0 max=1 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 1) }

//# default=0
export function toggleQuad(v) { quad = v > 0.5 }

var clock = 0, cur = 0, blend = 0, rot = 0, drift = 0, cs = 1, sn = 0
var useQuad = 0, sz = 0.38, lw = 0.05, invLw = 20, reach = 1

export function beforeRender(delta) {
  var dt = delta / 1000
  clock = mod(clock + dt, (holdS + morphS) * 6)
  var step = floor(clock / (holdS + morphS))
  var ph = clock - step * (holdS + morphS)
  cur = step
  blend = ph > holdS ? easeInOutCubic((ph - holdS) / morphS) : 0
  rot = mod(rot + dt * PI2 / spinS, PI2)
  cs = cos(rot)
  sn = sin(rot)
  drift = time(0.4)
  useQuad = quad && gridWidth() >= 48
  sz = useQuad ? size * 0.42 : size
  lw = useQuad ? max(lineWidth * 0.6, 0.02) : lineWidth
  invLw = 1 / lw
  reach = (useQuad ? 1.45 : max(outR[step], outR[step + 1])) * sz + lw
  reach *= reach
  buildField()
}

// signed distances, radius r (Inigo Quilez-style 2D SDFs)
function sdCircle(x, y, r) { return sqrt(x * x + y * y) - r }

function sdCross(x, y, r) {
  var ax = abs(x), ay = abs(y), bw = r * 0.32
  if (ay > ax) { var t = ax; ax = ay; ay = t }
  var qx = ax - r, qy = ay - bw, kk = max(qx, qy)
  if (kk > 0) { qx = max(qx, 0); qy = max(qy, 0); return sqrt(qx * qx + qy * qy) }
  qx = max(bw - ax, 0)
  return -sqrt(qx * qx + kk * kk)
}

function sdStar(x, y, r) {
  var ax = abs(x), ay = abs(y)
  r *= 0.72
  var m = 2 * min(0.8660254 * ay - 0.5 * ax, 0)
  ax += m * 0.5
  ay -= m * 0.8660254
  m = 2 * min(0.8660254 * ax - 0.5 * ay, 0)
  ax -= m * 0.8660254
  ay += m * 0.5
  ax -= clamp(ax, r * 0.5773503, r * 1.7320508)
  ay -= r
  m = sqrt(ax * ax + ay * ay)
  return ay < 0 ? -m : m
}

function sdSquare(x, y, r) {
  var ax = abs(x) - r, ay = abs(y) - r
  var px = max(ax, 0), py = max(ay, 0)
  return sqrt(px * px + py * py) + min(max(ax, ay), 0)
}

function sdTriangle(x, y, r) {
  var ax = abs(x) - r, ay = y + r * 0.5773503
  if (ax + 1.7320508 * ay > 0) {
    var t = (ax - 1.7320508 * ay) / 2
    ay = (-1.7320508 * ax - ay) / 2
    ax = t
  }
  ax -= clamp(ax, -2 * r, 0)
  t = sqrt(ax * ax + ay * ay)
  return ay > 0 ? -t : t
}

function sdHexagon(x, y, r) {
  var ax = abs(x), ay = abs(y)
  var m = 2 * min(0.5 * ay - 0.8660254 * ax, 0)
  ax += m * 0.8660254
  ay -= m * 0.5
  ax -= clamp(ax, -0.5773503 * r, 0.5773503 * r)
  ay -= r
  m = sqrt(ax * ax + ay * ay)
  return ay < 0 ? -m : m
}

var shapes = [sdCircle, sdCross, sdStar, sdSquare, sdTriangle, sdHexagon, sdCircle, sdCross, sdStar, sdSquare]
var CW = 32, dist = array(CW * CW)
var outR = [1, 1.05, 1.44, 1.42, 1.16, 1.16, 1, 1.05, 1.44, 1.42]   // circumradius per shape, in r

// the signed distance on a 32 x 32 field, once per frame (twice the work while
// morphing); every pixel then takes a bilinear sample of it
function buildField() {
  var i = 0, u, v, x, y, d, k, t, s2
  for (var r = 0; r < CW; r++) for (var c = 0; c < CW; c++) {
    u = (c + 0.5) / CW - 0.5
    v = (r + 0.5) / CW - 0.5
    k = cur
    s2 = sn
    if (useQuad) {
      var qx = u >= 0, qy = v >= 0
      k += qx + 2 * qy
      u = u - qx * 0.5 + 0.25
      v = v - qy * 0.5 + 0.25
      if (qx != qy) s2 = -sn                    // neighbours turn the other way
    }
    t = u * u + v * v
    if (t > reach) d = sqrt(t) - outR[k] * sz   // far outside: a cheap upper bound
    else {
      x = u * cs + v * s2
      y = v * cs - u * s2
      d = shapes[k](x, y, sz)
      if (blend > 0) d = mix(d, shapes[k + 1](x, y, sz), blend)
    }
    dist[i++] = d
  }
}

export function render2D(index, x, y) {
  var d = canvasGet(dist, CW, x, y)
  var e = filled ? d : abs(d)
  if (e >= lw) { rgb(0, 0, 0); return }
  var bri = min(1 - e * invLw, 1)
  if (palette) oklch(0.3 + 0.45 * bri, 0.14, d * 3 + drift)
  else hsv(d * 1.6 + drift, 1, bri * bri)
}
