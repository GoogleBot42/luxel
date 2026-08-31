// name: colourful fireflies
// Clean-room reimplementation from a prose functional description of the
// community pattern "colourful fireflies"; original source never consulted.

// A swarm of colored sparks that dart along the strip with comet tails,
// slow down by friction, coast to a stop, and respawn elsewhere with a new
// random velocity. Density scales with strip length.

var autoSparks = 1 + floor(pixelCount / 10)   // the original's density rule
var maxSparks = max(64, autoSparks)           // allocation ceiling

// --- controls (defaults reproduce the original constants) ---------------
var wantSparks = 0      // 0 = auto (one spark per 10 pixels)
var maxSpeed = 50       // top travel speed of a spark, pixels/second
var trailFade = 10      // trail decay, % of remaining energy per frame
var hueSpread = 1       // fraction of the color wheel the swarm covers
var hueBase = 0         // where that span starts

//# min=0 max=64 step=1 default=0
export function sliderSparkCount(v) { wantSparks = floor(clamp(v, 0, maxSparks)) }

//# min=5 max=200 step=5 default=50
export function sliderMaxSpeedPixelsPerSecond(v) { maxSpeed = max(v, 1) }

//# min=1 max=50 step=1 default=10
export function sliderTrailFadePercent(v) { trailFade = clamp(v, 1, 99) }

//# min=0 max=1 step=0.05 default=1
export function sliderHueSpread(v) { hueSpread = clamp(v, 0, 1) }

// only the hue is used; the swarm keeps its own saturation and brightness
export function hsvPickerBaseColor(h, s, v) { hueBase = h }

var numSparks = autoSparks
var sparkV = array(maxSparks)   // signed velocity, pixels per (scaled) ms
var sparkX = array(maxSparks)   // fractional position in pixel units
var sparkH = array(maxSparks)   // fixed hue, spread around the wheel
var pixV = array(pixelCount)    // accumulated signed energy per pixel
var pixH = array(pixelCount)    // hue stamp: last spark to touch the pixel

// Reference values: each control scales its constant by a ratio that is
// exactly 1 at the control's default, so the untouched pattern renders
// bit-for-bit as before.
var REF_SPEED = 50

var speedRatio = 1     // maxSpeed / REF_SPEED — scales travel, not brightness
var decay = 0.9        // per-frame trail retention

var i
for (i = 0; i < numSparks; i++) {
  sparkV[i] = (random(2) - 1) * 0.5
  sparkX[i] = random(pixelCount)
  sparkH[i] = i / numSparks
}

export function beforeRender(delta) {
  delta *= 0.1                       // global speed trim

  speedRatio = maxSpeed / REF_SPEED
  decay = 0.9 * ((100 - trailFade) / 90)   // == 1 - trailFade/100, exact at 10

  // Spark count changed: seed any newly enabled slots.
  var target = wantSparks
  if (target < 1) target = autoSparks
  if (target > maxSparks) target = maxSparks
  if (target > numSparks) {
    for (i = numSparks; i < target; i++) {
      sparkV[i] = (random(2) - 1) * 0.5
      sparkX[i] = random(pixelCount)
      sparkH[i] = i / target
    }
  }
  numSparks = target

  feedback(pixV, decay)              // trails: default ~10% decay per frame

  for (i = 0; i < numSparks; i++) {
    // Friction has brought it to rest: respawn with fresh speed/position.
    if (abs(sparkV[i]) < 0.005) {
      sparkV[i] = (random(2) - 1) * 0.5
      sparkX[i] = random(pixelCount)
    }

    sparkV[i] *= 0.99                // friction
    sparkX[i] = mod(sparkX[i] + sparkV[i] * speedRatio * delta, pixelCount)

    // Deposit signed energy under the spark and stamp its hue.
    var p = floor(sparkX[i])
    pixV[p] += sparkV[i]
    pixH[p] = sparkH[i]
  }
}

export function render(index) {
  // Squaring makes backward (negative-energy) sparks just as bright, and
  // gives the tails a fast nonlinear fade.
  var v = pixV[index]
  hsv(hueBase + pixH[index] * hueSpread, 0.95, v * v * 10)
}
