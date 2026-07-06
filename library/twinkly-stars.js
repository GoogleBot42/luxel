// name: twinkly stars
// Clean-room reimplementation from a prose functional description of the
// community pattern "twinkly stars"; original source never consulted.

// A solid, fully saturated blue strip. Random pixels occasionally "twinkle":
// snap to pure white, then ease back to blue over about a second. Only
// saturation animates; hue stays blue and brightness stays full.
//
// The described original was frame-count based; per its own suggested fix
// this version drives the recovery ramp and twinkle probability from
// elapsed time, so the look is frame-rate independent.

var recoveryMs = 1000     // time to fade from white back to full blue
var chancePerFrame = 0.01 // ~1-in-100 per pixel per (60 fps) frame
var blueHue = 2 / 3

// ms since each pixel last twinkled; start fully recovered (solid blue)
var sinceTwinkle = array(pixelCount)
var ii
for (ii = 0; ii < pixelCount; ii++) sinceTwinkle[ii] = recoveryMs

var dt = 0

export function beforeRender(delta) {
  dt = delta
}

export function render(index) {
  var elapsed = sinceTwinkle[index]
  var s = 1
  if (elapsed < recoveryMs) {
    // mid-recovery: saturation climbs linearly from white back to blue
    s = elapsed / recoveryMs
    sinceTwinkle[index] = elapsed + dt
  } else if (random(1) < chancePerFrame * dt / 16.667) {
    // fresh twinkle: pure white, restart the ramp
    s = 0
    sinceTwinkle[index] = 0
  }
  hsv(blueHue, s, 1)
}
