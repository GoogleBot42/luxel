// name: Scanner
// tags: both, retro
// Clean-room reimplementation from the prose specs docs/pattern-specs/scanner.md, cylon.md,
// kitt.md, kitt-w-color-picker.md — reimagined, not a replica.
// The sweeping eye: a hot-cored head glides end to end and leaves a glowing wake.
// One eye, a mirrored pair that crosses in the middle, or two eyes bouncing in
// their own halves; sine-eased turnarounds and an optional tempo lock that
// lands the eye on the ends and swells it on every beat. No buffers: the frame
// persists, fade() is the wake, the span swept since last frame is filled
// anti-aliased so fast sweeps never break up. On a panel it is a vertical bar.

var pickH = 0, pickS = 1
// Eye colour (hue and saturation; the picker's brightness is ignored).
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s }

var rainbow = 0
// Colour the wake by position along the strip instead of the picker colour.
//# default=0
export function toggleRainbow(v) { rainbow = v }

var eyes = 0
// 0 one eye, 1 mirrored pair crossing in the middle, 2 an eye in each half
//# min=0 max=2 step=1 default=0
export function sliderEyes(v) { eyes = clamp(floor(v), 0, 2) }

var sweepS = 2
// Seconds for one end-to-end pass.
//# min=0.3 max=10 step=0.1 default=2
export function sliderSweepSeconds(v) { sweepS = clamp(v, 0.3, 10) }

var tailS = 0.6
// Seconds for the wake to fade to 5 %.
//# min=0.05 max=3 step=0.05 default=0.6
export function sliderTailSeconds(v) { tailS = clamp(v, 0.05, 3) }

var widthPx = 3
// Width of the eye in pixels (panel: columns).
//# min=1 max=20 step=1 default=3
export function sliderWidth(v) { widthPx = clamp(v, 1, 20) }

var ease = 1
// Slow down into the turnarounds like a pendulum.
//# default=1
export function toggleEase(v) { ease = v }

var bpmLock = 0
// Lock the sweep to a tempo: the eye hits an end every two beats and swells on each beat.
//# default=0
export function toggleBpmLock(v) { bpmLock = v }

var bpm = 120
//# min=40 max=200 step=1 default=120
export function sliderBpm(v) { bpm = clamp(v, 40, 200) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var ph = 0, p = 0, keep = 0.9, thump = 0, drift = 0
var prev0 = -1, prev1 = -1

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  keep = pow(0.05, dt / tailS)
  drift = mod(drift + dt / 30, 1)
  if (bpmLock) {
    ph = beat(bpm / 4)                 // one round trip per four beats
    var b = beat(bpm)
    thump = (1 - b) * (1 - b)
  } else {
    ph = mod(ph + dt / (2 * sweepS), 1)
    thump = 0
  }
  p = triangle(ph)
  if (ease) p = (1 - cos(p * PI)) / 2
}

var bh = 0, bs = 1
function tint(u) {
  if (rainbow) { bh = drift + u; bs = 1 }
  else { bh = pickH; bs = pickS }
}

// one eye on the strip at pixel position x, coming from px
function eye1D(x, px, n) {
  var w = widthPx * (1 + thump)
  tint(x / n)
  if (px >= 0 && abs(x - px) < n / 2) {
    // the span crossed this frame, already as faded as a frame-old wake
    gradientRange(px + 0.5, x + 0.5, bh, bs, keep, bh, bs, 1, 2)
  }
  hsv(bh, bs, 0.6)
  splatIndex(x + 0.5, w * 0.5 + 2, 2)                   // halo
  hsv(bh, bs, 1)
  fillRangeAA(x + 0.5 - w * 0.5, x + 0.5 + w * 0.5, 2)  // body
  hsv(bh, bs * 0.3, 1)
  splatIndex(x + 0.5, max(0.6, w * 0.3), 2)             // white-hot core
}

// one eye on the panel at column position x (0..1), coming from px
function eye2D(x, px, gw) {
  var w = widthPx * (1 + thump) / gw
  tint(x)
  if (px >= 0 && abs(x - px) < 0.5) {
    var k = 0.5 + 0.5 * keep, a = min(x, px), b = max(x, px)
    hsv(bh, bs, 0.55 * k)
    fillRect(a, 0, b, 1)
    hsv(bh, bs, k)
    fillRect(a, 0.25, b, 0.75)
  }
  hsv(bh, bs, 0.55)
  drawLine(x, -0.2, x, 1.2, w * 0.5 + 1.5 / gw, 2)     // full-height bar
  hsv(bh, bs, 1)
  drawLine(x, 0.22, x, 0.78, w * 0.5 + 0.8 / gw, 2)    // brighter middle
  hsv(bh, bs * 0.3, 1)
  drawLine(x, 0.38, x, 0.62, max(0.6, w * 0.25 * gw) / gw, 2)
}

// repaint last frame's white-hot core in the eye colour, so the wake behind
// it stays saturated instead of keeping a pale dot per frame
function uncore(x, two, n, gw) {
  if (x < 0) return
  tint(two ? x : x / n)
  hsv(bh, bs, 1)
  var c = max(0.6, widthPx * (1 + thump) * 0.3)
  if (two) fillRect(x - c / gw, 0.36, x + c / gw, 0.64)
  else fillRange(floor(x + 0.5 - c), ceil(x + 0.5 + c))
}

export function renderBulk() {
  var two = has2DMap()
  var n = pixelCount, gw = 1
  if (two) { gw = gridWidth(); if (gw < 1) gw = 32 }
  uncore(prev0, two, n, gw)
  uncore(prev1, two, n, gw)
  fade(keep)
  var x0, x1
  if (two) {
    var m = widthPx * 0.5 / gw
    var span = 1 - 2 * m
    if (eyes == 2) { x0 = m + p * (span / 2 - m); x1 = 1 - x0 }
    else { x0 = m + p * span; x1 = 1 - x0 }
    eye2D(x0, prev0, gw)
    if (eyes > 0) eye2D(x1, prev1, gw)
  } else {
    var last = n - 1
    if (eyes == 2) { x0 = p * (floor(n / 2) - 1); x1 = last - x0 }
    else { x0 = p * last; x1 = last - x0 }
    eye1D(x0, prev0, n)
    if (eyes > 0) eye1D(x1, prev1, n)
  }
  prev0 = x0
  prev1 = eyes > 0 ? x1 : -1
}
