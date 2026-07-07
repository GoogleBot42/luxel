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

//# min=0 max=1 step=0.143 default=0.429
export function sliderColumns(v) { bands = 1 + floor(v * 6.99) }

export function beforeRender(delta) {
  t = mod(t + delta / 1000, 3600)
  t1 = t * 0.25
  t2 = -t * 0.5
}

export function render2D(index, x, y) {
  // warp y: noise makes band widths swell and pinch along their length
  var yw = y - 0.33 * perlin(x * 2, y * 2, t2, 1.618)
  // warp x: traveling sine fed the *warped* y couples the two distortions
  var xw = x + 0.1 * sin(5 * (t1 + yw))

  var pos = xw * bands
  var bin = floor(pos)
  var v = 1 - 2 * abs(pos - bin - 0.5)   // 1 at band center, 0 at seams
  v = pow(v, 1.4)                        // softer falloff

  hsv(bin / bands, 0.92, v)
}

export function render(index) {
  render2D(index, index / pixelCount, 0.25)
}
