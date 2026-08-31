// name: firework dust
// Clean-room reimplementation from a prose functional description of the
// community pattern "firework dust"; original source never consulted.

// Nearly-black strip with tiny multicolored single-frame sparks popping
// at random positions — drifting firework embers / glitter. Completely
// stateless at the default fade of 0: each pixel re-rolls every frame, so
// sparkle rate scales with frame rate (kept as a faithful quirk of the
// original).

var SPARK_CHANCE = 0.004   // fraction-of-a-percent of pixels lit per frame

// --- controls (defaults reproduce the original constants) ---------------
var density = 0.4      // % of pixels sparking per frame (0.4% == 0.004)
var fadeSecs = 0       // seconds a spark takes to fade out; 0 = single-frame
var hueSpread = 1      // fraction of the color wheel the sparks may take
var hueBase = 0        // where that span starts
var sat = 1            // saturation, from the color picker

//# min=0 max=5 step=0.05 default=0.4
export function sliderDensityPercent(v) { density = clamp(v, 0, 100) }

//# min=0 max=3 step=0.05 default=0
export function sliderFadeSeconds(v) { fadeSecs = max(v, 0) }

//# min=0 max=1 step=0.05 default=1
export function sliderHueSpread(v) { hueSpread = clamp(v, 0, 1) }

export function hsvPickerSparkColor(h, s, v) { hueBase = h; sat = s }

var val = array(pixelCount)    // brightness left in each spark
var hues = array(pixelCount)   // the hue that spark was born with
var chance = 0.004

export function beforeRender(delta) {
  // 0.004 * ratio is exactly 0.004 at the control's default
  chance = 0.004 * (density / 0.4)

  if (fadeSecs <= 0) {
    // no fade: every spark lasts exactly one frame, as the original
    for (var i = 0; i < pixelCount; i++) val[i] = 0
  } else {
    var drop = delta / 1000 / fadeSecs
    for (var j = 0; j < pixelCount; j++) val[j] = max(val[j] - drop, 0)
  }
}

export function render(index) {
  var hue = random(1)                    // any color of the allowed span
  var lit = random(1) < chance           // hard on, then fade (or not)
  if (lit) {
    hues[index] = hueBase + hue * hueSpread
    val[index] = 1
  }
  hsv(hues[index], sat, val[index])
}
