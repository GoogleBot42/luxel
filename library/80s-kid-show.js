// name: 80s Kid Show
// tags: panel, retro, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/80s-kid-show.md — reimagined, not a replica.
// A Saturday-morning title card: Memphis-style shapes (discs, targets,
// triangles, squares, crosses, stars, squiggles, zigzags) in teal, hot pink,
// yellow, purple and coral drift, bounce, spin and breathe over an indigo
// backdrop, each with a dark outline and a drop shadow. renderBulk only:
// polygons are filled one row span at a time, lines are drawLine capsules.

var POOL = 18
var px = array(POOL), py = array(POOL), vx = array(POOL), vy = array(POOL)
var rot = array(POOL), spin = array(POOL), kind = array(POOL), col = array(POOL)
var sizeMul = array(POOL), brPh = array(POOL), brRate = array(POOL), order = array(POOL)
var qx = array(8), qy = array(8)     // vertex scratch for the polygon filler
var ROWS = 256, xl = array(ROWS), xr = array(ROWS)

var count = 9
var sizePct = 16
var speed = 0.12

//# min=3 max=18 step=1 default=9
export function sliderCount(v) { count = clamp(floor(v), 3, POOL) }

// Typical shape radius as a percentage of the panel.
//# min=5 max=40 step=1 default=16
export function sliderSizePct(v) { sizePct = clamp(v, 5, 40) }

// Drift speed in panel widths per second.
//# min=0.02 max=0.5 step=0.01 default=0.12
export function sliderSpeedPanelsS(v) { speed = clamp(v, 0.02, 0.5) }

for (var i = 0; i < POOL; i++) {
  px[i] = random(1)
  py[i] = random(1)
  var a = random(PI2)
  vx[i] = cos(a)
  vy[i] = sin(a)
  rot[i] = random(PI2)
  spin[i] = (random(1) - 0.5) * 2.4
  kind[i] = i % 8
  col[i] = (i * 3) % 5
  sizeMul[i] = 0.7 + random(0.6)
  brPh[i] = random(PI2)
  brRate[i] = 0.4 + random(1.2)
  order[i] = i
}

var gw = 32, gh = 32, kx = 1 / 31, ky = 1 / 31, unit = 32

function ink(c) {
  if (c == 0) oklch(0.76, 0.13, 0.52)        // teal
  else if (c == 1) oklch(0.66, 0.25, 0.99)   // hot pink
  else if (c == 2) oklch(0.9, 0.17, 0.29)    // yellow
  else if (c == 3) oklch(0.58, 0.22, 0.8)    // purple
  else oklch(0.72, 0.18, 0.08)               // coral
}
function dark() { oklch(0.1, 0.04, 0.8) }

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  for (var i = 0; i < count; i++) {
    px[i] += vx[i] * speed * dt
    py[i] += vy[i] * speed * dt
    if (px[i] < 0.03) { px[i] = 0.03; vx[i] = abs(vx[i]) }
    if (px[i] > 0.97) { px[i] = 0.97; vx[i] = -abs(vx[i]) }
    if (py[i] < 0.03) { py[i] = 0.03; vy[i] = abs(vy[i]) }
    if (py[i] > 0.97) { py[i] = 0.97; vy[i] = -abs(vy[i]) }
    rot[i] = mod(rot[i] + spin[i] * dt, PI2)
    brPh[i] = mod(brPh[i] + brRate[i] * dt, PI2)
  }
  if (random(1) < dt / 3) {                 // now and then two shapes trade places in the stack
    var a = floor(random(count)), b = floor(random(count)), tmp = order[a]
    order[a] = order[b]
    order[b] = tmp
  }
}

// fill the convex polygon in qx/qy (pixel space, n vertices): walk each edge
// once into per-row left/right bounds, then one fillRect per row span
function fillPoly(n) {
  var y0 = 9999, y1 = -9999, i, y
  for (i = 0; i < n; i++) { y0 = min(y0, qy[i]); y1 = max(y1, qy[i]) }
  y0 = max(ceil(y0), 0)
  y1 = min(floor(y1), min(gh - 1, ROWS - 1))
  if (y1 < y0) return
  for (y = y0; y <= y1; y++) { xl[y] = 9999; xr[y] = -9999 }
  for (i = 0; i < n; i++) {
    var j = i == n - 1 ? 0 : i + 1
    var ya = qy[i], yb = qy[j], xa = qx[i], xb = qx[j]
    if (yb < ya) { var t = ya; ya = yb; yb = t; t = xa; xa = xb; xb = t }
    var dx = yb > ya ? (xb - xa) / (yb - ya) : 0
    for (y = max(ceil(ya), y0); y <= min(yb, y1); y++) {
      var x = xa + (y - ya) * dx
      if (x < xl[y]) xl[y] = x
      if (x > xr[y]) xr[y] = x
    }
  }
  for (y = y0; y <= y1; y++) if (xr[y] >= xl[y]) fillRect((xl[y] - 0.5) * kx, (y - 0.5) * ky, (xr[y] + 0.5) * kx, (y + 0.5) * ky)
}

function ngon(cx, cy, r, n, a) {
  for (var i = 0; i < n; i++) {
    qx[i] = cx + r * cos(a + i * PI2 / n)
    qy[i] = cy + r * sin(a + i * PI2 / n)
  }
  fillPoly(n)
}

function bar(cx, cy, hl, ht, a) {             // rotated rectangle
  var c = cos(a), s = sin(a)
  qx[0] = cx + c * hl - s * ht; qy[0] = cy + s * hl + c * ht
  qx[1] = cx - c * hl - s * ht; qy[1] = cy - s * hl + c * ht
  qx[2] = cx - c * hl + s * ht; qy[2] = cy - s * hl - c * ht
  qx[3] = cx + c * hl + s * ht; qy[3] = cy + s * hl - c * ht
  fillPoly(4)
}

// one shape, grown by e pixels (the outline pass draws it fat and dark)
function shape(k, cx, cy, r, a, e, i) {
  if (k == 0 || k == 7) fillCircle(cx * kx, cy * ky, (r + e) * kx)
  else if (k == 1) ngon(cx, cy, r * 1.15 + 2 * e, 3, a)
  else if (k == 2) ngon(cx, cy, r + 1.4 * e, 4, a)
  else if (k == 3) { bar(cx, cy, r + e, r * 0.32 + e, a); bar(cx, cy, r * 0.32 + e, r + e, a) }
  else if (k == 4) { ngon(cx, cy, r * 1.1 + 2 * e, 3, a); ngon(cx, cy, r * 1.1 + 2 * e, 3, a + PI) }
  else {
    var w = max(1.5, r * 0.26) + e, n = 5, x0 = 0, y0 = 0, c = cos(a), s = sin(a)
    for (var j = 0; j <= n; j++) {
      var u = (j / n - 0.5) * 2.4 * r
      var v = k == 5 ? r * 0.35 * sin(j * 1.9 + brPh[i] * 2) : (j % 2 ? 0.35 : -0.35) * r
      var x1 = cx + c * u - s * v, y1 = cy + s * u + c * v
      if (j > 0) drawLine(x0 * kx, y0 * ky, x1 * kx, y1 * ky, w * kx, 2)
      x0 = x1
      y0 = y1
    }
  }
}

export function renderBulk() {
  gw = gridWidth()
  gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / max(gw - 1, 1)
  ky = 1 / max(gh - 1, 1)
  unit = min(gw, gh)
  fillGradient(0.7, 0.85, 0.13, 0.8, 0.8, 0.24, 2)   // indigo backdrop…
  var sh = max(1, round(unit / 48)), d = 10 * sh
  oklch(0.3, 0.09, 0.85)                              // …with a Memphis dot grid
  for (var gy = 3 * sh; gy < gh; gy += d) for (var gx = 3 * sh + (floor(gy / d) % 2) * d / 2; gx < gw; gx += d) {
    fillRect((gx - 0.5) * kx, (gy - 0.5) * ky, (gx + sh - 0.5) * kx, (gy + sh - 0.5) * ky)
  }
  for (var n = 0; n < count; n++) {
    var i = order[n]
    if (i >= count) continue
    var cx = px[i] * (gw - 1), cy = py[i] * (gh - 1)
    var r = unit * sizePct / 100 * sizeMul[i] * (0.8 + 0.2 * sin(brPh[i]))
    var k = kind[i], line = k == 5 || k == 6
    dark()                                     // one fat dark copy: outline + drop shadow
    shape(k, cx + 0.8 * sh, cy + 0.8 * sh, r, rot[i], line ? 0 : 1.6 * sh, i)
    ink(col[i])
    shape(k, cx, cy, r, rot[i], 0, i)
    if (k == 7) { ink((col[i] + 2) % 5); fillCircle(cx * kx, cy * ky, r * 0.5 * kx) }   // target
  }
}
