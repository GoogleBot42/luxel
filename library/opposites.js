// name: Opposites
// tags: both, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/opposites.md
// — reimagined, not a replica.
// Two swells travel opposite ways and a third wave is warped by their sum;
// brightness is the squared product of all three, so light pools only where
// they meet. Successive blobs alternate between two opposite hue families
// (switching only in the dark, so rims no longer fringe) and the pair drifts
// round the wheel. On a panel a second pair of swells runs along y and the
// two lenses multiply into drifting glowing islands, rendered on a grid of at
// most 24x24 (setRenderSize) and stretched bilinearly over the panel.

var iA = 6               // seconds for the fast swell to cross once
var ratio = 2            // how many times slower the opposing swell runs
var band = 0.34          // hue lean across a blob (fraction of the wheel)
var lift = 0.1           // keeps the wave product off zero

//# min=1 max=30 step=0.5 default=6
export function sliderCycleSeconds(v) { iA = clamp(v, 1, 30) }

// How many times slower the opposing swell runs; 1 makes the two symmetric.
//# min=1 max=6 step=0.5 default=2
export function sliderSlowWaveRatio(v) { ratio = clamp(v, 1, 6) }

// How far the hue leans across each blob: small values give two tight
// complementary families, large ones rainbow-tinted blobs.
//# min=2 max=100 step=1 default=34
export function sliderHueSpreadPercent(v) { band = clamp(v, 2, 100) / 100 }

// How much the gaps between blobs glow: 0 leaves them black and sparse.
//# min=0 max=50 step=1 default=10
export function sliderGlowPercent(v) { lift = clamp(v, 0, 50) / 100 }

var mirror = 0
// Fold the swells about the middle, so both ends mirror.
//# default=0
export function toggleMirror(v) { mirror = v }

var tA = 0, tB = 0, tC = 0, tD = 0, sized = 0
export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  tA = mod(tA + dt / iA, 1)
  tB = mod(tB + dt / (iA * ratio), 1)
  tC = mod(tC + dt / (iA * 1.37), 1)       // the y pair runs on its own clocks
  tD = mod(tD + dt / (iA * ratio * 1.21), 1)
  if (!sized && has2DMap()) {
    // soft islands: a 24-cell grid stretched bilinearly is plenty
    var gw = gridWidth(), gh = gridHeight()
    setRenderSize(gw > 0 ? min(gw, 24) : 24, gh > 0 ? min(gh, 24) : 24, 1)
    sized = 1
  }
}

// one lens along p: sets B (lifted triple product), W (the warped wave) and
// L (which lap of the warp we are on — it only changes where W is darkest)
var B = 0, W = 0, L = 0
function lens(p, a, b) {
  var w1 = wave(a + p), w2 = wave(b - p), u = p + w1 + w2
  W = wave(frac(u))
  L = floor(u + 0.25)
  B = (w1 + lift) * (w2 + lift) * (W + lift)
}

// alternate laps take opposite sides of the wheel; inside a blob the hue
// only leans across the band, so rims no longer fringe
function hueOf(lap, w) { return tA + 0.5 * mod(lap, 2) + band * (w - 0.5) }

export function render(index) {
  var p = index / pixelCount
  if (mirror) p = abs(p * 2 - 1)
  lens(p, tA, tB)
  hsv(hueOf(L, W), 1, B * B)
}

export function render2D(index, x, y) {
  if (mirror) { x = abs(x * 2 - 1); y = abs(y * 2 - 1) }
  // each lens runs along a gently bent axis, so the islands are organic
  lens(x + 0.2 * sin(PI2 * (y + tC)), tA, tB)
  var bx = B, wx = W, lx = L
  lens(y + 0.2 * sin(PI2 * (x + tD)), tC, tD)
  hsv(hueOf(lx + L, (wx + W) * 0.5), 1, smoothstep(0.05, 0.6, sqrt(bx * B)))
}
