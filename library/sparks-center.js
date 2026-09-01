// name: sparks center
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparks center"; original source never consulted.

// Sparks shoot from the middle of the strip in both directions, decelerating
// under friction and dimming as they slow (brightness = speed). Fast sparks
// bleach white-hot; slow ones settle into deep indigo before dying. The pool
// fires all at once on the very first frame — the one-off opening fountain —
// after which a spark that flew clean off an end comes back within a second,
// while one that merely ran out of momentum mid-strip lies dormant and relights
// only rarely. Speeds are in PIXELS per second, so how far a spark gets depends
// on strip length: on a short strip nearly every spark reaches an end and keeps
// recycling (busy sparkle), on a long one they stall near the middle and the
// strip is near-black with an occasional lone spark.

const NUM_SPARKS = 20
const SPEED = 1          // global time scale on frame delta
const FRICTION = 100     // deceleration, pixels/s^2
const LAUNCH_MIN = 40    // launch speed floor, pixels/s
const LAUNCH_SPAN = 40   // launch speed spread above the floor
const RELIGHT = 0.02     // per-second chance a stalled spark relights
const REKINDLE = 1.5     // per-second chance a spark that left the strip returns
const HUE = 0.63         // blue-indigo
const GAIN = 0.0095      // deposit gain: brightness tracks speed

sparkV = array(NUM_SPARKS)   // signed velocity, pixels/s
sparkX = array(NUM_SPARKS)   // position, in pixels
sparkOut = array(NUM_SPARKS) // 1 = left the strip, so it comes back quickly
pixels = array(pixelCount)   // brightness buffer, cleared every frame
opened = 0                   // has the opening fountain fired?

export function beforeRender(delta) {
  var dt = delta * 0.001 * SPEED
  // no trails: blank the deposit buffer. (arrayReplace splats its value
  // arguments starting at index 0 — it is NOT an array fill.)
  for (var c = 0; c < pixelCount; c++) pixels[c] = 0

  var i
  for (i = 0; i < NUM_SPARKS; i++) {
    var v = sparkV[i]

    // dead: the first frame launches the whole pool at once; after that a
    // spark that ran off an end comes back quickly, while one that stalled
    // mid-strip waits a long time before relighting
    if (v == 0) {
      var rate = sparkOut[i] ? REKINDLE : RELIGHT
      if (opened && random(1) >= rate * dt) continue
      v = LAUNCH_MIN + random(LAUNCH_SPAN)
      if (random(1) < 0.5) v = -v
      sparkX[i] = pixelCount / 2
      sparkOut[i] = 0
    }

    // constant friction opposing motion
    if (v > 0) v = max(0, v - FRICTION * dt)
    else v = min(0, v + FRICTION * dt)

    sparkX[i] += v * dt

    // off either end: zero out and flag for a quick relaunch
    if (sparkX[i] < 0 || sparkX[i] >= pixelCount) {
      sparkX[i] = 0
      v = 0
      sparkOut[i] = 1
    }
    sparkV[i] = v

    // deposit speed magnitude; overlapping sparks stack
    if (v != 0) pixels[floor(sparkX[i])] += abs(v) * GAIN
  }
  opened = 1
}

export function render(index) {
  var v = pixels[index]
  v = v * v   // gamma-like emphasis: faint sparks stay subtle
  // saturation falls as value rises: slow = deep indigo, fast = white-hot
  var s = clamp(1 - v, 0, 1)
  hsv(HUE, s, v)
}
