// name: Lava Lamp
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/metaballs-of-fire-2d.md, sound-lavablob.md — reimagined, not a replica.
// A real lava lamp: wax blobs warm in the pool at the bottom, rise with a stretched
// neck, cool at the top and sink, merging and pinching apart inside a tapered glass.
// Temperature drives buoyancy. The field sum((r^2/d^2)^2) (no sqrt) is built per frame
// on a canvas of at most 32x48 cells from per-blob row/column tables and painted
// with one paintCanvas; the glass and the bulb's glow are a static brightness
// canvas. On a strip (index 0 = bottom) the same blobs rise and fall in a tube.

var MAXB = 8, U = 10              // field math runs in tenths of the height
var bx = array(MAXB), by = array(MAXB), vy = array(MAXB), tp = array(MAXB)
var br = array(MAXB), r2 = array(MAXB), dyr = array(MAXB), seed = array(MAXB)

var blobs = 6
//# min=3 max=8 step=1 default=6
export function sliderBlobs(v) { blobs = clamp(floor(v), 3, MAXB) }

var speed = 100                   // percent of the natural pace
//# min=25 max=400 step=5 default=100
export function sliderSpeedPercent(v) { speed = clamp(v, 25, 400) }

// 0 red in yellow | 1 purple in blue | 2 fire | 3 custom (the two pickers)
var palette = 0
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }

// 0 tapered lamp glass | 1 the whole panel
var shape = 0
//# min=0 max=1 step=1 default=0
export function sliderShape(v) { shape = clamp(floor(v), 0, 1) }

// How much the blobs swell with the bass (sensor board or microphone; no sound, no swell).
var swell = 50
//# min=0 max=100 step=1 default=50
export function sliderBassSwellPercent(v) { swell = clamp(v, 0, 100) }

export var frequencyData = array(32)

// palette positions: 0.15..0.28 liquid (brighter near wax), 0.33 the wax skin,
// 0.5..1 wax from the rim to the warmer core
var P0 = [0, 0.5, 0.25, 0, 0.15, 0.75, 0.4, 0, 0.28, 1, 0.62, 0.02, 0.33, 0.5, 0, 0,
  0.5, 1, 0.04, 0, 0.8, 1, 0.1, 0, 1, 1, 0.22, 0.02]
var P1 = [0, 0, 0.05, 0.3, 0.15, 0.02, 0.1, 0.5, 0.28, 0.05, 0.22, 0.8, 0.33, 0.3, 0, 0.4,
  0.5, 0.65, 0.02, 0.9, 0.8, 0.85, 0.08, 1, 1, 1, 0.3, 1]
var P2 = [0, 0, 0, 0, 0.15, 0.08, 0, 0, 0.28, 0.2, 0.02, 0, 0.33, 0.45, 0.02, 0,
  0.5, 0.9, 0.12, 0, 0.75, 1, 0.45, 0.02, 0.9, 1, 0.8, 0.2, 1, 1, 1, 0.75]
var P3 = [0, 0, 0, 0, 0.15, 0, 0, 0, 0.28, 0, 0, 0, 0.33, 0, 0, 0,
  0.5, 0, 0, 0, 0.8, 0, 0, 0, 1, 0, 0, 0]
var tmp = array(3)
var hW = 0.92, sW = 0.9, hL = 0.5, sL = 0.9
function stop(k, h, s, v) {
  hsv2rgb(h, s, v, tmp)
  P3[k * 4 + 1] = tmp[0]; P3[k * 4 + 2] = tmp[1]; P3[k * 4 + 3] = tmp[2]
}
function buildCustom() {
  stop(0, hL, sL, 0.3); stop(1, hL, sL, 0.55); stop(2, hL, sL, 0.8)
  stop(3, hW, sW, 0.5); stop(4, hW, sW, 1); stop(5, hW + 0.02, sW * 0.9, 1)
  stop(6, hW + 0.04, sW * 0.75, 1)
}
buildCustom()
// Custom wax and liquid colours (hue and saturation; the picker's brightness is ignored).
export function hsvPickerWaxColor(h, s, v) { hW = h; sW = s; buildCustom() }
export function hsvPickerLiquidColor(h, s, v) { hL = h; sL = s; buildCustom() }

// canvases (panel only; sized at the first frame and on a resize)
var CAPW = 32, CAPH = 48
var cv = array(CAPW * CAPH), bri = array(CAPW * CAPH)
var DX = array(MAXB * CAPW), DY = array(MAXB * CAPH), pool = array(CAPH)
var c0 = array(CAPH), c1 = array(CAPH)
var CW = 0, CH = 0, AX = 1, madeFor = -1

function glassHalf(y) { return shape ? AX / 2 : (0.25 + 0.15 * y) * min(AX, 1) }

function layout(w, h, ax) {
  CW = w; CH = h; AX = ax; madeFor = shape
  for (var r = 0; r < h; r++) {
    var y = (r + 0.5) / h, hg = glassHalf(y), lo = w, hi = -1
    var yp = 1.04 - y
    pool[r] = 0.0064 / (yp * yp)      // the wax pool lying on the bottom
    for (var c = 0; c < w; c++) {
      var x = (c + 0.5) / w * ax
      var m = shape ? 1 : clamp((hg - abs(x - ax / 2)) * w / ax + 0.5, 0, 1)
      bri[r * w + c] = m * (0.5 + 0.5 * y)   // the bulb lights the lamp from below
      if (m > 0) { lo = min(lo, c); hi = c }
    }
    c0[r] = lo; c1[r] = hi
  }
}

// start mid-cycle: some blobs rising, some resting, some sinking
for (var j = 0; j < MAXB; j++) {
  seed[j] = random(100)
  br[j] = 0.085 + random(0.045)
  by[j] = 0.15 + 0.8 * ((j * 0.618) % 1)
  tp[j] = (j * 0.29 + 0.2) % 1
  bx[j] = 0.5
  vy[j] = 0
}

var clock = 0, bass = 0, installed = -1

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000 * speed / 100
  clock = mod(clock + dt, 1000)
  var b = (frequencyData[0] + frequencyData[1] + frequencyData[2] + frequencyData[3]) / 4
  bass += (b - bass) * (1 - pow(0.5, delta / 60))
  var grow = 1 + swell / 100 * clamp(bass, 0, 1) * 1.6
  var drag = pow(0.25, dt)
  for (var j = 0; j < blobs; j++) {
    var y = by[j]
    // heat in the pool at the bottom (small blobs warm faster), cool everywhere,
    // and fast at the top
    if (y > 0.78) tp[j] += dt * 0.3 * (y - 0.78) / 0.22 * (0.1 / br[j])
    tp[j] -= dt * (0.025 + (y < 0.3 ? 0.15 * (0.3 - y) / 0.3 : 0))
    tp[j] = clamp(tp[j], 0, 1)
    vy[j] = vy[j] * drag - (tp[j] - 0.45) * 0.4 * dt
    by[j] = y + vy[j] * dt
    var top = br[j] * 0.7, bot = 0.97
    if (by[j] < top) { by[j] = top; vy[j] = max(vy[j], 0) }
    if (by[j] > bot) { by[j] = bot; vy[j] = min(vy[j], 0) }
    // wander sideways inside the glass
    var room = max(glassHalf(by[j]) - br[j] * 0.9, 0.02)
    var tx = AX / 2 + room * clamp(((j % 3) - 1) * 0.45 + simplex2(seed[j], clock * 0.05), -1, 1)
    bx[j] += (tx - bx[j]) * min(1, 0.6 * dt)
    r2[j] = br[j] * br[j] * grow * grow * U * U
  }
}

function install() {
  setPalette(palette == 0 ? P0 : palette == 1 ? P1 : palette == 2 ? P2 : P3)
  installed = palette
}

function panel() {
  var gw = gridWidth(), gh = gridHeight()
  var w = 32, h = 32, ax = 1
  if (gw > 0 && gh > 0) {
    w = ceil(gw / ceil(gw / CAPW)); h = ceil(gh / ceil(gh / CAPH)); ax = gw / gh
  }
  if (w != CW || h != CH || shape != madeFor) layout(w, h, ax)
  setBlur(gw > CW * 1.5 ? 0.5 : 0)   // soften an upscaled canvas's blocks
  var n = blobs
  for (var j = 0; j < n; j++) {
    var o = j * CW, k = (1 + 5 * abs(vy[j]))
    for (var c = 0; c < CW; c++) {
      var dx = ((c + 0.5) / CW * AX - bx[j]) * U
      DX[o + c] = dx * dx
    }
    o = j * CH
    for (var r = 0; r < CH; r++) {
      var dy = ((r + 0.5) / CH - by[j]) * U
      // the trailing side stretches into a neck, the leading side flattens
      var trail = dy * vy[j] < 0
      DY[o + r] = dy * dy * (trail ? 1 / (k * k) : k)
    }
  }
  for (var r = 0; r < CH; r++) {
    for (var j = 0; j < n; j++) dyr[j] = DY[j * CH + r] + 0.002
    var base = r * CW, pr = pool[r]
    for (var c = c0[r]; c <= c1[r]; c++) {
      var f = pr
      for (var j = 0; j < n; j++) {
        var q = min(r2[j] / (DX[j * CW + c] + dyr[j]), 8)
        f += q * q                  // (r/d)^4: blobs fuse when close, not across the lamp
      }
      // field -> palette position: liquid below 0.8, then the skin, then deeper
      // wax (1 - 1/f grows with depth; squared, only the very middle is core)
      if (f < 0.8) cv[base + c] = 0.15 + f * 0.16
      else {
        var g = max(0, 1 - 1 / f)
        cv[base + c] = 0.33 + 0.67 * g * g
      }
    }
  }
  paintCanvas(cv, CW, CH, bri)
}

// strip: one capped field array; each blob only touches its own window
var FN = 1024
var F = array(FN)
function strip() {
  setBlur(0)
  var n = min(pixelCount, FN), last = max(n - 1, 1)
  feedback(F, 0)
  for (var i = 0; i <= last * 0.25; i++) {   // the pool, bottom quarter
    var yp = 0.04 + i / last
    F[i] = 0.0064 / (yp * yp)
  }
  for (var j = 0; j < blobs; j++) {
    // a tube is narrow: blobs there are a quarter of their panel size
    var rr = r2[j] * 0.07, k = 1 + 5 * abs(vy[j])
    var c = (1 - by[j]) * last, win = sqrt(rr) / U * 3 * k * last   // (r/d)^4 < 1 % past it
    var lo = max(0, floor(c - win)), hi = min(last, ceil(c + win))
    for (var i = lo; i <= hi; i++) {
      var dy = (c - i) / last * U
      var q = min(rr / (dy * dy * (dy * vy[j] < 0 ? 1 / (k * k) : k) + 0.002), 8)
      F[i] += q * q
    }
  }
  var kk = n / pixelCount, db = 0.5 / max(pixelCount - 1, 1)
  for (var p = 0; p < pixelCount; p++) {
    var f = F[floor(p * kk)], g = max(0, 1 - 1 / f)   // same mapping as the panel
    paint(f < 0.8 ? 0.15 + f * 0.16 : 0.33 + 0.67 * g * g, 1 - p * db)
    setPixel(p)
  }
}

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

export function renderBulk() {
  if (palette != installed) install()
  if (has2DMap()) panel()
  else strip()
}
