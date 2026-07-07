// name: Swirlpool 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Swirlpool 2D"; original source never consulted.

// A ring of bright dots orbits the center, each leaving a fading comet
// trail, so the whole reads as interlocking spiral arms — a whirlpool.
// On a much slower, user-set timescale the arms' orbit centers glide
// between spread-apart and collapsed-through-the-middle arrangements.
// Each arm has its own rainbow hue; the assignment drifts over time.
// Technique: draw a few points per frame into persistent brightness/hue
// canvases, decay the brightness exponentially, and square it at output
// for punchy trails. Deterministic wave math throughout. (The original
// sized its canvas as sqrt(pixelCount), assuming a square matrix; here
// the simulation runs on a fixed 16x16 virtual canvas instead.)

const W = 16
var bright = array(W * W)     // per-cell brightness canvas
var hues = array(W * W)       // per-cell hue canvas

var arms = 5
var colorRate = 0.03          // hue-drift cycles per second (pre-wobble)
var animate = 1               // 1 only when the swirl slider is far left
var swirlInterval = 2.5       // time() interval for the slow morph
var colorPhase = 0

//# min=0 max=1 step=0.01 default=0.25
export function sliderNumberOfArms(v) {
  // two arms up to the matrix width (16)
  arms = clamp(floor(2 + v * 14), 2, 16)
}

//# min=0 max=1 step=0.01 default=0.2
export function sliderColorSpeed(v) {
  colorRate = 0.01 + v * 0.15
}

//# min=0 max=1 step=0.01 default=0
export function sliderAnimateSwirl(v) {
  // slider acting as a toggle: only the far-left position animates the
  // morph; anywhere else freezes it in the spread arrangement
  animate = v < 0.05
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSwirlSpeed(v) {
  // inverted: right = faster; even at fastest ~a minute per cycle
  swirlInterval = 4 - 3 * v     // time() interval: ~65 s .. ~262 s
}

export function beforeRender(delta) {
  // slow morph phase: 0 = spread, 1 = collapsed past center
  var swirlPhase = animate ? triangle(time(swirlInterval)) : 0
  // spread factor runs +1 -> -(sqrt(2) - 1) as the phase rises
  var spread = 1 - SQRT2 * swirlPhase

  // exponential decay of old pixels: trails last roughly half a second
  // (per frame, so trail length varies with frame rate — as the original)
  feedback(bright, 0.94)

  // shared fast rotation: one revolution in a bit over a second
  var rot = time(0.018) * PI2

  // hue drift with a period wobbled on a couple-minute cycle, so the
  // coloring never settles into a fixed loop
  var wobble = 0.5 + triangle(time(2))
  colorPhase = (colorPhase + delta * 0.001 * colorRate * wobble) % 1

  for (var a = 0; a < arms; a++) {
    var off = a / arms                 // arm fraction of the circle
    var ang = rot + off * PI2
    // orbit of radius ~1/4 display around a per-arm center that the
    // spread factor slides outward/inward and slightly past the middle
    var px = 0.5 + 0.24 * cos(ang) + 0.24 * spread * cos(off * PI2)
    var py = 0.5 + 0.24 * sin(ang) + 0.24 * spread * sin(off * PI2)
    var cx = floor(px * W)
    var cy = floor(py * W)
    if (cx >= 0 && cx < W && cy >= 0 && cy < W) {
      var idx = cy * W + cx
      bright[idx] = 1
      // product (not sum): small phase bunches all arms near red, larger
      // phase fans them across the spectrum — the rainbow opens and closes
      hues[idx] = off * colorPhase
    }
  }
}

export function render2D(index, x, y) {
  var idx = floor(y * 15.99) * W + floor(x * 15.99)
  var b = bright[idx]
  hsv(hues[idx], 1, b * b)    // squared: steeper falloff, punchier trails
}
