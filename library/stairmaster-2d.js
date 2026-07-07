// name: Stairmaster 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Stairmaster 2D"; original source never consulted.

// An escalator of diagonal steps marches across the panel while a glowing
// ball at the horizontal center bounces once per passing step. Rainbow hue
// across the width; the base of the stairs washes out toward white.

// Internal constants, intended for hand-tweaking.
var steps = 4        // visible step columns
var ballR = 0.12     // ball radius (~an eighth of the panel width)
var speedT = 0.06    // master clock: one escalator cycle ~4 s

// Shift the scene down a bit so the action sits better in frame
// (one-time transform; persists across frames).
translate(0, -0.15)

var phase = 0
var ballY = 0

export function beforeRender(delta) {
  // One clock drives the staircase...
  phase = time(speedT)
  // ...and the ball bounces `steps` times per cycle, so hops stay locked
  // to passing steps at any master speed. It oscillates over roughly the
  // lower third of the panel, dipping slightly below its resting line.
  ballY = 1.03 - 0.4 * wave(time(speedT / steps))
}

export function render2D(index, x, y) {
  // Staircase: floor-quantize (x + phase) into step levels and subtract
  // from (y + phase). Feeding the same phase into both axes translates
  // the whole staircase diagonally; within a step it is a vertical ramp.
  var level = floor((x + phase) * steps) / steps
  var stair = y + phase - level

  // Ball: bright at the center, fading to zero at the rim.
  var d = hypot(x - 0.5, y - ballY)
  var ball = d < ballR ? 1 - d / ballR : 0

  var v = max(stair, ball)   // negatives clamp to black in the renderer

  // Rainbow across the width; saturation over-driven above 1 at the top
  // (clamped by the renderer) and washing out to near-white at the base.
  hsv(x, 1.1 - y, v)
}
