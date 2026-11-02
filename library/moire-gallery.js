// name: Moiré Gallery
// tags: panel, ambient
// Original work from the prose idea specs docs/pattern-specs/patternflow/origin.md, tile-waves.md, glitch-rings.md and warped-wave.md (Patternflow presets, CC BY-SA 4.0 — prose-described ideas only, no code).
// A wall of little screens, each sampling a ripple (rings, a zone plate or a
// saddle) on a deliberately coarse grid at a rising frequency, so every tile
// aliases into its own moiré; the ladder drifts by itself through the sweet
// spots. Glitch Rings mode: one posterized ring field with a sine wobble and
// VHS row tears that snap and settle. Values go into a small canvas.

var out = array(4096), rad = array(4096)
var dr = array(256), dz = array(256), dp = array(256)
var mu = array(160), lut = array(512), tr = array(64), wob = array(64), tmp = array(3)
var tearR = array(2), tearH = array(2), tearA = array(2), tearT = [9, 9]
var pal = [0, 0, 0, 0, 0.3, 0.13, 0.13, 0.15, 0.68, 0, 0.5, 1, 1, 1, 1, 1]
setPalette(pal)

// 0 gallery · 1 glitch rings
var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1); lay = -1 }

// Gallery tiles per side: 0 two · 1 three · 2 four · 3 five, gapless
var layout = 1
//# min=0 max=3 step=1 default=1
export function sliderLayout(v) { layout = clamp(floor(v), 0, 3); lay = -1 }

// Rings per tile radius on the first tile (glitch rings: per panel radius).
var rings = 3
//# min=1 max=12 step=0.5 default=3
export function sliderRings(v) { rings = clamp(v, 1, 12) }

// Rings born per second; negative runs them inward.
var speed = 0.6
//# min=-3 max=3 step=0.1 default=0.6
export function sliderSpeed(v) { speed = clamp(v, -3, 3) }

// Colour between the grey and the white of the ramp, in degrees.
var hueDeg = 200
//# min=0 max=359 step=1 default=200
export function sliderHue(v) { hueDeg = mod(v, 360) }

// Glitch rings: wobble and how often rows tear, percent.
var glitch = 50
//# min=0 max=100 step=1 default=50
export function sliderGlitch(v) { glitch = clamp(v, 0, 100) }

var cw = 16, ch = 16, lay = -1, lw = 0, lh = 0, m = 6, nx = 2, ny = 2, ox = 0, oy = 0, g = 1
var t = 0, ph = 0, drift = 1, tick = 0

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  t = mod(t + dt, 3600)
  ph = mod(ph + dt * speed, 1)
  drift = 1 + 0.4 * sin(t * PI2 / 50)          // the ladder wanders on its own
  hsv2rgb(hueDeg / 360, 1, 1, tmp)
  pal[9] = tmp[0]; pal[10] = tmp[1]; pal[11] = tmp[2]
  // glitch rings: every quarter second a band of rows may tear
  var k = floor(t * 4)
  if (k != tick) {
    tick = k
    if (random(100) < glitch * 0.6) {
      var j = tearT[0] > tearT[1] ? 0 : 1
      tearR[j] = random(1); tearH[j] = 0.03 + random(0.12)
      tearA[j] = (random(1) < 0.5 ? -1 : 1) * (0.05 + random(0.2)); tearT[j] = 0
    }
  }
  tearT[0] += dt; tearT[1] += dt
}

function setup(gw, gh) {
  var short = min(gw, gh), a, b, i
  if (mode) {
    var B = ceil(max(gw, gh) / 64)
    cw = ceil(gw / B); ch = ceil(gh / B)
    for (b = 0; b < ch; b++) for (a = 0; a < cw; a++) {
      rad[b * cw + a] = 30 * hypot((a + 0.5) / cw * 2 - 1, (b + 0.5) / ch * 2 - 1)
    }
    for (i = 0; i < cw + 96; i++) mu[i] = mod(i - 48, cw)
    return
  }
  var p = max(1, round(short / 32))
  cw = floor(gw / p); ch = floor(gh / p)
  var n = layout + 2
  g = layout == 3 ? 0 : 1
  m = clamp(floor((min(cw, ch) - (n + 1) * g) / n), 1, 16)
  nx = floor((cw - g) / (m + g)); ny = floor((ch - g) / (m + g))
  ox = floor((cw - nx * (m + g) + g) / 2); oy = floor((ch - ny * (m + g) + g) / 2)
  var hm = m / 2
  for (b = 0; b < m; b++) for (a = 0; a < m; a++) {
    var x = a + 0.5 - hm, y = b + 0.5 - hm
    dr[b * m + a] = hypot(x, y)
    dz[b * m + a] = (x * x + y * y) / hm
    dp[b * m + a] = x * y / hm * 2
  }
  feedback(out, 0)
}

function gallery() {
  var tiles = nx * ny, base = PI2 * rings / (m / 2) * drift, phr = ph * PI2
  var step = tiles > 1 ? 5 / (tiles - 1) : 0
  for (var k = 0; k < tiles; k++) {
    var fr = base * (1 + k * step), kind = k % 3
    var tbl = kind == 0 ? dr : (kind == 1 ? dz : dp)
    var c0 = ox + (k % nx) * (m + g), r0 = oy + floor(k / nx) * (m + g)
    for (var b = 0; b < m; b++) {
      var o = (r0 + b) * cw + c0, tb = b * m
      for (var a = 0; a < m; a++) out[o + a] = 0.5 + 0.47 * sin(tbl[tb + a] * fr - phr)
    }
  }
}

function glitchRings() {
  var gl = glitch / 100, f = rings * PI2 / 30, phr = ph * PI2
  for (var j = 0; j < 512; j++) {          // radius (1/8 cell) -> cel-shaded band
    var w = sin(j * 0.125 * f - phr)
    lut[j] = w < -0.35 ? 0 : (w < -0.15 ? 0.3 : (w < 0.75 ? 0.68 : 1))
  }
  for (var c = 0; c < cw; c++) wob[c] = gl * 5 * sin(c * 6 / cw + t * 1.7)
  for (var r = 0; r < ch; r++) tr[r] = 0
  for (j = 0; j < 2; j++) {
    if (tearT[j] > 0.7) continue
    var off = round(tearA[j] * cw * (1 - easeOutElastic(max(0, tearT[j] - 0.25) / 0.45)))
    for (r = floor(tearR[j] * ch); r < min(ch, (tearR[j] + tearH[j]) * ch); r++) tr[r] += off
  }
  for (r = 0; r < ch; r++) {
    var o = r * cw, s0 = 48 - tr[r] - round(gl * cw * 0.04 * sin(r * 5 / ch + t * 1.3))
    for (c = 0; c < cw; c++) {
      var cc = mu[c + s0]
      out[o + c] = lut[clamp((rad[o + cc] + wob[cc]) * 8, 0, 511)]
    }
  }
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  if (lay < 0 || gw != lw || gh != lh) { lay = 1; lw = gw; lh = gh; setup(gw, gh) }
  if (mode) glitchRings()
  else gallery()
  paintCanvas(out, cw, ch)
}
