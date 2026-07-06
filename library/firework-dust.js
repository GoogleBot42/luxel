// name: firework dust
// Clean-room reimplementation from a prose functional description of the
// community pattern "firework dust"; original source never consulted.

// Nearly-black strip with tiny multicolored single-frame sparks popping
// at random positions — drifting firework embers / glitter. Completely
// stateless: each pixel re-rolls every frame, so sparkle rate scales
// with frame rate (kept as a faithful quirk of the original).

var SPARK_CHANCE = 0.004   // fraction-of-a-percent of pixels lit per frame

export function render(index) {
  var hue = random(1)                        // any color of the wheel
  var lit = random(1) < SPARK_CHANCE         // hard on/off, no dimming
  hsv(hue, 1, lit)
}
