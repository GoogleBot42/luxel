// name: fire - red
// Clean-room reimplementation from a prose functional description of the
// community pattern "fire - red"; original source never consulted.

// Classic Fire2012-style cellular-automaton flame on a strip: cool every
// cell a little at random, diffuse heat upward with a weighted average,
// and occasionally inject a bright spark near the base. Heat maps through
// a black -> red -> orange -> yellow -> white ramp. The simulation runs on
// a fixed tick so flicker speed is independent of render frame rate.

// Direction/symmetry mode (a source constant, not a UI control):
//   0 = flames rise from the strip head
//   1 = flames rise from the tail
//   2 = symmetric, from both ends toward the middle
//   3 = symmetric, from the middle toward both ends
const MODE = 0

const STEP = 30          // simulation tick, ms
const SPARK_CHANCE = 0.5 // sparks per simulation step, on average
const SPARK_ZONE = 0.1   // sparks land in this bottom fraction of the flame
var COOLING = 0.09       // max random heat lost per cell per step
// Constants tuned around a ~60-70 pixel strip; longer strips want less
// cooling (they do not auto-scale with pixelCount).

// Symmetric modes simulate only half the strip and mirror it; bump the
// cooling slightly to compensate for the shorter flame run.
var simSize = MODE < 2 ? pixelCount : ceil(pixelCount / 2)
if (MODE >= 2) COOLING = COOLING * 1.3

var heat = array(pixelCount)  // 0..1 heat per simulated cell (simSize used)
var acc = 0                   // elapsed-time accumulator, ms

export function beforeRender(delta) {
  acc += delta
  // Advance in whole fixed steps so long frames don't drift the sim clock.
  while (acc >= STEP) {
    acc -= STEP
    fireStep()
  }
}

function fireStep() {
  var i = 0

  // 1. Cooling: every cell sheds a small random amount of heat.
  for (i = 0; i < simSize; i++) {
    heat[i] = clamp(heat[i] - random(COOLING), 0, 1)
  }

  // 2. Convection: from the top down, each cell becomes a weighted average
  // of the cells below — the cell two below counts double, which makes
  // heat rise faster than a plain neighbor average.
  for (i = simSize - 1; i >= 2; i--) {
    heat[i] = (heat[i - 1] + 2 * heat[i - 2]) / 3
  }

  // 3. Sparking: sometimes slam a cell near the base to (near) full heat.
  if (random(1) < SPARK_CHANCE) {
    var s = floor(random(simSize * SPARK_ZONE))
    heat[s] = clamp(0.6 + random(0.4), 0, 1)
  }
}

// Heat -> color: red ramps up over the first third of the range, green
// over the middle third, blue over the last third. Qualitatively black,
// deep red, red, orange, yellow, white.
function showHeat(h) {
  var k = clamp(h, 0, 1) * 3   // clamp keeps full heat on the ramp's end
  rgb(clamp(k, 0, 1), clamp(k - 1, 0, 1), clamp(k - 2, 0, 1))
}

export function render(index) {
  var cell = index
  if (MODE == 1) {
    cell = pixelCount - 1 - index
  } else if (MODE == 2) {
    // Base at both ends, tips meeting in the middle.
    cell = index < simSize ? index : pixelCount - 1 - index
  } else if (MODE == 3) {
    // Base in the middle, tips at both ends.
    cell = index < simSize ? simSize - 1 - index : index - simSize
  }
  showHeat(heat[cell])
}
