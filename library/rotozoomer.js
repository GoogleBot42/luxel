// name: Rotozoomer
// tags: panel, retro
// Original work from the prose idea specs docs/pattern-specs/patternflow/poincare-sphere.md, sliding-checker.md (Patternflow presets, CC BY-SA 4.0 — prose-described ideas only, no code); Rotozoom is the classic demoscene effect.
// Three demo effects. Rotozoom: a tiled emblem spins and breathes in and out, hue
// sliding with depth. Fisheye: a scrolling neon grid seen through a roaming spherical
// lens whose rim folds into a mirror, lines kept about a pixel wide. Drawers: checker
// rows slide a tile in a staggered easeInOutBack cascade, swapping solid squares and
// hollow frames. Each frame's maths is folded into a few per-pixel multiply-adds.

var mode = 0                      // 0 rotozoom, 1 fisheye, 2 drawers
var speed = 1                     // x
var tiles = 6                     // grid lines / tiles across (fisheye, drawers)

//# min=0 max=2 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 2) }
//# min=0.2 max=3 step=0.1 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.2, 3) }
//# min=2 max=12 step=1 default=6
export function sliderTilesAcross(v) { tiles = clamp(floor(v), 2, 12) }

// The emblem: a ring and a hub over a two-tone checker (value, hue step).
var TV = array(256), TH = array(256)
for (var ty = 0; ty < 16; ty++) for (var tx = 0; tx < 16; tx++) {
  var d = hypot(tx - 7.5, ty - 7.5), i = ty * 16 + tx
  TV[i] = (floor(tx / 8) + floor(ty / 8)) % 2 ? 0.55 : 0.22
  TH[i] = TV[i] > 0.3 ? 0.08 : 0
  if (abs(d - 5.2) < 1.3) { TV[i] = 1; TH[i] = 0.48 }
  else if (d < 2.2) { TV[i] = 0.9; TH[i] = 0.16 }
}

var t = 0, hue0 = 0, aspX = 1, aspY = 1, half = 16, key = -1
var ua = 0, ub = 0, uc = 0, va = 0, vb = 0, vc = 0, hb = 0, pu = 0, pv = 0
var lax = 2, lbx = -1, lay = 2, lby = -1, k = 1, ka = 0, G = 9, sc = 0, sc2 = 0, ig = 1
var cyc = 0, rows = 6, cols = 6
var SH = array(64)                 // drawers: per-row shift, in tiles

export function beforeRender(delta) {
  var w = gridWidth(), h = gridHeight()
  if (w * 4096 + h != key) {
    key = w * 4096 + h
    var m = max(max(w, h), 1)
    aspX = w > 0 ? w / m : 1; aspY = h > 0 ? h / m : 1
    half = w > 0 ? m / 2 : 16
  }
  var dt = min(delta, 100) / 1000 * speed
  t = mod(t + dt, 3600)
  hue0 = mod(hue0 + dt / 40, 1)
  setGlow(mode == 1 ? 0.35 : 0)
  if (mode == 0) {
    // zoom eases from 1.1 px per texel up to an emblem 3/4 of the panel (3 px at
    // most); the spin wanders and reverses
    var zb = (1 - cos(t * 0.45)) / 2, zr = clamp(half * 0.0937, 1.1, 3) / 1.1
    var zs = 2 * half / pow(zr, easeInOutSine(zb)) / 1.1   // texels per screen unit
    var th = sin(t * 0.21) * 2.4 + t * 0.3, c = cos(th) * zs, s = sin(th) * zs
    pu = mod(pu + dt * 6, 256); pv = mod(pv + dt * 3.7, 256)  // drift, wrapped a hue lap
    ua = c * aspX; ub = -s * aspY; uc = pu - 0.5 * (ua + ub)
    va = s * aspX; vb = c * aspY; vc = pv - 0.5 * (va + vb)
    hb = hue0 + zb * 0.35
  } else if (mode == 1) {
    var cx = sin(t * 0.23) * 0.22, cy = sin(t * 0.17 + 2) * 0.18
    lax = 2 * aspX; lbx = -aspX - 2 * cx; lay = 2 * aspY; lby = -aspY - 2 * cy
    k = 1.7 + 0.9 * sin(t * 0.5)    // the lens breathes
    G = tiles * 1.5
    sc = mod(sc + dt * 0.5, 1); sc2 = mod(sc * 0.6, 1)
    ig = half / G / 1.4             // line-width scale: pixels per grid unit at the centre
    hb = 0.55 + 0.2 * wave(hue0 * 3)
  } else {
    cols = tiles * 0.9999; rows = max(2, round(tiles * aspY / aspX))
    cyc = mod(cyc + dt / 6, 2)       // two cascades (out, back) per 12 s
    for (var r = 0; r < rows && r < 64; r++) {
      var p = clamp((mod(cyc, 1) * 6 - r * 2.4 / rows) / 1.6, 0, 1)
      var e = easeInOutBack(p) * (r % 2 ? -1 : 1)
      SH[r] = cyc < 1 ? e : (r % 2 ? -1 : 1) - e
    }
    rows = rows * 0.9999
  }
}

export function render2D(index, x, y) {
  if (mode == 0) {
    var u = x * ua + y * ub + uc, v = x * va + y * vb + vc
    var i = (floor(v) & 15) * 16 + (floor(u) & 15)
    var val = TV[i]
    hsv(hb + TH[i] + (u + v) * 0.0039, 1 - val * 0.25, val)
  } else if (mode == 1) {
    x = x * lax + lbx; y = y * lay + lby
    var q = 1 + k * (x * x + y * y), s = 1 / q
    var dd = min(triangle(x * s * G + sc), triangle(y * s * G - sc2))  // 0 on a line
    var line = clamp(1.4 - dd * q * ig, 0, 1)      // q = 1/s: about a pixel wide everywhere
    hsv(hb + dd * 0.15, 1 - line * line * 0.75, line)
  } else {
    var fy = y * rows, row = floor(fy)
    var fx = x * cols + SH[row], col = floor(fx)
    var e2 = max(abs(fx - col - 0.5), abs(fy - row - 0.5))
    var hr = hue0 + row * 0.035
    if ((col + row) & 1) hsv(hr, 1, e2 < 0.3)
    else hsv(hr + 0.5, 0.25, (e2 > 0.38 && e2 < 0.47) * 0.9)
  }
}
