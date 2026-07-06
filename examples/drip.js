// A dripping faucet at pixel 0: the droplet swells, tears off, falls
// under gravity (scaled to the strip so any length works), and splashes.
// One tiny state machine plus a feedback trail.
leds = array(pixelCount)
state = 0  // 0 = growing, 1 = falling, 2 = splash settling
size = 0
pos = 0
vel = 0
pause = 0

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  feedback(leds, pow(0.78, delta * 0.06))
  if (state == 0) {
    size += dt * (0.5 + random(0.5))
    leds[0] = size * size
    if (size >= 1) {
      state = 1
      pos = 0
      vel = pixelCount * 0.05
    }
  } else if (state == 1) {
    vel += pixelCount * 2.2 * dt  // gravity: ~1 s to fall any strip
    pos += vel * dt
    if (pos >= pixelCount - 1) {
      // splash: a burst that the feedback decay fades out
      leds[pixelCount - 1] = 1
      leds[pixelCount - 2] = 0.7
      leds[pixelCount - 3] = 0.4
      leds[pixelCount - 4] = 0.2
      state = 2
      pause = 0.4
    } else {
      leds[floor(pos)] = 1
    }
  } else {
    pause -= dt
    if (pause <= 0) {
      state = 0
      size = 0
    }
  }
}

export function render(index) {
  v = leds[index]
  hsv(0.55 + v * 0.06, 1 - v * v * 0.55, v * v)
}
