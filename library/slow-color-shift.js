// name: slow color shift
// Clean-room reimplementation from a prose functional description of the
// community pattern "slow color shift"; original source never consulted.

// Soft islands of color, roughly a dozen LEDs wide, separated by dark
// valleys. The blobs slosh back and forth (~10 s cycle) while the whole
// palette drifts around the hue wheel, with a gentle quarter-wheel hue
// gradient spread along the strip. Stateless: two clocks, per-pixel math.

var phaseA   // sloshing clock, full turn ~10 s
var hueBase  // hue lap, ~6.5 s

// time(n) laps in n * 65.536 s, hence the divisions in the handlers.
// Defaults reproduce the untouched pattern: 0.15 -> ~9.8 s slosh,
// 0.1 -> ~6.5 s hue lap, 0.52 rad/px -> ~12 px blobs, quarter wheel spread.
var sloshInterval = 0.15
var hueInterval = 0.1
var blobK = 0.52              // radians of the standing wave per pixel
var hueSpanDiv = 4 * pixelCount // strip-length hue gradient divisor

// Distance in LEDs between neighbouring colour islands.
//# min=3 max=40 step=0.5 default=12
export function sliderBlobSpacing(v) { blobK = PI2 / max(2, v) }

// Seconds for one full back-and-forth slosh of the islands.
//# min=1 max=60 step=0.1 default=9.8
export function sliderSloshSeconds(v) { sloshInterval = max(0.5, v) / 65.536 }

// Seconds for one lap around the colour wheel.
//# min=1 max=60 step=0.1 default=6.5
export function sliderColorCycleSeconds(v) { hueInterval = max(0.5, v) / 65.536 }

// Degrees of hue spread from one end of the strip to the other.
//# min=5 max=720 step=5 default=90
export function sliderHueSpread(v) { hueSpanDiv = 360 * pixelCount / max(1, v) }

export function beforeRender(delta) {
  phaseA = time(sloshInterval) * PI2   // ~9.8 s per full turn
  hueBase = time(hueInterval)          // ~6.5 s per hue lap
}

export function render(index) {
  // Standing wave against the raw pixel index (fixed blob size in LEDs),
  // phase-swept back and forth by clock A.
  var s = sin(index * blobK + 3 * sin(phaseA))

  // Fourth-power sharpening: plain sine -> distinct blobs with wide gaps.
  var v = (s + 1) / 2
  v = v * v
  v = v * v

  // Hue: drifting base + small wobble from the same wave + the strip-length
  // gradient set by the Hue Spread control.
  var h = hueBase + 0.2 * s * 0.2 + index / hueSpanDiv
  hsv(h, 1, v)
}
