// name: Blue Holiday Candle 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Blue Holiday Candle 2D"; original source never consulted.

// A stylized candle on a 2D matrix: a blue-cored flame with an orange/yellow
// rim sways organically above a blue candle body. The core shimmers with a
// fast fine-grained flicker. The background is a very dim deep purple in
// which three warm-white "stars" softly pulse, each respawning at a new
// random pixel when its life runs out.
//
// The flame is a pure function of (x, y, t), so it is evaluated directly in
// render2D; the only frame-to-frame state is the clock and the twinklers
// (which live on raw pixel indices — layout-agnostic).

var NSTARS = 3
var starIdx = array(NSTARS)    // target pixel index
var starLife = array(NSTARS)   // 1 -> 0
var starRate = array(NSTARS)   // life units per ms (some twinkle fast, some slow)

var tsec = 0        // running seconds clock (wrapped hourly)
var sway = 0        // multi-octave side-to-side sway, centered on zero
var wobble = 0      // faster, smaller secondary shudder
var flickPhase = 0  // fast internal-flicker phase

// How far the flame tip leans, in panel widths, at full sway. 0 freezes it.
var swayAmt = 0.22
//# min=0 max=0.5 step=0.01 default=0.22
export function sliderSway(v) {
  swayAmt = clamp(v, 0, 0.5)
}

function respawnStar(k) {
  starIdx[k] = floor(random(pixelCount))
  starLife[k] = 1
  starRate[k] = 1 / (900 + random(1600))   // full pulse in ~0.9 .. 2.5 s
}

var k
for (k = 0; k < NSTARS; k++) {
  respawnStar(k)
  starLife[k] = random(1)   // desynchronize at power-on
}

export function beforeRender(delta) {
  tsec += delta / 1000
  if (tsec > 3600) tsec -= 3600

  // three sine octaves: each successive one twice as fast, half the weight.
  // Periods shortened from 7/3.5/1.75 s to 2.6/1.3/0.65 s — at the old rate a
  // whole breath took longer than most people watch the panel, so the flame
  // read as frozen.
  sway = (sin(tsec * PI2 / 2.6) + 0.5 * sin(tsec * PI2 / 1.3)
          + 0.25 * sin(tsec * PI2 / 0.65)) / 1.75

  // a faster, shallower shudder riding on top (incoherent period, so the
  // combination never repeats on a short cycle)
  wobble = sin(tsec * PI2 / 0.37) * 0.3 + sin(tsec * PI2 / 0.23) * 0.2

  flickPhase = tsec * 3.5   // a few times faster than real time

  var i
  for (i = 0; i < NSTARS; i++) {
    starLife[i] -= delta * starRate[i]
    if (starLife[i] <= 0) respawnStar(i)
  }
}

export function render2D(index, x, y) {
  // recenter and flip vertical: candle sits at the bottom of the display
  var cx = x - 0.5
  var cy = 0.5 - y   // +up in these coordinates

  // candle body: lower two-fifths band, tapering toward the sides; blue only
  var body = 0
  if (cy < -0.1 && abs(cx) < 0.4) {
    body = (1 - abs(cx) / 0.4) * 0.6
  }

  // Sway: shift the flame sideways before the aspect correction, by an
  // amount that grows with height, so the root stays pinned on the wick and
  // the tip whips. The old version multiplied a sine BY the sway signal as
  // well, which squared a sub-pixel amplitude into nothing — the flame
  // simply never moved.
  var hFrac = clamp(cy + 0.35, 0, 1)          // 0 at the wick, ~1 at the tip
  var lean = (sway + wobble * 0.35) * swayAmt * hFrac
  var fx = (cx - lean) * 1.8
  // curl: the column bows rather than sliding rigidly
  fx += sin(cy * 5 + tsec * 1.9) * 0.35 * swayAmt * hFrac

  // inner core: soft radial blob stretched upward, over a narrow falloff band
  var d = sqrt(fx * fx + cy * cy)
  var core = 1 - clamp((d - 0.06 - max(0, cy) * 0.18) / 0.12, 0, 1)
  // shimmering burn texture: triangle flicker mixing fast time phase with
  // fine spatial frequencies (different per axis), swinging roughly +-half
  var flick = triangle(frac(flickPhase + fx * 3.1 + cy * 2.3))
  core = core * (0.5 + flick)

  // outer shell: soft annulus around a circle of radius ~1/3
  var shell = 1 - clamp((abs(d - 0.3) - 0.02) / 0.08, 0, 1)

  // channel composition
  var r = core * 0.25 + shell * 0.9
  var g = core * 0.18 + shell * 0.7 * clamp(0.5 - cy, 0, 1)   // yellow at base, red at tip
  var b = core + body

  if (r + g + b > 0.02) {
    rgb(clamp(r, 0, 1), clamp(g, 0, 1), clamp(b, 0, 1))
  } else {
    // night sky: is one of the twinklers on this pixel?
    var s = -1
    var i
    for (i = 0; i < NSTARS; i++) {
      if (starIdx[i] == index) s = i
    }
    if (s >= 0) {
      // warm near-white star, soft sine hump over its remaining life
      var pulse = sin(starLife[s] * PI) * 0.6
      rgb(pulse, pulse * 0.85, pulse * 0.65)
    } else {
      hsv(0.78, 1, 0.015)   // deep dim purple, just above black
    }
  }
}
