// name: Tunnel of Squares 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Tunnel of Squares 2D"; original source never consulted.

// Concentric square-ish rings rush toward the viewer from the center of
// the display. Log-spaced radii sell the fly-down-a-corridor perspective;
// a small fixed rotation of the per-quadrant sign vector warps the clean
// diamonds into a subtly corkscrewing spiral. Hue sweeps radially outward
// and the whole palette drifts slowly around the wheel.

// fixed twist applied to the sign vector, precomputed once
const TWIST = 0.12
var twistCos = cos(TWIST)
var twistSin = sin(TWIST)

var flowRate = 11.2   // rad/s at the default slider position
//# min=0 max=1 step=0.01 default=0.75
export function sliderSpeed(v) {
  // tenfold sweep that never fully stops: ~2 rad/s crawl to ~20 rad/s rush
  flowRate = 2 * pow(10, v)
}

var rings = 4
//# min=0 max=1 step=0.01 default=0.55
export function sliderSquarocity(v) {
  rings = 1 + floor(v * 6.99)   // 1..7 square rings per log-octave
}

var flowPhase = 0
var hueDrift = 0

export function beforeRender(delta) {
  // accumulate the animation phase directly; wrapping at 2*PI inside the
  // accumulator is equivalent to the original's hour-long time wrap, and
  // keeps full fixed-point precision forever
  flowPhase = mod(flowPhase + delta / 1000 * flowRate, PI2)
  hueDrift = time(0.08)   // full palette rotation every ~5 s
}

export function render2D(index, x, y) {
  // center the unit square on the origin
  var px = x - 0.5
  var py = y - 0.5

  // square-ish radial metric: dot the position with its own sign vector,
  // rotated by a fixed nudge — an |x|+|y| diamond norm warped into a
  // twisted square, with the twist differing per quadrant
  var sx = sign(px)
  var sy = sign(py)
  var m = px * (sx * twistCos - sy * twistSin)
        + py * (sx * twistSin + sy * twistCos)
  m = max(m, 0.004)   // floor the log singularity at dead center

  // log spacing packs rings tight at the center, exponentially wider out;
  // adding the polar angle turns rings into one continuous spiral
  var phase = rings * log(m) + atan2(py, px) - flowPhase

  // |sin| cubed: narrow bright bands, deep dark gaps, crisp ring edges
  var s = abs(sin(phase))
  hsv(hueDrift + m, 1, s * s * s)
}
