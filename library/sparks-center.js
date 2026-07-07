// name: sparks center
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparks center"; original source never consulted.

// Sparks shoot from the middle of the strip in both directions, decelerating
// under friction and dimming as they slow (brightness = speed). Fast sparks
// bleach white-hot; slow ones settle into deep indigo before dying and
// relaunching from the center. Positions/velocities are normalized 0..1 so
// travel proportions are length-independent (the original derived friction
// from pixel count to the same effect).

const NUM_SPARKS = 20
const SPEED = 1          // global time scale on frame delta
const FRICTION = 0.3     // deceleration, strip-fractions/s^2
const HUE = 0.65         // blue-indigo
const GAIN = 1.8         // deposit gain so launch speed reads near-white

sparkV = array(NUM_SPARKS)   // signed velocity, strip-fractions/s
sparkX = array(NUM_SPARKS)   // position, 0..1
pixels = array(pixelCount)   // brightness buffer, cleared every frame

export function beforeRender(delta) {
  var dt = delta * 0.001 * SPEED
  arrayReplace(pixels, 0)   // no trails

  var i
  for (i = 0; i < NUM_SPARKS; i++) {
    var v = sparkV[i]

    // respawn from center when momentum runs out; coin-toss direction
    if (abs(v) < 0.001) {
      v = 0.33 + random(0.33)
      if (random(1) < 0.5) v = -v
      sparkX[i] = 0.5
    }

    // constant friction opposing motion
    if (v > 0) v = max(0, v - FRICTION * dt)
    else v = min(0, v + FRICTION * dt)

    sparkX[i] += v * dt

    // off either end: zero out; the dead spark relaunches next frame
    if (sparkX[i] <= 0 || sparkX[i] >= 1) {
      sparkX[i] = 0
      v = 0
    }
    sparkV[i] = v

    // deposit speed magnitude; overlapping sparks stack
    if (v != 0) pixels[floor(sparkX[i] * pixelCount)] += abs(v) * GAIN
  }
}

export function render(index) {
  var v = pixels[index]
  v = v * v   // gamma-like emphasis: faint sparks stay subtle
  // saturation falls as value rises: slow = deep indigo, fast = white-hot
  var s = clamp(1.2 - v, 0, 1)
  hsv(HUE, s, v)
}
