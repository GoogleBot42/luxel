// name: Oil Slick
// tags: both, ambient
// Original work from the prose idea specs docs/pattern-specs/patternflow/oil-slick-glitch.md and sliding-strata.md (Patternflow presets, CC BY-SA 4.0 — prose-described ideas only, no code).
// Thin-film iridescence: a domain-warped simplex field cut into tight contour
// bands, coloured through an interference palette whose fringe spacing
// breathes over time. Glitch slices thin rows sideways with stutters. Strata
// mode shears noise bands past each other with dark seams. Panels: a canvas
// at half resolution painted by paintCanvas; strips: two noise rows.

var cv = array(4096), cb = array(4096)      // palette position, brightness
var wrow = array(64), bandOf = array(64), bandPos = array(64), tmp = array(3)
// cyclic thin-film palette: violet, magenta, gold, lime, cyan, blue, pink
var palO = [0, 0.35, 0.1, 0.7, 0.14, 0.85, 0.15, 0.55, 0.28, 0.95, 0.7, 0.2, 0.42, 0.35, 0.85, 0.3,
  0.57, 0.15, 0.7, 0.85, 0.71, 0.2, 0.3, 0.9, 0.85, 0.9, 0.4, 0.7, 1, 0.35, 0.1, 0.7]
var palS = array(32)
var hs = [0, 0.03, 0.07, 0.11], ss = [1, 0.95, 0.85, 0.5], vs = [0.3, 0.8, 1, 1]

// 0 oil slick · 1 sliding strata
var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

// Seconds for the liquid to melt into a new shape.
var flowS = 20
//# min=4 max=60 step=1 default=20
export function sliderFlowS(v) { flowS = clamp(v, 4, 60) }

// Contour bands across the slick (strata: bands of rock).
var bands = 7
//# min=2 max=12 step=1 default=7
export function sliderBands(v) { bands = clamp(floor(v), 2, 12); lay = 0 }

// Row-slice glitch, percent; 0 is calm liquid.
var glitch = 0
//# min=0 max=100 step=1 default=0
export function sliderGlitch(v) { glitch = clamp(v, 0, 100) }

// Strata: hue turn between alternate bands, in degrees.
var splitDeg = 150
//# min=0 max=180 step=1 default=150
export function sliderSplit(v) { splitDeg = clamp(v, 0, 180); strataPalette() }

// warm sandstone ramp in 0..0.48, the same ramp turned by Split in 0.52..1
function strataPalette() {
  for (var j = 0; j < 8; j++) {
    var q = j & 3, hi = j < 4 ? 0 : 1
    hsv2rgb(hs[q] + hi * splitDeg / 360, ss[q], vs[q], tmp)
    palS[j * 4] = hi * 0.52 + q * 0.16; palS[j * 4 + 1] = tmp[0]; palS[j * 4 + 2] = tmp[1]; palS[j * 4 + 3] = tmp[2]
  }
}
strataPalette()

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var cw = 32, ch = 32, lay = 0, z = 0, t = 0, ph = 0, fs = 0.25, ang = 0, kx = 0.05

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  t = mod(t + dt, 3600)
  z = mod(z + dt / flowS * 1.4, 1000)
  ph = mod(ph + dt / flowS, 1)
  ang = mod(ang + dt * 0.11, PI2)
  fs = 0.24 + 0.12 * sin(t * PI2 / 37)          // fringe spacing breathes
  setPalette(mode ? palS : palO)
}

function layout() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  var B = gw > 32 || gh > 32 ? 2 : 1
  B = max(B, ceil(max(gw, gh) / 64))
  if (lay && cw == ceil(gw / B) && ch == ceil(gh / B)) return
  cw = ceil(gw / B); ch = ceil(gh / B); kx = 1.7 / cw; lay = 1
  // strata of uneven height: hashed weights, normalised to the canvas
  var tot = 0, j, r
  for (j = 0; j < bands; j++) tot += 0.5 + hash(j * 3.1 + 2)
  var acc = 0, b = 0, edge = (0.5 + hash(2)) / tot * ch
  for (r = 0; r < ch; r++) {
    while (r + 0.5 > edge && b < bands - 1) { acc = edge; b++; edge += (0.5 + hash(b * 3.1 + 2)) / tot * ch }
    bandOf[r] = b
    bandPos[r] = (r + 0.5 - acc) / (edge - acc)
  }
}

// sideways shove of a thin slice of rows, with the odd stutter
function slice(r, n) {
  if (glitch == 0) return 0
  var s = floor(r * 14 / n), h = hash(s * 1.7)
  var d = max(0, sin(h * PI2 + t * 1.7) - 0.4) * (h < 0.5 ? -1 : 1) * glitch * 0.006
  return sin(t * 9 + s * 1.7) > 0.95 ? d * 4 : d
}

function oil() {
  var wa = 0.55, cs = cos(ang) * wa, sn = sin(ang) * wa, b2 = bands * 0.5
  for (var r = 0; r < ch; r++) {
    var y = r * kx, sx = slice(r, ch), o = r * cw
    fillNoise3D(wrow, cw, 1, kx, 0, sx + 40, y, z * 0.6, 3)
    for (var c = 0; c < cw; c++) {
      var w = wrow[c]
      var n = simplex3(c * kx + sx + w * cs, y + w * sn, z), k = (n + 1) * b2
      var tt = 1 - triangle(k)
      cv[o + c] = mod(k * fs + ph, 1)
      cb[o + c] = (0.3 + 0.7 * clamp(n + 0.8, 0, 1)) * (1 - tt * tt * 0.92)
    }
  }
}

function strata() {
  for (var r = 0; r < ch; r++) {
    var b = bandOf[r], dir = b & 1 ? -1 : 1
    var sh = dir * t * (0.05 + 0.08 * hash(b * 5.3)) + 0.5 * simplex2(b * 1.7, t * 0.15) + slice(r, ch)
    var bump = sin(PI * bandPos[r]), base = b & 1 ? 0.52 : 0, o = r * cw
    fillNoise3D(wrow, cw, 1, kx, 0, sh + b * 7, r * kx * 1.5, z, 5)
    for (var c = 0; c < cw; c++) {
      var n = wrow[c]
      cv[o + c] = base + 0.47 * clamp(n * 0.55 + 0.55, 0, 1)
      cb[o + c] = bump * clamp(n * 0.7 + 0.7, 0.2, 1)
    }
  }
}

function stripOut() {
  var n1 = min(pixelCount, 1024), k1 = 6 / n1, b2 = bands * 0.6, rr = n1 / pixelCount
  var st = pixelCount > 160 ? 2 : 1          // long strips shade pixel pairs
  if (mode == 0) {
    fillNoise3D(cv, n1, 1, k1, 0, 0, 0, z, 0)
    fillNoise3D(cb, n1, 1, k1 * 2.3, 0, 9, 0, z * 1.3, 1)
  }
  for (var i = 0; i < pixelCount; i += st) {
    var j = floor(i * rr)
    if (mode == 0) {
      var n = cv[j] + 0.5 * cb[j], k = (n + 1.5) * b2, tt = 1 - triangle(k)
      paint(mod(k * fs + ph, 1), (0.3 + 0.7 * clamp(n + 0.8, 0, 1)) * (1 - tt * tt * 0.92))
    } else {
      var seg = j * bands / n1, s = floor(seg), dir = s & 1 ? -1 : 1
      var nz = simplex2(j * k1 + dir * t * 0.6 + s * 7, z * 4, 5)
      paint((s & 1 ? 0.52 : 0) + 0.47 * clamp(nz * 0.6 + 0.5, 0, 1), sin(PI * (seg - s)) * clamp(nz * 0.8 + 0.6, 0.15, 1))
    }
    fillRange(i, i + st)
  }
}

export function renderBulk() {
  if (!has2DMap()) { stripOut(); return }
  layout()
  if (mode) strata()
  else oil()
  paintCanvas(cv, cw, ch, cb, gridWidth() > cw || gridHeight() > ch ? 1 : 0)   // bilinear only when upscaling
}
