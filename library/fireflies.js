// name: FireFlies
// Clean-room reimplementation from a prose functional description of the
// community pattern "FireFlies"; original source never consulted.

// Warm amber particles drift both ways along the strip, brightest when
// fastest, leaving short fading trails. Each firefly slows to a stop,
// dims out, and respawns somewhere new with a fresh random velocity.
// Positions wrap around the ends. A slowed-down fork of a classic
// "sparks" particle pattern.
//
// Both decays are frame-rate compensated (pow(k, delta/frame) rather than
// per-frame multiplies), tuned so the feel matches a typical high frame
// rate — the reimplementation fix the original description suggests.

var HUE = 0.055                     // fixed warm amber/orange
var TIME_SCALE = 0.1                // "slowed way down": dt is delta / 10
var MAX_V = 0.18                    // max spawn speed, pixels per (scaled) ms
var DEAD_ZONE = 0.012               // respawn when |v| decays below this
var TRAIL_KEEP = 0.9                // trail multiplier per 60fps-frame
var VEL_KEEP = 0.99                 // velocity multiplier per 60fps-frame
var REF_FRAME = 1000 / 60           // reference frame time (ms) for the decays

var numFlies = 1 + floor(pixelCount / 10)
var pix = array(pixelCount)         // brightness accumulation buffer
var vel = array(numFlies)           // signed velocity, pixels / scaled-ms
var pos = array(numFlies)           // position in pixel units

export function beforeRender(delta) {
  var dt = delta * TIME_SCALE
  var frames = delta / REF_FRAME            // how many reference frames elapsed

  feedback(pix, pow(TRAIL_KEEP, frames))    // fading trails

  var i
  for (i = 0; i < numFlies; i++) {
    if (abs(vel[i]) < DEAD_ZONE) {
      // firefly died: rebirth at a random spot, random speed & direction
      pos[i] = random(pixelCount)
      vel[i] = (random(2) - 1) * MAX_V
    }
    vel[i] = vel[i] * pow(VEL_KEEP, frames)  // gradual slowdown
    pos[i] = mod(pos[i] + vel[i] * dt, pixelCount)  // move & wrap ends
    // deposit the SIGNED velocity — squaring at display time makes
    // negative-direction flies glow just as brightly
    pix[floor(pos[i])] += vel[i]
  }
}

export function render(index) {
  var v = pix[index] * 3.2
  hsv(HUE, 1, v * v)   // square: fast = bright, sign vanishes
}
