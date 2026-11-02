// name: tixy
// tags: both, retro
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/tixy.md, golden-tix.md — reimagined, not a replica.
// A tixy.land-style reel of tiny formulas f(t, i, x, y) -> -1..1 on a coarse grid of
// cells: Dots draws each cell as a disc sized by |value| (white for +, red for -, or
// your picker colours), Tiles fills the cells, Rainbow maps the value to hue. On a
// strip the reel switches to formulas of (t, i, p) — the first is the accelerating
// moire. Every formula is evaluated once per CELL (not per pixel); dots are
// fillCircles over a fading frame, tiles one fillCanvas.

var MAXC = 32
// -1 plays the reel; 0..19 holds one formula (a strip uses 0..7, wrapping).
var formula = -1
//# min=-1 max=19 step=1 default=-1
export function sliderFormula(v) { formula = clamp(floor(v), -1, 19); t = 0 }

var sketchSec = 8
//# min=3 max=60 step=1 default=8
export function sliderSketchSeconds(v) { sketchSec = clamp(v, 3, 60) }

// cells across the panel
var cells = 16
//# min=8 max=32 step=1 default=16
export function sliderCells(v) { cells = clamp(floor(v), 8, MAXC) }

// formula time, x realtime
var speed = 1
//# min=0.25 max=4 step=0.05 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.25, 4) }

// 0 dots, 1 tiles, 2 rainbow tiles
var style = 0
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }

// Picker hue and saturation are used; V is ignored (brightness is global).
var posH = 0, posS = 0, negH = 0, negS = 1
export function hsvPickerPositiveColor(h, s, v) { posH = h; posS = s }
export function hsvPickerNegativeColor(h, s, v) { negH = h; negS = s }

var ch = array(MAXC * MAXC), cs = array(MAXC * MAXC), cvv = array(MAXC * MAXC)
var t = 0, cur = 0, clock = 0, keep = 0.5, hueOff = 0
var N = 16, M = 16, ox = 0, oy = 0, sx = 1 / 16, sy = 1 / 16

function f2(n, t, i, x, y) {
  var cx = x - (N - 1) / 2, cy = y - (M - 1) / 2
  if (n == 0) return sin(x * 0.6 + t * 2.5 + sin(y * 0.45 + t * 1.7) * 1.5)      // swell
  if (n == 1) return sin(t * 1.5 + (x + y) * 0.4) * cos(t - x * 0.3)              // beats
  if (n == 2) return mod(floor(x / 2 + t * 1.5) + floor(y / 2 - t), 2) * 2 - 1    // checker drift
  if (n == 3) return sin(y / 2 - t * 4)                                           // falling bars
  if (n == 4) return max(-0.12, 1 - abs(cx * cos(t) + cy * sin(t)) / 1.5)         // spinning bar
  if (n == 5) return sin(max(abs(cx), abs(cy)) * 0.8 - t * 4)                     // square tunnel
  if (n == 6) return sin((abs(cx) + abs(cy)) * 0.6 - t * 3)                       // diamonds
  if (n == 7) return ((floor(t * 4) + y * 7) >> mod(x, 16)) & 1 ? 1 : -0.15       // binary counters
  if (n == 8) return (mod(x, 5) && mod(y, 5)) ? sin(t * 2 + hash2(floor(x / 5), floor(y / 5)) * 6) : 0  // panes
  if (n == 9) return max(-0.1, 1 - hypot(cx - N * 0.33 * sin(t * 1.3), cy - M * 0.33 * sin(t * 1.9)) / 3)  // ball
  if (n == 10) {                                                   // rain
    var d = mod(y - t * (5 + hash(x) * 7) - hash(x + 9) * M, M + 4)
    return d < 5 ? 1 - d / 5 : -0.1
  }
  if (n == 11) return (sin(x * 0.4 + t) + sin(y * 0.35 - t * 1.3) + sin((x + y) * 0.25 + t * 0.7)) / 3  // plasma
  if (n == 12) {                                                  // heartbeat
    var hx = cx / (N * 0.3), hy = -cy / (M * 0.3) + 0.25
    var k = hx * hx + hy * hy - 1, beat = 0.55 + 0.45 * pow(abs(sin(t * 3)), 6)
    return k * k * k - hx * hx * hy * hy * hy < 0 ? beat : -0.08
  }
  if (n == 13) {                                                  // snake
    var s = mod(floor(t * 25) - i, N * M)
    return s < 18 ? 1 - s / 24 : -0.06
  }
  if (n == 14) return max(-0.1, 1.2 - abs(y - (M - 1) / 2 - sin(x * 0.5 - t * 3) * M * 0.35))  // sine
  if (n == 15) return mod((x ^ y) + floor(t * 6), 16) / 8 - 1                     // xor
  if (n == 16) return sin((x - y) / 2 + t * 3) * cos(t + x / 5)                   // weave
  if (n == 17) return (mod(floor(x / 4) + floor(y / 4), 2) * 2 - 1) * sin(t * 2)  // see-saw
  if (n == 18) return hash2(i, floor(t * 6)) > 0.85 ? 1 : -0.05                   // sparkle
  var ca = cos(t * 0.8), sa = sin(t * 0.8)                        // 19: turning cross
  return (abs(cx * ca + cy * sa) < 0.9 || abs(cy * ca - cx * sa) < 0.9) ? 1 : -0.1
}

function f1(n, t, i, p) {
  if (n == 0) return sin(i * t * 0.4)                                             // accelerating moire
  if (n == 1) return sin(p * 12 - t * 4)                                          // travelling waves
  if (n == 2) return ((floor(t * 8 + hash(floor(i / 32)) * 256)) >> mod(floor(i / 4), 8)) & 1 ? 1 : -0.1  // byte counters
  if (n == 3) return sin(abs(p - 0.5) * 30 - t * 6) * (0.4 + 0.6 * pow(abs(sin(t * 2.5)), 4))  // heartbeat
  if (n == 4) {                                                   // two scanners
    var a = abs(p - triangle(t / 3)) * 12, b = abs(p - triangle(t / 5 + 0.5)) * 12
    return a < 1 ? 1 - a : (b < 1 ? b - 1 : 0)
  }
  if (n == 5) return sin(i * i * 0.002 + t * 2)                                   // chirp
  if (n == 6) return hash2(i, floor(t * 8)) > 0.88 ? 1 : -0.04                   // sparkle
  return sin(i / 3 + sin(t) * i / 10)                                             // breathing comb
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  t += dt * speed
  clock += dt
  if (formula >= 0) cur = formula
  else if (clock >= sketchSec) { clock = 0; t = 0; cur = mod(cur + 1, 20) }
  hueOff = mod(hueOff + dt / 30, 1)
  keep = pow(0.5, delta / 45)                         // dot trails: half-life 45 ms
  var gw = gridWidth(), gh = gridHeight()
  N = cells
  M = gw > 0 ? clamp(round(cells * gh / gw), 1, MAXC) : cells
  // cell centres in map units (a grid's x is col / (w - 1))
  sx = gw > 1 ? gw / N / (gw - 1) : 1 / N
  sy = gh > 1 ? gh / M / (gh - 1) : 1 / M
  ox = gw > 1 ? -0.5 / (gw - 1) : 0
  oy = gh > 1 ? -0.5 / (gh - 1) : 0
}

function paintVal(v, b) {
  if (v >= 0) hsv(posH, posS, b)
  else hsv(negH, negS, b)
}

export function renderBulk() {
  if (!has2DMap()) {
    var n = pixelCount, k = mod(cur, 8)
    for (var i = 0; i < n; i++) {
      var v = clamp(f1(k, t, i, i / n), -1, 1)
      paintVal(v, v * v)
      setPixel(i)
    }
    return
  }
  if (style == 0) {
    fade(keep)
    var r0 = 0.46 * sx
    for (var y = 0; y < M; y++) for (var x = 0; x < N; x++) {
      v = clamp(f2(cur, t, y * N + x, x, y), -1, 1)
      var a = abs(v)
      if (a < 0.06) continue
      paintVal(v, 0.4 + 0.6 * a)
      fillCircle((x + 0.5) * sx + ox, (y + 0.5) * sy + oy, r0 * a)
    }
  } else {
    for (y = 0; y < M; y++) for (x = 0; x < N; x++) {
      i = y * N + x
      v = clamp(f2(cur, t, i, x, y), -1, 1)
      if (style == 2) { ch[i] = v * 0.4 + hueOff; cs[i] = 1; cvv[i] = 0.25 + 0.75 * abs(v) }
      else { ch[i] = v >= 0 ? posH : negH; cs[i] = v >= 0 ? posS : negS; cvv[i] = v * v }
    }
    fillCanvas(ch, cs, cvv, N, M)
  }
}

// a strip keeps the default index map (no grid): renderBulk's 1-D branch runs
export function render(index) {}
