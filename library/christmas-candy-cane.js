// name: Candy Stripes
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/christmas-candy-cane.md,
// holiday-diagonal-stripes.md — reimagined, not a replica.
// Candy-cane stripes crawling like a barber pole: two colours (or red / white / green)
// with anti-aliased one-pixel edges, so the crawl glides instead of stepping, and the
// brighter colour held near the other's luminance so white never swamps red.
// Strip: stripes along the strip. Panel: the same stripe function along a rotated
// axis, so diagonals at any angle come out clean, without a staircase.

var segments = 8        // stripes around the strip (half coloured, half base)
//# min=2 max=24 step=2 default=8
export function sliderStripes(v) { segments = max(2, floor(v / 2) * 2) }

var speed = 3           // px/s; negative crawls the other way
//# min=-30 max=30 step=0.5 default=3
export function sliderPixelsPerSecond(v) { speed = clamp(v, -30, 30) }

var colorFrac = 0.5     // share of each coloured/base pair given to the coloured stripe
//# min=10 max=90 step=5 default=50
export function sliderColorWidthPercent(v) { colorFrac = clamp(v / 100, 0.1, 0.9); dirty = 1 }

var palette = 0         // 0: stripe + base colour | 1: red / white / green (green alternates in)
//# min=0 max=1 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 1); dirty = 1 }

var angleDeg = 45       // panel: stripe direction
//# min=0 max=179 step=1 default=45
export function sliderAngleDegrees(v) { angleDeg = clamp(v, 0, 179) }

// raw colours (A stripe, B base, G the third stripe) and their luminance-matched forms
var raw = array(9), col = array(9), tmp = array(3)
raw[0] = 1; raw[1] = 0.02; raw[2] = 0.03
raw[3] = 1; raw[4] = 0.93; raw[5] = 0.82
raw[6] = 0; raw[7] = 0.8; raw[8] = 0.14
function lum(i) { return 0.2126 * raw[i] + 0.7152 * raw[i + 1] + 0.0722 * raw[i + 2] }
function match() {      // nothing brighter than 3.5x the dimmest colour's luminance
  var lo = max(0.05, min(lum(0), min(lum(3), lum(6)))) * 3.5
  for (var i = 0; i < 9; i += 3) {
    var k = min(1, lo / max(lum(i), 0.001))
    col[i] = raw[i] * k; col[i + 1] = raw[i + 1] * k; col[i + 2] = raw[i + 2] * k
  }
  dirty = 1
}
match()
// The pickers' brightness is ignored: the pattern balances the two itself.
export function hsvPickerStripeColor(h, s, v) { hsv2rgb(h, s, 1, tmp); raw[0] = tmp[0]; raw[1] = tmp[1]; raw[2] = tmp[2]; match() }
export function hsvPickerBaseColor(h, s, v) { hsv2rgb(h, s, 1, tmp); raw[3] = tmp[0]; raw[4] = tmp[1]; raw[5] = tmp[2]; match() }

// Two stripe pairs, edges and all, baked into a 256-step table that the per-pixel
// code only indexes; rebuilt when a control or the edge width changes.
var N = 256, TR = array(N), TG = array(N), TB = array(N), dirty = 1, builtPx = 0
function bake(pxPer) {
  for (var i = 0; i < N; i++) {
    var p = i / N * 2, k = floor(p), f = p - k
    var d = f < colorFrac ? min(f, colorFrac - f) : -min(f - colorFrac, 1 - f)
    var c = smoothstep(-0.75, 0.75, d * pxPer)
    if (f > (1 + colorFrac) / 2) k += 1           // the base's far half borders the next stripe
    var j = palette == 1 && k == 1 ? 6 : 0
    TR[i] = mix(col[3], col[j], c); TG[i] = mix(col[4], col[j + 1], c); TB[i] = mix(col[5], col[j + 2], c)
  }
  dirty = 0; builtPx = pxPer
}

var offset = 0, ph2 = 0, pairs = 4, sc = 1, ca = 0.7, sa = 0.7
export function beforeRender(delta) {
  pairs = segments / 2
  if (palette == 1) pairs = max(2, round(pairs / 2) * 2)  // an even count, so R and G close up
  var flat = !has2DMap(), gw = flat ? 0 : gridWidth() > 0 ? max(gridWidth(), gridHeight()) : sqrt(pixelCount)
  var pxPer = flat ? pixelCount / pairs : gw / pairs      // pixels per stripe pair
  if (dirty || pxPer != builtPx) bake(pxPer)
  offset = mod(offset + speed * delta / 1000, pixelCount)
  if (!flat) ph2 = mod(ph2 + speed * delta / 1000 / gw * pairs, 2)
  ca = cos(angleDeg * PI / 180) * pairs * N / 2; sa = sin(angleDeg * PI / 180) * pairs * N / 2
  sc = pairs * N / 2 / pixelCount
}

export function render(index) {
  var i = floor(mod((index + 0.5 - offset) * sc, N))
  rgb(TR[i], TG[i], TB[i])
}

export function render2D(index, x, y) {
  var i = floor(mod((x - 0.5) * ca + (y - 0.5) * sa - ph2 * N / 2, N))
  rgb(TR[i], TG[i], TB[i])
}
