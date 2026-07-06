// name: sparkfire
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparkfire"; original source never consulted.

// Fire burns from index 0 toward the far end. Ember sparks launch at the
// base, accelerate (position advances by speed SQUARED, so they run away
// as they age), and deposit heat along every pixel they cross. The heat
// buffer cools two ways at once (subtractive + slight multiplicative) and
// convects upward via a top-down weighted smear of the four lower
// neighbors. Heat drives a classic palette: black -> deep red -> orange ->
// golden yellow -> near-white where heat piles past unity.

var NUM_SPARKS = 5
var SPEED = 1          // global pacing factor (dt is in seconds * SPEED)
var ACCEL = 1.6        // linear speed growth per second
var MAX_LAUNCH = 1.2   // cap on a fresh spark's random speed
var COOL_SUB = 0.9     // heat units subtracted per second
var COOL_MUL = 0.35    // fraction of heat lost per second (multiplicative)
var DEPOSIT = 0.5      // "roughly half strength" additive deposit scale

var heat = array(pixelCount)
var sparkSpeed = array(NUM_SPARKS)
var sparkPos = array(NUM_SPARKS)

// Startup only: sparks scattered anywhere with modest speeds, so the
// strip isn't empty for the first seconds. After this they always
// relaunch from position zero.
var i
for (i = 0; i < NUM_SPARKS; i++) {
  sparkSpeed[i] = 0.2 + random(MAX_LAUNCH - 0.2)
  sparkPos[i] = random(pixelCount)
}

export function beforeRender(delta) {
  var dt = delta / 1000 * SPEED
  var i, j

  // 1. Cooling: subtractive amount snaps to zero if it would overshoot,
  // otherwise subtract and also decay slightly.
  var sub = COOL_SUB * dt
  var keep = 1 - COOL_MUL * dt
  for (i = 0; i < pixelCount; i++) {
    if (heat[i] <= sub) {
      heat[i] = 0
    } else {
      heat[i] = (heat[i] - sub) * keep
    }
  }

  // 2. Convection: walk top-down so each pixel reads its lower
  // neighbors' still-unmodified values. Farthest-below neighbors weigh
  // most (1:1:2:3 over 7) -> heat smears upward like rising flame.
  for (i = pixelCount - 1; i >= 4; i--) {
    heat[i] = (heat[i - 1] + heat[i - 2] + 2 * heat[i - 3] + 3 * heat[i - 4]) / 7
  }

  // 3. Sparks.
  for (i = 0; i < NUM_SPARKS; i++) {
    if (sparkSpeed[i] <= 0) {
      // Respawn at the base with a fresh modest speed.
      sparkSpeed[i] = 0.2 + random(MAX_LAUNCH - 0.2)
      sparkPos[i] = 0
    }
    sparkSpeed[i] += ACCEL * dt
    var oldPos = sparkPos[i]
    // Speed squared: slow launch, runaway travel as the spark ages.
    sparkPos[i] += sparkSpeed[i] * sparkSpeed[i] * dt * pixelCount / 12

    if (sparkPos[i] >= pixelCount) {
      // Off the end: kill it, deposit nothing; respawns next frame.
      sparkPos[i] = 0
      sparkSpeed[i] = 0
    } else {
      // Deposit along the whole span crossed this frame so fast sparks
      // leave gap-free trails. Older/faster sparks deposit less.
      var amount = max(0, MAX_LAUNCH * 1.6 - sparkSpeed[i] * 0.5) * DEPOSIT
      for (j = floor(oldPos); j <= floor(sparkPos[i]); j++) {
        heat[j] += amount
      }
    }
  }
}

export function render(index) {
  var h = heat[index]
  var hc = min(h, 1.4)
  // Hue shifts red -> yellow with heat squared: only genuinely hot
  // pixels leave red.
  var hue = 0.09 * hc * hc
  // Full saturation until heat passes unity, then a steep bleach to white.
  var sat = clamp(1 - (h - 1) * 2, 0, 1)
  var v = clamp(h * 2, 0, 1)
  hsv(hue, sat, v)
}
