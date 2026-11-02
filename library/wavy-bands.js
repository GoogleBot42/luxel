// name: Wavy Bands
// tags: both, ambient, holiday
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/wavy-bands.md, halloween-wavy-bands.md — reimagined, not a replica.
// Vertical colour bands whose edges snake sideways (a travelling sine) and whose
// widths swell and pinch (a noise warp on y): bright band centres, soft dark
// seams, a slow lava-lamp drift. The noise warp (fillNoise3D) and the sideways
// sine fold into one 16x16 displacement canvas, read back bilinearly, so a
// pixel costs a single canvasGet. Palettes:
// rainbow (slowly rolling), Halloween, Christmas. A strip shows one scanline.

var bands = 4
var speed = 1
var sway = 0.1
var breathe = 0.33
var palette = 0

//# min=1 max=12 step=1 default=4
export function sliderColumns(v) { bands = clamp(floor(v), 1, 12) }
// Drift pace: 1x is the tuned crawl, 0 freezes the field.
//# min=0 max=4 step=0.05 default=1
export function sliderDriftSpeed(v) { speed = clamp(v, 0, 4) }
// How far the band edges snake sideways, in percent of the display width.
//# min=0 max=50 step=1 default=10
export function sliderSwayPercent(v) { sway = clamp(v, 0, 50) / 100 }
// How hard band widths swell and pinch, in percent of the display height.
//# min=0 max=100 step=1 default=33
export function sliderBreathePercent(v) { breathe = clamp(v, 0, 100) / 100 }
// 0 rainbow · 1 Halloween · 2 Christmas
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

// six-entry hue/saturation cycles for the themed palettes
var hH = [0.03, 0.08, 0.76, 0.05, 0.11, 0.8], hS = [1, 0.95, 0.85, 1, 0.9, 0.8]
var xH = [0, 0.33, 0.12, 0.55, 0.97, 0.42], xS = [1, 1, 0.75, 0.08, 0.85, 0.9]
var warpA = array(256), dxA = array(256)
var t = 0, roll = 0

export function beforeRender(delta) {
  t = mod(t + speed * min(delta, 100) / 1000, 3600)
  roll = mod(t / 40, 1)
  fillNoise3D(warpA, 16, 16, 2 / 15, 2 / 15, 0, 0, -t * 0.5, 3)
  for (var i = 0; i < 256; i++) {
    var yw = (floor(i / 16) + 0.5) / 16 - breathe * warpA[i]
    dxA[i] = sway * sin(5 * (t * 0.25 + yw))
  }
}

export function render2D(index, x, y) {
  var pos = (x + canvasGet(dxA, 16, x, y)) * bands
  var bin = floor(pos)
  var v = 1 - 2 * abs(pos - bin - 0.5)          // 1 at the band centre, 0 at the seams
  v = v * (2 - v)
  if (palette == 0) hsv(bin / bands + roll, 0.92, v * v)
  else {
    var k = mod(bin, 6)
    if (palette == 1) hsv(hH[k], hS[k], v * v)
    else hsv(xH[k], xS[k], v * v)
  }
}

export function render(index) { render2D(index, index / pixelCount, 0.25) }
