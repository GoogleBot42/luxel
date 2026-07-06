// name: ChristmasPewPew
// Clean-room reimplementation from a prose functional description of the
// community pattern "ChristmasPewPew"; original source never consulted.

// Christmas laser volley: eight green/red projectiles (weighted ~5 green to
// 3 red) fly along the strip at differing random speeds, each dragging an
// exponentially fading tail through a persistent trail buffer. Drawing is
// additive with clamping, so overlaps brighten toward white. A faint
// constant deep-red ambient keeps "empty" pixels dim red instead of black.

var SHOTS = 8
var DECAY = 0.8          // per-frame trail retention (~1/5 lost per frame)
var SPEED_SCALE = 0.015  // pixels per ms per unit of velocity
var AMBIENT_R = 0.06     // faint red underglow

var SHOT_LEVEL = 0.5     // medium-dim projectile channel level

var pos = array(SHOTS)
var vel = array(SHOTS)
var colR = array(SHOTS)
var colG = array(SHOTS)

// Trail canvas: three unpacked channel arrays (the original packed all
// three byte channels into one fixed-point number; not essential).
var trailR = array(pixelCount)
var trailG = array(pixelCount)
var trailB = array(pixelCount)

function rollVelocity() {
  return 1 + random(3)   // 1..4: several-fold speed variation
}

// Initialize the volley: scattered start positions, round-robin palette
// weighted five green to three red, color fixed for life.
for (var i = 0; i < SHOTS; i++) {
  pos[i] = random(pixelCount)
  vel[i] = rollVelocity()
  if (mod(i, 8) < 5) {
    colG[i] = SHOT_LEVEL
  } else {
    colR[i] = SHOT_LEVEL
  }
}

export function beforeRender(delta) {
  // 1. Fade pass: every trail channel decays toward black.
  feedback(trailR, DECAY)
  feedback(trailG, DECAY)
  feedback(trailB, DECAY)

  // 2. Advance each projectile and additively stamp every integer pixel it
  //    passed over since last frame, so fast shots leave solid streaks.
  for (var i = 0; i < SHOTS; i++) {
    var oldPos = pos[i]
    var newPos = oldPos + delta * SPEED_SCALE * vel[i]

    var from = floor(oldPos) + 1
    var to = floor(newPos)
    if (to > pixelCount - 1) to = pixelCount - 1
    for (var p = from; p <= to; p++) {
      if (p < 0) continue
      trailR[p] = min(1, trailR[p] + colR[i])
      trailG[p] = min(1, trailG[p] + colG[i])
    }

    // 3. Respawn past the end: back to zero with a fresh random speed.
    if (newPos >= pixelCount) {
      newPos = 0
      vel[i] = rollVelocity()
    }
    pos[i] = newPos
  }
}

export function render(index) {
  // Pure read: trail plus ambient, clamped, emitted as RGB.
  rgb(
    min(1, trailR[index] + AMBIENT_R),
    trailG[index],
    trailB[index]
  )
}
