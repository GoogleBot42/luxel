// name: Edge Burst
// Curated original (hand-written showcase of the Luxel language/builtins), after the
// community pattern "Edgeburst" (idea only, no source consulted).
// Bursts of colour race out from the centre to the edges, a bright rainbow-
// tinted front over a glowing wake, and the next burst launches as the last
// one reaches the edges, so the wake of one is always fading under the next.
// One radial profile is sampled per frame: a strip draws it as gradientRange
// spans out from the middle; a panel loads it into a live palette and paints
// a precomputed radius canvas with it (paintCanvas, bilinear) — no per-pixel code.

var periodS = 3
// Seconds per burst.
//# min=0.5 max=10 step=0.1 default=3
export function sliderPeriodSeconds(v) { periodS = clamp(v, 0.5, 10) }

var bandPct = 10
// Width of the bright front, in percent of the centre-to-edge distance.
//# min=2 max=50 step=1 default=10
export function sliderBandPercent(v) { bandPct = clamp(v, 2, 50) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var K = 48                       // profile samples, 0 = centre .. K = edge
var pal = array((K + 1) * 4)
var rgb3 = array(3)
setPalette(pal)

// the radius canvas: 0 at the centre, 1 in the corners
var CW = 32, CH = 32
var rad = array(CW * CH)
var built = 0
function buildCanvas() {
  CW = min(max(gridWidth(), 8), 32)
  CH = min(max(gridHeight(), 8), 32)
  if (gridWidth() < 1) { CW = 32; CH = 32 }
  for (var r = 0; r < CH; r++) {
    for (var c = 0; c < CW; c++) {
      var x = (c + 0.5) / CW - 0.5, y = (r + 0.5) / CH - 0.5
      rad[r * CW + c] = min(1, sqrt(x * x + y * y) * 1.414)
    }
  }
  built = 1
}

var ph = 0, burst = 0
export function beforeRender(delta) {
  ph += min(delta, 100) / 1000 / periodS
  if (ph >= 1) { ph = mod(ph, 1); burst = mod(burst + 1, 1000) }
}

// colour of the frame at radius d (0..1) -> ph_, ps_, pv_
var oh = 0, os = 0, ov = 0
function profile(d) {
  var bw = bandPct / 100
  var f = 1.15 * ph * (1.6 - 0.6 * ph)          // the front, easing out
  var hue = burst * 0.29
  if (d <= f) {
    var x = (f - d) / bw                        // depth behind the front
    var band = x < 1 ? (1 - x) * (1 - x) : 0
    var glow = x < 4 ? (1 - x / 4) * (1 - x / 4) : 0
    oh = hue + d * 0.3 - band * 0.06
    os = 1 - band * 0.5                         // the front runs white-hot
    ov = 0.42 + 0.58 * max(band, glow * 0.45) - 0.1 * d
  } else {
    var lead = max(0, 1 - (d - f) / (bw * 0.35))
    var wake = 0.42 * (1 - ph * 0.7) - 0.1 * d  // the last burst, fading
    oh = hue - 0.29 + d * 0.3
    os = 1 - lead * 0.45
    ov = max(wake, lead)
    if (lead > 0) oh = hue + d * 0.3
  }
}

function strip() {
  clear()
  var c = pixelCount / 2
  profile(0)
  for (var k = 0; k < K; k++) {
    var h0 = oh, s0 = os, v0 = ov
    profile((k + 1) / K)
    var a = c * k / K, b = c * (k + 1) / K
    gradientRange(c + a, c + b, h0, s0, v0, oh, os, ov, 1)
    gradientRange(c - a, c - b, h0, s0, v0, oh, os, ov, 1)
  }
}

function panel() {
  if (!built) buildCanvas()
  for (var k = 0; k <= K; k++) {
    profile(k / K)
    hsv2rgb(oh, os, ov, rgb3)
    pal[k * 4] = k / K
    pal[k * 4 + 1] = rgb3[0]
    pal[k * 4 + 2] = rgb3[1]
    pal[k * 4 + 3] = rgb3[2]
  }
  paintCanvas(rad, CW, CH, 1, 1)
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
