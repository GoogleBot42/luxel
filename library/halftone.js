// name: Halftone
// tags: panel, ambient
// Original work from the prose idea specs docs/pattern-specs/patternflow/pulse-dot-grid.md, wave-cell-grid.md, tri-march.md, glyph-mosaic.md and hatched-warp.md (Patternflow presets, CC BY-SA 4.0 — prose-described ideas only, no code).
// A flowing field (simplex noise plus a diagonal swell) sampled once per CELL
// and printed as shapes: halftone dots, squares, a marching triangle lattice,
// ASCII-style glyphs ordered by ink weight, or a three-ink riso print (solid,
// stipple, hatch). One fillNoise3D per frame, a few shape ops per cell.

var field = array(2304)       // one value per cell, up to 48 x 48 cells
var shape = 0
var cell = 8
var period = 6

// 0 dots · 1 squares · 2 triangles · 3 glyphs · 4 riso print
//# min=0 max=4 step=1 default=0
export function sliderShape(v) { shape = clamp(floor(v), 0, 4) }

//# min=4 max=16 step=1 default=8
export function sliderCellPx(v) { cell = clamp(floor(v), 4, 16) }

// Seconds for the diagonal swell to cross one panel.
//# min=2 max=20 step=0.5 default=6
export function sliderPeriodS(v) { period = clamp(v, 2, 20) }

var t = 0, z = 0, march = 0, fadeK = 1, x2 = 0, y2 = 0
var gw = 32, gh = 32, kx = 1 / 31, ky = 1 / 31

export function beforeRender(delta) {
  var dt = delta / 1000
  t = mod(t + dt / period, 1)
  z = mod(z + dt * 0.9 / period, 200)
  march = mod(march + dt * cell * 0.8, 9000)
  fadeK = pow(0.3, delta / 100)
}

function rect(x0, y0, x1, y1) { fillRect((x0 - 0.5) * kx, (y0 - 0.5) * ky, (x1 + 0.5) * kx, (y1 + 0.5) * ky) }
function line(x0, y0, x1, y1, w) { drawLine(x0 * kx, y0 * ky, x1 * kx, y1 * ky, w * kx, 2) }

// colour: two OKLCH families on a checkerboard, brighter with the value
function ink(v, odd) {
  if (odd) oklch(0.5 + 0.4 * v, 0.15, 0.43 + 0.1 * v)
  else oklch(0.55 + 0.37 * v, 0.17, 0.12 + 0.13 * v)
}

// field value of cell (c, r): noise plus a diagonal swell, 0..1
function val(n, c, r, nc) {
  return clamp(0.5 + 0.45 * n + 0.32 * sin(PI2 * ((c + r) * 0.5 / nc - t)), 0, 1)
}

function tri(cx, ytop, w, h, up, sv) {
  var yc = ytop + (up ? h * 2 / 3 : h / 3)
  var ya = yc - (up ? h * 2 / 3 : h / 3) * sv
  var yb = yc + (up ? h / 3 : h * 2 / 3) * sv
  var hw = w / 2 * sv
  for (var py = ceil(ya - 0.5); py <= floor(yb + 0.5); py++) {
    var f = clamp((py - ya) / (yb - ya), 0, 1)
    var half = (up ? f : 1 - f) * hw
    if (half > 0.3) fillRect((cx - half) * kx, (py - 0.5) * ky, (cx + half) * kx, (py + 0.5) * ky)
  }
}

export function renderBulk() {
  gw = gridWidth()
  gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / max(gw - 1, 1)
  ky = 1 / max(gh - 1, 1)
  var big = max(gw, gh)
  var c, r, v, x, y, i
  if (shape == 2) { drawTriangles(big); return }
  var cs = shape == 4 ? max(3, round(cell / 2)) : cell
  var nc = min(ceil(gw / cs), 48), nr = min(ceil(gh / cs), 48)
  var ox = floor((gw - nc * cs) / 2), oy = floor((gh - nr * cs) / 2)
  var sc = cs * 2.4 / big
  fillNoise3D(field, nc, nr, sc, sc, 0, 0, z, 5)
  if (shape >= 3) clear()
  else fade(fadeK)
  var h = (cs - 1) / 2
  for (r = 0; r < nr; r++) for (c = 0; c < nc; c++) {
    v = val(field[r * nc + c], c, r, nc)
    x = ox + c * cs
    y = oy + r * cs
    var odd = (c + r) % 2
    if (shape == 0) {
      if (v < 0.12) continue
      ink(v, odd)
      fillCircle((x + h) * kx, (y + h) * ky, (cs * 0.62 * sqrt(v)) * kx)
      if (v > 0.6) { oklch(1, 0, 0); splat((x + h) * kx, (y + h) * ky, cs * (v - 0.45) * kx, 2) }
    } else if (shape == 1) {
      if (v < 0.1) continue
      var e = h * (0.3 + 0.7 * v)
      oklch(0.6 + 0.4 * v, 0.06, odd ? 0.5 : 0.2)   // pale pane frame…
      rect(x + h - e, y + h - e, x + h + e, y + h + e)
      ink(v * 0.85, odd)                             // …around a saturated fill
      if (e > 1.4) rect(x + h - e + 1, y + h - e + 1, x + h + e - 1, y + h + e - 1)
    } else if (shape == 3) {
      var lv = floor(v * 6.99)
      if (lv == 0) continue
      oklch(0.42 + 0.09 * lv, 0.16, 0.8 - 0.1 * lv)
      if (lv == 1) rect(x + h - 0.4, y + h - 0.4, x + h + 0.4, y + h + 0.4)
      else if (lv == 2) line(x + 0.5, y + cs - 1.5, x + cs - 1.5, y + 0.5, 0.9)
      else if (lv == 3) { rect(x + h - 0.4, y + 1, x + h + 0.4, y + cs - 2); rect(x + 1, y + h - 0.4, x + cs - 2, y + h + 0.4) }
      else if (lv == 4) { line(x + 1, y + 1, x + cs - 2, y + cs - 2, 0.9); line(x + 1, y + cs - 2, x + cs - 2, y + 1, 0.9) }
      else if (lv == 5) { rect(x + 1, y + 1, x + cs - 2, y + cs - 2); hsv(0, 0, 0); rect(x + 2, y + 2, x + cs - 3, y + cs - 3) }
      else rect(x, y, x + cs - 2, y + cs - 2)
    } else {
      if (v < 0.36) { oklch(0.68, 0.24, 0.98); rect(x, y, x + cs - 1, y + cs - 1) }
      else if (v < 0.47) continue
      else if (v < 0.66) {
        oklch(0.75, 0.13, 0.52)
        for (y2 = y + mod(-y, 3); y2 < y + cs; y2 += 3) for (x2 = x + mod(-x, 3); x2 < x + cs; x2 += 3) rect(x2, y2, x2, y2)
      } else {
        oklch(0.92, 0.19, 0.29)
        for (i = mod(-(x + y), 4); i < 2 * cs - 1; i += 4) {
          var a0 = max(0, i - (cs - 1)), a1 = min(i, cs - 1)
          line(x + a0, y + i - a0, x + a1, y + i - a1, 0.75)
        }
      }
    }
  }
}

// a lattice of up/down triangles marching left, each scaled by its field value
function drawTriangles(big) {
  var th = cell, tw = cell * 1.1547, step = tw / 2
  var k0 = floor(march / step)
  var nt = min(ceil(gw / step) + 3, 48), nr = min(ceil(gh / th), 48)
  var sh = mod(march, step)
  var sc = th * 2.4 / big
  fillNoise3D(field, nt, nr, step / th * sc, sc, k0 * step / th * sc, 0, z, 5)
  fade(fadeK)
  for (var r = 0; r < nr; r++) for (var j = 0; j < nt; j++) {
    var v = val(field[r * nt + j], j, r, nt)
    var up = (j + k0 + r) % 2 == 0
    ink(v, up ? 0 : 1)
    tri((j - 1) * step - sh, r * th, tw, th, up, 0.35 + 0.65 * v)
  }
}
