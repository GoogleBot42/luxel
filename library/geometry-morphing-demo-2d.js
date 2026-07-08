// name: Geometry Morphing Demo 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Geometry Morphing Demo 2D"; original source never
// consulted.

// A single centered figure (circle, plus/cross, six-pointed star, square,
// triangle, hexagon in repeating order) slowly spins while smoothly melting
// into the next shape by lerping signed-distance values. Rainbow contour
// bands follow the edge and drift over time. Outline or filled per control.

var NSHAPES = 6
var HOLD_DUR = 1.0        // seconds per hold/morph phase

// ---- controls ----
var size = 0.15
var filled = 0
var lineWidth = 0.04

//# min=0.02 max=0.25 step=0.01 default=0.15
export function sliderSize(v) { size = 0.02 + v * 0.23 }

//# min=0 max=1 step=0.01 default=0.3
export function sliderFilled(v) { filled = (v > 0.5) ? 1 : 0 }

//# min=0 max=1 step=0.01 default=0.35
export function sliderLineWidth(v) {
  lineWidth = v * v * 0.12 + 0.006   // quadratic: fine control at the low end
}

// ---- animation state ----
var morphClock = 0
var holdFlag = 1          // 1 = hold phase, 0 = morph phase
var curShape = 0
var nextShape = 1
var blend = 0
var rot = 0
var hueDrift = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  morphClock = morphClock + dt
  if (morphClock >= HOLD_DUR) {
    morphClock = morphClock - HOLD_DUR
    holdFlag = 1 - holdFlag
    if (holdFlag == 1) {
      // entering hold: adopt the target, step the next shape forward
      curShape = nextShape
      nextShape = (nextShape + 1) % NSHAPES
    }
  }
  blend = (holdFlag == 0) ? morphClock / HOLD_DUR : 0

  rot = rot + dt * (PI2 / 7)   // one full turn per ~7 s
  hueDrift = time(0.4)

  resetTransform()
  translate(-0.5, -0.5)        // display center -> origin
  rotate(rot)
}

// ---- SDF catalog (Inigo Quilez style) ----

function sdfCircle(x, y, r) {
  return sqrt(x * x + y * y) - r
}

function sdfBox(x, y, r) {
  var dx = abs(x) - r
  var dy = abs(y) - r
  var ax = max(dx, 0)
  var ay = max(dy, 0)
  return sqrt(ax * ax + ay * ay) + min(max(dx, dy), 0)
}

function sdfTriangle(x, y, r) {
  var k = 1.7320508                    // sqrt(3)
  var px = abs(x) - r
  var py = y + r / k
  if (px + k * py > 0) {
    var nx = (px - k * py) / 2
    var ny = (-k * px - py) / 2
    px = nx; py = ny
  }
  px = px - clamp(px, -2 * r, 0)
  return -sqrt(px * px + py * py) * sign(py)
}

function sdfHexagon(x, y, r) {
  var kx = -0.86602540, ky = 0.5, kz = 0.57735027
  var px = abs(x), py = abs(y)
  var d = 2 * min(kx * px + ky * py, 0)
  px = px - d * kx
  py = py - d * ky
  px = px - clamp(px, -kz * r, kz * r)
  py = py - r
  return sqrt(px * px + py * py) * sign(py)
}

function sdfHexagram(x, y, r) {
  var kx = -0.5, ky = 0.86602540, kz = 0.57735027, kw = 1.7320508
  var px = abs(x), py = abs(y)
  var t1 = 2 * min(kx * px + ky * py, 0)
  px = px - t1 * kx
  py = py - t1 * ky
  var t2 = 2 * min(ky * px + kx * py, 0)
  px = px - t2 * ky
  py = py - t2 * kx
  px = px - clamp(px, r * kz, r * kw)
  py = py - r
  return sqrt(px * px + py * py) * sign(py)
}

function sdfCross(x, y, r) {
  // arm length r, arm thickness a small fraction of it
  var bx = r, by = r * 0.32
  var px = abs(x), py = abs(y)
  if (py > px) { var tmp = px; px = py; py = tmp }
  var qx = px - bx
  var qy = py - by
  var k = max(qy, qx)
  var wx, wy
  if (k > 0) { wx = qx; wy = qy }
  else { wx = by - px; wy = -k }
  var mx = max(wx, 0), my = max(wy, 0)
  return sign(k) * sqrt(mx * mx + my * my)
}

function shapeSDF(idx, x, y, r) {
  if (idx == 0) return sdfCircle(x, y, r)
  else if (idx == 1) return sdfCross(x, y, r)
  else if (idx == 2) return sdfHexagram(x, y, r)
  else if (idx == 3) return sdfBox(x, y, r)
  else if (idx == 4) return sdfTriangle(x, y, r)
  else return sdfHexagon(x, y, r)
}

export function render2D(index, x, y) {
  var d = shapeSDF(curShape, x, y, size)
  if (blend > 0) {
    var dn = shapeSDF(nextShape, x, y, size)
    d = mix(d, dn, blend)      // lerp raw signed distances -> organic melt
  }

  var lit = (filled == 1) ? (d < lineWidth) : (abs(d) < lineWidth)
  if (!lit) {
    hsv(0, 0, 0)
    return
  }

  // brightness ramps down away from the boundary; interior (d<0) stays full
  var t = (filled == 1) ? d / lineWidth : abs(d) / lineWidth
  var bri = clamp(1 - t, 0, 1)
  bri = bri * bri                            // gamma

  var sat = clamp(1.2 - abs(d) / (size * 2), 0, 1)
  var hue = d + hueDrift                     // concentric drifting rainbow bands

  hsv(hue, sat, bri)
}
