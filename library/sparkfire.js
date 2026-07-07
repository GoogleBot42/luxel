// name: sparkfire
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparkfire"; original source never consulted.

// Fire that burns from index 0 toward the far end. Ember sparks launch at the
// base, accelerate (position advances by speed squared), and deposit heat along
// every pixel they cross. Heat convects upward via a biased weighted average
// and cools both subtractively and multiplicatively. Positions/speeds are kept
// in normalized 0..1 strip fractions so pacing is length-independent.

const NUM_SPARKS = 5
const SPEED = 0.001      // global time scale: ms -> seconds
const ACCEL = 0.35       // spark acceleration, strip-fractions/s^2
const LAUNCH = 0.35      // cap on random launch speed
const COOL_SUB = 0.4     // subtractive cooling per second
const COOL_MUL = 1.2     // multiplicative cooling rate per second

heat = array(pixelCount)
sparkPos = array(NUM_SPARKS)
sparkSpeed = array(NUM_SPARKS)

// startup: random modest speed, random position anywhere on the strip
var i
for (i = 0; i < NUM_SPARKS; i++) {
  sparkSpeed[i] = 0.05 + random(LAUNCH)
  sparkPos[i] = random(1)
}

export function beforeRender(delta) {
  var dt = delta * SPEED
  var i, p

  // 1. cooling: subtractive + slight multiplicative decay; snap to zero
  //    when the subtractive amount alone would exceed the pixel's heat
  var sub = COOL_SUB * dt
  var keep = max(0, 1 - COOL_MUL * dt)
  for (i = 0; i < pixelCount; i++) {
    var h = heat[i]
    if (h <= sub) heat[i] = 0
    else heat[i] = (h - sub) * keep
  }

  // 2. upward convection: walk top-down so each pixel reads still-unmodified
  //    lower neighbors; weights biased toward the farthest-below neighbor
  for (i = pixelCount - 1; i >= 4; i--) {
    heat[i] = (heat[i - 1] + heat[i - 2] + heat[i - 3] * 2 + heat[i - 4] * 3) / 7
  }

  // 3. sparks
  for (i = 0; i < NUM_SPARKS; i++) {
    var spd = sparkSpeed[i]
    if (spd <= 0) {                 // respawn from the base
      spd = 0.05 + random(LAUNCH)
      sparkPos[i] = 0
    }
    spd += ACCEL * dt               // linear speed growth
    var oldPix = floor(sparkPos[i] * pixelCount)
    var pos = sparkPos[i] + spd * spd * dt   // travel by speed squared
    if (pos >= 1) {
      sparkPos[i] = 0               // fell off the end: respawn next frame
      sparkSpeed[i] = 0
    } else {
      var newPix = floor(pos * pixelCount)
      var dep = max(0, 1 - spd) * 0.5   // fast old sparks deposit less heat
      for (p = oldPix; p <= newPix; p++) heat[p] += dep
      sparkPos[i] = pos
      sparkSpeed[i] = spd
    }
  }
}

export function render(index) {
  var h = heat[index]
  // hue: red shifting toward yellow with the *square* of heat
  var hue = 0.13 * min(1, h * h)
  // brightness roughly doubled heat; saturation bleaches past unity
  var v = clamp(h * 2, 0, 1)
  var s = h <= 1 ? 1 : max(0, 1 - (h - 1) * 1.5)
  hsv(hue, s, v)
}
