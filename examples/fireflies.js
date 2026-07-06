// A particle system: eight fireflies wander the strip, each with its own
// position, drift, and flash clock held in parallel arrays. Flashes are
// eased triangles deposited into a decaying frame buffer.
numFlies = 8
pos = array(numFlies)
vel = array(numFlies)
clocks = array(numFlies)
leds = array(pixelCount)

for (i = 0; i < numFlies; i++) {
  pos[i] = random(pixelCount)
  vel[i] = (random(1) - 0.5) * 0.012
  clocks[i] = random(1)
}

export function beforeRender(delta) {
  feedback(leds, pow(0.8, delta * 0.06))
  for (var i = 0; i < numFlies; i++) {
    pos[i] += vel[i] * delta
    if (pos[i] < 0) pos[i] += pixelCount
    if (pos[i] >= pixelCount) pos[i] -= pixelCount
    clocks[i] += delta * 0.0003          // ~3.3 s per cycle
    if (clocks[i] >= 1) {
      clocks[i] -= 1
      vel[i] = (random(1) - 0.5) * 0.012 // pick a new drift each cycle
    }
    f = clocks[i] * 4                    // flash fills the first quarter
    if (f < 1) {
      leds[floor(pos[i])] += easeInOutQuad(triangle(f))
    }
  }
}

export function render(index) {
  v = min(leds[index], 1)
  // warm yellow-green, whitening slightly at full flash
  hsv(0.19 - v * 0.04, 1 - v * 0.4, v * v)
}
