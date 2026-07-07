// name: Austin FC
// Clean-room reimplementation from a prose functional description of the
// community pattern "Austin FC"; original source never consulted.

// A calm field of green "verde" embers drifting both directions along the
// strip. Each spark glides, exponentially coasts to a stop under drag,
// leaves a short fading trail, and silently respawns elsewhere at a fresh
// random speed and direction. Sparks wrap end to end.

// hand-edit constants (the original exposes no UI controls)
const hue = 0.36          // Austin FC verde
const maxVel = 0.4        // top launch speed (pixels per scaled ms)
const drag = 0.98         // per-frame velocity multiplier
const deadZone = 0.01     // |velocity| below this triggers respawn
const trailDecay = 0.9    // ~10% intensity loss per frame

var numSparks = floor(pixelCount / 10) + 1
var sparkPos = array(numSparks)
var sparkVel = array(numSparks)
var pixels = array(pixelCount)

export function beforeRender(delta) {
  var dt = delta / 10
  feedback(pixels, trailDecay)

  for (var i = 0; i < numSparks; i++) {
    if (abs(sparkVel[i]) < deadZone) {
      // respawn: symmetric speed band gives random direction and speed
      sparkVel[i] = random(2 * maxVel) - maxVel
      sparkPos[i] = random(pixelCount)
    }
    sparkVel[i] *= drag
    sparkPos[i] = mod(sparkPos[i] + sparkVel[i] * dt, pixelCount)
    // deposit the signed velocity: brightness comes straight from speed,
    // so sparks dim naturally as they decelerate
    pixels[floor(sparkPos[i])] += sparkVel[i]
  }
}

export function render(index) {
  var v = pixels[index]
  // squaring hides faint residue and makes moving sparks pop
  hsv(hue, 1, min(v * v * 10, 1))
}
