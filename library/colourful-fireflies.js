// name: colourful fireflies
// Clean-room reimplementation from a prose functional description of the
// community pattern "colourful fireflies"; original source never consulted.

// A swarm of colored sparks that dart along the strip with comet tails,
// slow down by friction, coast to a stop, and respawn elsewhere with a new
// random velocity. Density scales with strip length.

var numSparks = 1 + floor(pixelCount / 10)
var sparkV = array(numSparks)   // signed velocity, pixels per (scaled) ms
var sparkX = array(numSparks)   // fractional position in pixel units
var sparkH = array(numSparks)   // fixed hue, spread around the wheel
var pixV = array(pixelCount)    // accumulated signed energy per pixel
var pixH = array(pixelCount)    // hue stamp: last spark to touch the pixel

var i
for (i = 0; i < numSparks; i++) {
  sparkV[i] = (random(2) - 1) * 0.5
  sparkX[i] = random(pixelCount)
  sparkH[i] = i / numSparks
}

export function beforeRender(delta) {
  delta *= 0.1                       // global speed trim

  feedback(pixV, 0.9)                // trails: ~10% decay per frame

  for (i = 0; i < numSparks; i++) {
    // Friction has brought it to rest: respawn with fresh speed/position.
    if (abs(sparkV[i]) < 0.005) {
      sparkV[i] = (random(2) - 1) * 0.5
      sparkX[i] = random(pixelCount)
    }

    sparkV[i] *= 0.99                // friction
    sparkX[i] = mod(sparkX[i] + sparkV[i] * delta, pixelCount)

    // Deposit signed energy under the spark and stamp its hue.
    var p = floor(sparkX[i])
    pixV[p] += sparkV[i]
    pixH[p] = sparkH[i]
  }
}

export function render(index) {
  // Squaring makes backward (negative-energy) sparks just as bright, and
  // gives the tails a fast nonlinear fade.
  var v = pixV[index]
  hsv(pixH[index], 0.95, v * v * 10)
}
