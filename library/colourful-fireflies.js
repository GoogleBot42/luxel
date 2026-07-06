// name: colourful fireflies
// Clean-room reimplementation from a prose functional description of the
// community pattern "colourful fireflies"; original source never consulted.

// A swarm of colored sparks darts along the strip, each dragging a fading
// comet tail, slowing under friction until it dies and respawns elsewhere
// with a fresh random velocity. Each spark owns a fixed hue; together they
// cover the rainbow.

var sparkCount = 1 + floor(pixelCount / 10)
var velocity = array(sparkCount)   // signed, pixels per scaled ms
var position = array(sparkCount)   // fractional, in pixel units
var sparkHue = array(sparkCount)
var bright = array(pixelCount)     // accumulated (signed) energy
var hueStamp = array(pixelCount)   // hue of the last spark to touch a pixel

var maxSpeed = 0.15
var deadBand = 0.01

var i
for (i = 0; i < sparkCount; i++) {
  sparkHue[i] = i / sparkCount  // spread evenly around the hue wheel
  // velocity starts at 0 = inside the dead-band, so every spark
  // respawns with a random state on the first frame
}

export function beforeRender(delta) {
  var dt = delta * 0.1  // global speed trim

  // Trail decay: ~10% per frame (frame-rate-dependent, like the original).
  feedback(bright, 0.9)

  for (var s = 0; s < sparkCount; s++) {
    if (abs(velocity[s]) < deadBand) {
      // Died: respawn somewhere else with a fresh signed velocity.
      velocity[s] = random(2 * maxSpeed) - maxSpeed
      position[s] = random(pixelCount)
    }
    velocity[s] *= 0.995  // friction

    var p = position[s] + velocity[s] * dt
    if (p >= pixelCount) p -= pixelCount
    if (p < 0) p += pixelCount
    position[s] = p

    // Deposit signed energy proportional to speed; render squares it,
    // so backward movers glow just as bright and dying sparks dim out.
    var px = floor(p)
    bright[px] += velocity[s]
    hueStamp[px] = sparkHue[s]
  }
}

export function render(index) {
  var b = bright[index]
  hsv(hueStamp[index], 0.95, b * b * 10)
}
