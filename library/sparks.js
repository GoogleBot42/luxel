// name: sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparks"; original source never consulted.

var numSparks = 24
var pixels = array(pixelCount)   // per-pixel energy buffer, persists across frames
var energy = array(numSparks)
var pos = array(numSparks)

// Stagger the initial shower so the strip is busy from the first frame
var i
for (i = 0; i < numSparks; i++) {
  energy[i] = random(1)
  pos[i] = random(pixelCount)
}

export function beforeRender(delta) {
  delta *= 0.1                       // scale time down ~an order of magnitude
  feedback(pixels, 0.2)              // multiplicative decay: ~a fifth survives each frame

  for (i = 0; i < numSparks; i++) {
    if (energy[i] <= 0) {
      // respawn: energy a bit above unity, position in the first few pixels
      energy[i] = 1 + random(0.3)
      pos[i] = random(4)
    }
    // friction ~ delta, inversely proportional to strip length, so a spark
    // travels roughly the whole strip regardless of pixelCount
    energy[i] -= delta * 0.5 / pixelCount
    // velocity ~ energy squared: flare fast, then die slow
    pos[i] += energy[i] * energy[i] * delta
    if (pos[i] >= pixelCount) {
      pos[i] = 0
      energy[i] = 0                  // respawns next frame
    } else {
      pixels[floor(pos[i])] += max(0, energy[i])   // additive deposit makes the trail
    }
  }
}

export function render(index) {
  var p = pixels[index]
  var v = p * p                      // squared for punchy gamma
  // hot cores desaturate toward white; faint trails stay deep ember orange
  hsv(0.02, 1.1 - v, v)
}
