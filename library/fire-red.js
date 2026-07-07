// name: fire - red
// Clean-room reimplementation from a prose functional description of the
// community pattern "fire - red"; original source never consulted.

// Classic Fire2012-style cellular-automaton flame: heat cells cool by a
// random amount, convect upward (each cell becomes a weighted average of
// the two cells below it, the lower-by-two weighted double), and random
// sparks inject heat into the bottom tenth. Heat maps through a
// black -> red -> orange -> yellow -> white ramp. The simulation runs on
// a fixed 30 ms tick so flicker speed is frame-rate independent.

// Direction/symmetry mode (a source constant, not a UI control):
//   0 = from the strip head, 1 = from the tail,
//   2 = symmetric from both ends toward the middle,
//   3 = symmetric from the middle toward both ends.
const MODE = 0

var simLen = (MODE < 2) ? pixelCount : floor(pixelCount / 2)
// Symmetric modes run half-length flames, so cool a little harder.
var cooling = (MODE < 2) ? 0.05 : 0.065
const SPARK_CHANCE = 0.5     // sparks per simulation step, on average
const STEP = 30              // simulation tick, ms

var heat = array(simLen)
var acc = 0                  // elapsed-time accumulator, ms

function simulate() {
  var i
  // 1. Cooling: every cell loses a small random amount of heat.
  for (i = 0; i < simLen; i++) {
    heat[i] = clamp(heat[i] - random(cooling), 0, 1)
  }
  // 2. Convection: heat diffuses upward, the cell two below counted twice.
  for (i = simLen - 1; i >= 2; i--) {
    heat[i] = (heat[i - 1] + 2 * heat[i - 2]) / 3
  }
  // 3. Sparking: occasionally slam a cell near the base to high heat.
  if (random(1) < SPARK_CHANCE) {
    var p = floor(random(max(1, simLen * 0.1)))
    heat[p] = clamp(0.6 + random(0.4), 0, 1)
  }
}

export function beforeRender(delta) {
  acc += delta
  if (acc > 150) acc = 150      // don't spiral after a long stall
  while (acc >= STEP) {
    simulate()
    acc -= STEP
  }
}

// Map a strip index to a heat cell according to MODE.
function heatIndex(index) {
  if (MODE == 1) return pixelCount - 1 - index
  if (MODE == 2) {
    // both ends toward the middle
    return min(simLen - 1, (index < simLen) ? index : pixelCount - 1 - index)
  }
  if (MODE == 3) {
    // middle toward both ends
    return min(simLen - 1, floor(abs(index - (pixelCount - 1) / 2)))
  }
  return index
}

export function render(index) {
  var h = heat[heatIndex(index)]
  // Heat ramp: red rises over the first third, green over the middle,
  // blue over the last — black, deep red, orange, yellow, white.
  rgb(clamp(h * 3, 0, 1), clamp(h * 3 - 1, 0, 1), clamp(h * 3 - 2, 0, 1))
}
