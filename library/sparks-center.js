// name: sparks center
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparks center"; original source never consulted.

// Sparks shoot out of the middle of the strip in both directions, like a
// grinding wheel seen edge-on. Each spark launches fast, decelerates
// under constant friction, and its BRIGHTNESS IS ITS SPEED, so it dims as
// it slows and fades to black wherever its momentum runs out — then
// relaunches from the center. Friction is inversely proportional to strip
// length (halved again), so travel distance stays a proportionate
// fraction of any strip. Fast sparks bleach white; slow ones settle into
// deep indigo-blue.

var NUM_SPARKS = 20
var SPEED_SCALE = 1        // launch-speed scale; draws are 1/3..2/3 of it
var DT_SCALE = 0.05        // global frame-time scale-down
var FRICTION = SPEED_SCALE / pixelCount / 2

var sparkVel = array(NUM_SPARKS)   // signed: sign is direction
var sparkPos = array(NUM_SPARKS)   // pixel-index units
var pixels = array(pixelCount)     // cleared every frame: no trails

export function beforeRender(delta) {
  var dt = delta * DT_SCALE
  var i
  arrayReplace(pixels, 0)

  for (i = 0; i < NUM_SPARKS; i++) {
    var v = sparkVel[i]

    // Respawn: momentum spent -> relaunch from center, coin-toss direction.
    if (abs(v) < 0.001) {
      v = (SPEED_SCALE / 3) + random(SPEED_SCALE / 3)
      if (random(1) < 0.5) v = -v
      sparkPos[i] = pixelCount / 2
    }

    // Constant friction opposing motion; a sign flip means it's dead.
    if (v > 0) {
      v = max(0, v - FRICTION * dt)
    } else {
      v = min(0, v + FRICTION * dt)
    }

    sparkPos[i] += v * dt

    // Off either end: zero it out; the zero speed respawns it next frame.
    if (sparkPos[i] >= pixelCount || sparkPos[i] < 0) {
      sparkPos[i] = 0
      v = 0
    }

    // Deposit speed magnitude; overlapping sparks stack.
    pixels[floor(sparkPos[i])] += abs(v)
    sparkVel[i] = v
  }
}

export function render(index) {
  var v = pixels[index]
  var b = v * v            // gamma-like emphasis: faint sparks stay subtle
  // Fixed indigo-blue hue; saturation drops as value rises so the
  // fastest sparks bleach toward white.
  hsv(0.65, clamp(1.2 - b * 1.5, 0, 1), clamp(b, 0, 1))
}
