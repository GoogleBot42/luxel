// The glow of a TV through the curtains: scene cuts at random intervals,
// a one-pole flicker inside each scene, rare full-screen flashes. A
// vacant-house utility pattern more than a looker (WLED ships one too).
sceneT = 0
hue = 0.6
sat = 0.4
lvl = 0.8
flick = 1
flash = 0

export function beforeRender(delta) {
  sceneT -= delta * 0.001
  if (sceneT <= 0) {
    sceneT = 0.3 + random(5)  // scene length
    // mostly cool broadcast blues, sometimes a warm interior shot
    hue = random(1) < 0.6 ? 0.55 + random(0.15) : random(0.14)
    sat = 0.15 + random(0.55)
    lvl = 0.25 + random(0.75)
  }
  // in-scene luminance wander (one-pole low-pass over random targets)
  flick += (0.55 + random(0.45) - flick) * min(delta * 0.02, 1)
  flash = random(1) < delta * 0.0007  // explosion / lightning on screen
}

export function render(index) {
  if (flash) {
    hsv(0, 0, 1)
  } else {
    // slight spatial falloff so it reads as spill, not a solid panel
    hsv(hue, sat, lvl * flick * (0.8 + 0.2 * triangle(index / pixelCount)))
  }
}
