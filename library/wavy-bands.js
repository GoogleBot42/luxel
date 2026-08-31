// name: Wavy Bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "Wavy Bands"; original source never consulted.

// Vertical rainbow bands whose boundaries snake sideways (traveling sine)
// and whose widths breathe organically (perlin-warped y). Hue comes from the
// quantized band, brightness from the continuous pre-quantization position —
// bright band centers, soft dark seams. Slow, lava-lamp-like drift.
// 1D fallback shows one horizontal scanline of the 2D field.

export var bands = 4

var t = 0        // seconds, wraps after ~an hour
var t1 = 0       // slow wiggle-phase clock
var t2 = 0       // faster, opposite-direction noise clock

// Tunables — the top-level values reproduce the constants the port shipped
// with (4 bands, 1x drift, 10% sway, 33% breathing), so an untouched pattern
// renders exactly as before.
var speed = 1     // drift-clock multiplier; 1 = the hand-tuned lava-lamp pace
var sway = 0.1    // sideways wave amplitude, fraction of the display width
var breathe = 0.33 // noise warp depth on y, fraction of the display height

// How many colored bands span the display.
//# min=1 max=8 step=1 default=4
export function sliderColumns(v) { bands = clamp(floor(v), 1, 8) }

// Overall drift pace. 1x is the tuned crawl; 0 freezes the field.
//# min=0 max=4 step=0.05 default=1
export function sliderDriftSpeed(v) { speed = max(v, 0) }

// How far the band boundaries snake sideways, in percent of display width.
//# min=0 max=50 step=1 default=10
export function sliderSwayPercent(v) { sway = clamp(v, 0, 50) / 100 }

// How hard the noise field swells and pinches band widths along their length,
// in percent of display height. 0 gives perfectly straight-edged bands.
//# min=0 max=100 step=1 default=33
export function sliderBreathePercent(v) { breathe = clamp(v, 0, 100) / 100 }

export function beforeRender(delta) {
  t = mod(t + speed * delta / 1000, 3600)
  t1 = t * 0.25
  t2 = -t * 0.5
}

export function render2D(index, x, y) {
  // warp y: noise makes band widths swell and pinch along their length
  var yw = y - breathe * perlin(x * 2, y * 2, t2, 1.618)
  // warp x: traveling sine fed the *warped* y couples the two distortions
  var xw = x + sway * sin(5 * (t1 + yw))

  var pos = xw * bands
  var bin = floor(pos)
  var v = 1 - 2 * abs(pos - bin - 0.5)   // 1 at band center, 0 at seams
  v = pow(v, 1.4)                        // softer falloff

  hsv(bin / bands, 0.92, v)
}

export function render(index) {
  render2D(index, index / pixelCount, 0.25)
}
