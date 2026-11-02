// name: Xorcery
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/xorcery-2d-3d.md,
// docs/pattern-specs/sierpinski-rainbow-2d.md — reimagined, not a replica; the Tapestry op is
// original work from the prose idea spec docs/pattern-specs/patternflow/retro-digital-tapestry.md
// (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// Bit logic as light. XOR: the fixed-point XOR of centred coordinates gives
// mirrored, self-similar blockwork that zooms and pulses. Sierpinski: x AND y
// carves the triangle in a seamless endless zoom (each finer level of holes
// fades in as the view doubles). Tapestry: coarse cells lit by stepped bit
// logic over graph-paper lines, washed by a soft travelling hue wave.

var op = 0
// XOR | Sierpinski zoom | Tapestry
//# min=0 max=2 step=1 default=0
export function sliderOp(v) { op = clamp(floor(v), 0, 2) }

var zoom = 4
//# min=1 max=16 step=0.5 default=4
export function sliderZoom(v) { zoom = clamp(v, 1, 16) }

var speed = 1
//# min=0.1 max=4 step=0.05 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.1, 4) }

var spread = 108 / 360
//# min=0 max=360 step=5 default=108
export function sliderColorSpreadDegrees(v) { spread = clamp(v, 0, 360) / 360 }

var RAINBOW = [0, 1, 0, 0, 1 / 6, 1, 1, 0, 2 / 6, 0, 1, 0, 3 / 6, 0, 1, 1, 4 / 6, 0, 0, 1, 5 / 6, 1, 0, 1, 1, 1, 0, 0]
var NEON = [0, 1, 0.1, 0.8, 0.3, 0.35, 0.1, 1, 0.55, 0, 0.85, 1, 0.8, 0.6, 1, 0.9, 1, 1, 0.1, 0.8]
var EMBER = [0, 0.85, 0.05, 0.02, 0.3, 1, 0.4, 0, 0.6, 1, 0.8, 0.25, 0.8, 1, 0.55, 0.3, 1, 0.85, 0.05, 0.02]
setPalette(RAINBOW)

var palMode = 0
// rainbow | neon | ember (all cyclic)
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) {
  palMode = clamp(floor(v), 0, 2)
  setPalette(palMode == 0 ? RAINBOW : (palMode == 1 ? NEON : EMBER))
}

var cellPx = 4
//# min=2 max=8 step=1 default=4
export function sliderTapestryCellPixels(v) { cellPx = clamp(floor(v), 2, 8) }

var hx = array(64)      // tapestry: per-column and per-row hue waves
var hy = array(64)
var t = 0
var s = 4
var inv = 0.25
var p = 0
var s1 = 16
var tick = 0
var rule = 0
var gw1 = 31
var gh1 = 31

export function beforeRender(delta) {
  t = time(0.06 / speed)
  s = zoom * (0.55 + 0.9 * triangle(time(0.3 / speed)))   // XOR zoom breathes
  inv = 1 / s
  var gw = gridWidth() > 0 ? gridWidth() : 32
  var gh = gridWidth() > 0 ? gridHeight() : 32
  gw1 = gw - 1
  gh1 = gh - 1
  // Sierpinski: the finest level is ~1 px at zoom phase 0 and doubles by 1
  p = time(0.12 / speed)
  s1 = pow(2, -p) * max(8, pow(2, floor(log2(max(gw, gh) / 2))))
  tick = floor(time(0.5 / speed) * 128)          // ~4 steps a second
  rule = floor(tick / 32) % 4                    // a new tapestry rule every ~8 s
  if (op == 2) {
    for (var i = 0; i < 64; i++) {
      hx[i] = wave(i * 0.07 - t)
      hy[i] = wave(i * 0.05 + t * 2)
    }
  }
}

export function render3D(index, x, y, z) {
  if (op != 0) { render2D(index, x, y); return }
  var v = (((x - 0.5) * s) ^ ((y - 0.5) * s) ^ ((z - 0.5) * s)) * inv
  var b = triangle(v * 1.5 + t)
  paint(v * spread + (x + y + z) * 0.35 + t, 0.05 + 0.95 * b * b)
}

export function render2D(index, x, y) {
  if (op == 0) {
    var v = (((x - 0.5) * s) ^ ((y - 0.5) * s)) * inv
    var b = triangle(v * 1.5 + t)
    paint(v * spread + (x + y) * 0.35 + t, 0.05 + 0.95 * b * b)
  } else if (op == 1) {
    // u & w == 0 is the triangle; (u & w) < 2 are the holes fading in
    var u = floor(abs(2 * x - 1) * s1 * 0.999)
    var w = floor((1 - y) * s1 * 0.999)
    var m = u & w
    paint((u * 0.5 + w) / s1 * spread - t, m == 0 ? 1 : (m < 2 ? 0.06 + 0.94 * (1 - p) : 0.06))
  } else {
    var px = round(x * gw1)
    var py = round(y * gh1)
    var c = floor(px / cellPx)
    var r = floor(py / cellPx)
    var k
    if (rule == 0) k = (c ^ r) + tick
    else if (rule == 1) k = ((c * 3) & (r * 3)) + tick * 2
    else if (rule == 2) k = (c * 3 + r * 5) ^ tick
    else k = floor((c * c + r * r) / 6) + tick
    if (k & 4) paint(hx[c % 64] * hy[r % 64] * spread * 2 + (k & 3) * 0.04 + t, 0.85)
    else if (px % cellPx == 0 || py % cellPx == 0) hsv(0.75, 0.7, 0.1)   // graph paper
    else rgb(0, 0, 0)
  }
}

// strip: a data stream — the pixel index XOR a clock that steps faster in
// each successive 16-pixel lane (six speeds, repeating; a whole-number speed
// of 8 would never flip bit 2 and park its lane)
export function render(index) {
  var k = floor(index / 2) ^ floor(tick * (1 + (floor(index / 16) % 6) * 0.5))
  paint(index / pixelCount * spread + t + (k & 3) * 0.05, k & 4 ? 1 : 0.06)
}
