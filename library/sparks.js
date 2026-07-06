// name: sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparks"; original source never consulted.

// Embers shot down a channel: each spark has an energy level and a fractional
// position. Velocity goes with energy squared, friction drains energy linearly,
// and every spark deposits additively into a fast-decaying per-pixel buffer —
// the trail is just the buffer's memory of recent deposits.

var numSparks = 24
var pixels = array(pixelCount)
var energy = array(numSparks)
var pos = array(numSparks)

export function beforeRender(delta) {
  delta *= 0.1 // overall speed feel

  // per-frame multiplicative decay: only ~a fifth of each pixel survives
  feedback(pixels, 0.2)

  for (var i = 0; i < numSparks; i++) {
    if (energy[i] <= 0) {
      // respawn: a bit above unity, somewhere in the first few pixels
      energy[i] = 1 + random(0.3)
      pos[i] = random(4)
    }

    // friction scaled inversely with strip length so sparks travel
    // roughly the full strip regardless of pixel count
    energy[i] -= delta * 0.5 / pixelCount

    // energetic sparks fly; motion slows as friction drains them
    pos[i] += energy[i] * energy[i] * delta

    if (pos[i] >= pixelCount) {
      pos[i] = 0
      energy[i] = 0 // respawns next frame
    } else if (energy[i] > 0) {
      pixels[floor(pos[i])] += energy[i]
    }
  }
}

export function render(index) {
  var v = pixels[index]
  v = v * v // punchy gamma
  // fixed ember hue; hot pixels desaturate toward white, faint trails stay orange
  hsv(0.02, 1.1 - v, v)
}
