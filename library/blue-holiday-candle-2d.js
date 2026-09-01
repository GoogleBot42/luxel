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

var tsec = 0        // running seconds clock (wrapped every 15 min)
var sway = 0        // multi-octave side-to-side sway, centered on zero
var wobble = 0      // faster, smaller secondary shudder
var flickPhase = 0  // fast internal-flicker phase

// Shape state: a real flame is not a rigid blob that slides sideways, so the
// silhouette also breathes (tall+thin <-> short+fat), licks at the tip, and
// occasionally gutters.
var heightF = 1     // vertical stretch of the flame body
var widthF = 1      // girth of the flame body
var tipLick = 0     // fast whip added to the outline near the tip
var gutter = 0      // 0 normally, ->1 during a rare guttering duck
var lively = 1      // overall liveliness, driven by the sway dial

// How far the flame tip leans, in panel widths, at full sway. This also sets
// how hard the flame breathes, licks and gutters, so 0 holds the silhouette
// perfectly still (the core's burn shimmer keeps going).
var swayAmt = 0.22
//# min=0 max=0.5 step=0.01 default=0.22
export function sliderSway(v) {
  swayAmt = clamp(v, 0, 0.5)
  lively = clamp(swayAmt * 4.5, 0, 1.6)   // 1.0 at the default sway
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
  // 900 s keeps the fastest phase term (tsec * PI2 / 0.23 ~ 27x) inside the
  // engine's 16.16 range; an hour would overflow it.
  if (tsec > 900) tsec -= 900

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

  // Breath: height and girth run on separate incommensurate periods and pull
  // in opposite directions, so the flame draws itself up thin and then squats
  // out fat instead of holding one outline.
  heightF = clamp(1 + (sin(tsec * PI2 / 1.9) * 0.13
                       + sin(tsec * PI2 / 0.83) * 0.07) * lively, 0.4, 2)
  widthF = clamp(1 - (sin(tsec * PI2 / 1.7 + 2.1) * 0.11
                      + sin(tsec * PI2 / 0.61) * 0.06) * lively, 0.5, 2)

  // Guttering: three slow humps multiplied together only spike when all three
  // coincide, which is rare and irregular. When they do the flame ducks,
  // spreads and loses its point, the way a real one does in a draught.
  var g1 = 0.5 + 0.5 * sin(tsec * PI2 / 6.7)
  var g2 = 0.5 + 0.5 * sin(tsec * PI2 / 4.3 + 1.1)
  var g3 = 0.5 + 0.5 * sin(tsec * PI2 / 2.9 + 2.3)
  gutter = clamp((g1 * g2 * g3 - 0.42) / 0.3, 0, 1) * lively
  heightF = clamp(heightF * (1 - gutter * 0.4), 0.4, 2)
  widthF = clamp(widthF * (1 + gutter * 0.3), 0.5, 2)

  // the tip's own fast whip, on top of the travelling edge waves
  tipLick = sin(tsec * 7.7) * 0.035 * lively

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
  // widthF > 1 fattens the flame, heightF > 1 draws it up tall
  var fx = (cx - lean) * 1.8 / widthF
  // curl: the column bows rather than sliding rigidly
  fx += sin(cy * 5 + tsec * 1.9) * 0.35 * swayAmt * hFrac
  var fy = cy / heightF

  // inner core: soft radial blob stretched upward, over a narrow falloff band
  var d = sqrt(fx * fx + fy * fy)
  var core = 1 - clamp((d - 0.06 - max(0, fy) * 0.18) / 0.12, 0, 1)
  // shimmering burn texture: triangle flicker mixing fast time phase with
  // fine spatial frequencies (different per axis), swinging roughly +-half
  var flick = triangle(frac(flickPhase + fx * 3.1 + cy * 2.3))
  core = core * (0.5 + flick)

  // outer shell: soft annulus whose radius is not constant. Two travelling
  // waves run UP the flame (they move because their phase advances with
  // time), plus the tip whip; all of it tapers to nothing at the wick, so the
  // base stays anchored while the upper outline licks and reshapes.
  var upness = clamp((fy + 0.08) / 0.38, 0, 1)
  var lick = sin(fy * 11 - tsec * 5.7) * 0.045 + sin(fy * 17 + tsec * 3.3) * 0.025
  var rOut = 0.3 + (lick + tipLick) * upness * lively
  var shell = 1 - clamp((abs(d - rOut) - 0.02) / 0.08, 0, 1)

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
