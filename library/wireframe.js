// name: Wireframe
// Curated original (hand-written showcase of the Luxel language/builtins).
// A vector solid tumbles in perspective — cube, octahedron, icosahedron,
// torus knot — and every few seconds its edges fly apart and re-dock as the
// next one. Each solid is 30 edges sorted by direction, so edge k morphs to
// edge k (a 12-edge solid repeats edges). Near edges are bright and thick,
// far ones dim and thin; Style 1 leaves a phosphor ghost, Style 2 adds a
// scrolling synthwave floor. renderBulk: <= 30 drawLine + ~20 for the floor.

var rpm = 14         // tumble speed, revolutions per minute
var hold = 5         // seconds each solid is held before morphing
var size = 70        // % of the panel the solid spans
var style = 2

const NE = 30        // edges per solid (the icosahedron's count)
var E = array(4 * NE * 6)   // 4 solids x 30 edges x (ax ay az bx by bz)
var V = array(36)            // vertex scratch while building
var K = array(NE)            // sort keys while building
var P = array(NE * 4)        // projected edges this frame
var Z = array(NE)            // mean depth per edge this frame

var ne = 0
function edge(ax, ay, az, bx, by, bz) {
  if (ne < NE) {
    var o = ne * 6
    T[o] = ax; T[o + 1] = ay; T[o + 2] = az; T[o + 3] = bx; T[o + 4] = by; T[o + 5] = bz
    ne++
  }
}
var T = array(NE * 6)        // one solid's raw edges before sorting
// Copy solid s's raw edges into E in direction order, repeating short solids.
function commit(s, scale) {
  var i, j, k, best, o
  for (i = 0; i < ne; i++) {
    o = i * 6
    K[i] = atan2(T[o + 2] + T[o + 5], T[o] + T[o + 3]) + 4 * (T[o + 1] + T[o + 4])
  }
  for (i = 0; i < ne; i++) {              // selection sort of T by K
    best = i
    for (j = i + 1; j < ne; j++) if (K[j] < K[best]) best = j
    for (k = 0; k < 6; k++) { var tmp = T[i * 6 + k]; T[i * 6 + k] = T[best * 6 + k]; T[best * 6 + k] = tmp }
    var tk = K[i]; K[i] = K[best]; K[best] = tk
  }
  for (i = 0; i < NE; i++) {
    o = floor(i * ne / NE) * 6
    for (k = 0; k < 6; k++) E[(s * NE + i) * 6 + k] = T[o + k] * scale
  }
  ne = 0
}

// cube: corners that differ in exactly one coordinate
var a, b, c
for (a = 0; a < 8; a++) for (b = a + 1; b < 8; b++) {
  c = (a ^ b)
  if (c == 1 || c == 2 || c == 4)
    edge((a & 1) * 2 - 1, (a >> 1 & 1) * 2 - 1, (a >> 2 & 1) * 2 - 1,
         (b & 1) * 2 - 1, (b >> 1 & 1) * 2 - 1, (b >> 2 & 1) * 2 - 1)
}
commit(0, 0.58)
// octahedron: every pair of axis points that is not opposite
for (a = 0; a < 6; a++) {
  V[a * 3] = 0; V[a * 3 + 1] = 0; V[a * 3 + 2] = 0
  V[a * 3 + floor(a / 2)] = (a & 1) ? -1 : 1
}
for (a = 0; a < 6; a++) for (b = a + 1; b < 6; b++)
  if (floor(a / 2) != floor(b / 2))
    edge(V[a * 3], V[a * 3 + 1], V[a * 3 + 2], V[b * 3], V[b * 3 + 1], V[b * 3 + 2])
commit(1, 1)
// icosahedron: cyclic permutations of (0, ±1, ±phi); edges are length 2
for (a = 0; a < 12; a++) {
  var p = floor(a / 4), s1 = (a & 1) ? -1 : 1, s2 = (a & 2) ? -1.618 : 1.618
  V[a * 3 + p] = 0; V[a * 3 + (p + 1) % 3] = s1; V[a * 3 + (p + 2) % 3] = s2
}
for (a = 0; a < 12; a++) for (b = a + 1; b < 12; b++) {
  var dx = V[a * 3] - V[b * 3], dy = V[a * 3 + 1] - V[b * 3 + 1], dz = V[a * 3 + 2] - V[b * 3 + 2]
  if (abs(dx * dx + dy * dy + dz * dz - 4) < 0.1)
    edge(V[a * 3], V[a * 3 + 1], V[a * 3 + 2], V[b * 3], V[b * 3 + 1], V[b * 3 + 2])
}
commit(2, 0.53)
// (2,3) torus knot as a closed 30-segment loop
var px = 0, py = 0, pz = 0
for (a = 0; a <= NE; a++) {
  var u = a / NE * PI2, rr = 0.62 + 0.3 * cos(3 * u)
  var qx = rr * cos(2 * u), qy = 0.42 * sin(3 * u), qz = rr * sin(2 * u)
  if (a > 0) edge(px, py, pz, qx, qy, qz)
  px = qx; py = qy; pz = qz
}
commit(3, 1.2)

var ry = 0, rx = 0, clock = 0, scroll = 0, hue = 0.83, pxu = 1 / 31
var cy, sy, cx, sx

export function beforeRender(delta) {
  var dt = delta * 0.001
  ry = mod(ry + rpm / 60 * PI2 * dt, PI2)
  rx = mod(rx + rpm / 60 * PI2 * 0.61 * dt, PI2)
  clock = mod(clock + dt / (hold + 1.5), 4)
  scroll = mod(scroll + dt * 0.6, 1)
  hue = mod(hue + dt * 0.02, 1)
  cy = cos(ry); sy = sin(ry); cx = cos(rx); sx = sin(rx)
  pxu = 1 / (gridWidth() > 1 ? gridWidth() - 1 : 31)    // one pixel, normalized
}

function drawFloor() {          // synthwave floor: converging rails + rows
  var i, z, y, n = pxu < 0.025 ? 5 : 3
  oklch(0.42, 0.2, 0.9)
  for (i = -n; i <= n; i++) drawLine(0.5 + i * 0.18 / n, 0.62, 0.5 + i * 1.8 / n, 1.2, pxu * 0.7, 2)
  for (i = 0; i < 6; i++) {
    z = 1 + 6 * pow(1 - frac((i + scroll) / 6), 1.6)
    y = 0.62 + 0.42 / z
    oklch((0.2 + 0.35 / z) * saturate((z - 1) * 4), 0.2, 0.9)
    drawLine(0, y, 1, y, pxu * 0.7, 2)
  }
  oklch(0.3, 0.12, 0.95)                          // horizon glow
  drawLine(0, 0.6, 1, 0.6, pxu * 5, 2)
  oklch(0.6, 0.16, 0.05)
  drawLine(0, 0.62, 1, 0.62, pxu, 2)
}

export function renderBulk() {
  if (style == 1) fade(0.72); else clear()
  if (style == 2) drawFloor()
  var s0 = floor(clock), s1 = (s0 + 1) % 4
  var m = easeInOutCubic(saturate((frac(clock) * (hold + 1.5) - hold) / 1.5))
  var sc = size * 0.0045 * (1 + 0.35 * sin(m * PI))     // swell mid-morph
  var oy = style == 2 ? 0.42 : 0.5
  var i, k, o0, o1, x, y, z, t, f, w
  for (i = 0; i < NE; i++) {
    o0 = (s0 * NE + i) * 6
    o1 = (s1 * NE + i) * 6
    for (k = 0; k < 2; k++) {
      x = mix(E[o0 + k * 3], E[o1 + k * 3], m)
      y = mix(E[o0 + k * 3 + 1], E[o1 + k * 3 + 1], m)
      z = mix(E[o0 + k * 3 + 2], E[o1 + k * 3 + 2], m)
      t = x * cy + z * sy; z = z * cy - x * sy; x = t          // yaw
      t = y * cx - z * sx; z = y * sx + z * cx; y = t          // pitch
      f = 2.6 / (2.6 + z)                                      // perspective
      P[i * 4 + k * 2] = 0.5 + x * f * sc
      P[i * 4 + k * 2 + 1] = oy + y * f * sc
      if (k == 0) Z[i] = z; else Z[i] = (Z[i] + z) * 0.5
    }
  }
  for (i = 0; i < NE; i++) {
    f = saturate(0.5 - Z[i] * 0.5)                  // 1 = nearest
    w = pxu * (0.8 + 0.9 * f)
    oklch(0.3 + 0.6 * f, 0.17 - 0.08 * f * f, hue + 0.12 * (1 - f) + 0.05 * i / NE)
    drawLine(P[i * 4], P[i * 4 + 1], P[i * 4 + 2], P[i * 4 + 3], w, 2)
  }
}

//# min=0 max=60 step=1 default=14
export function sliderSpeed(v) { rpm = clamp(v, 0, 60) }

//# min=1 max=20 step=0.5 default=5
export function sliderHold(v) { hold = clamp(v, 1, 20) }

//# min=30 max=100 step=1 default=70
export function sliderSize(v) { size = clamp(v, 30, 100) }

//# min=0 max=2 step=1 default=2
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }
