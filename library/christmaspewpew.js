// name: ChristmasPewPew
// Clean-room reimplementation from a prose functional description of the
// community pattern "ChristmasPewPew"; original source never consulted.
//
// Green and red laser volleys fly down the strip at mixed random speeds,
// dragging fading tails over a faint constant deep-red underglow. Overlaps
// add and clamp. Plain per-channel arrays replace the original's packed
// fixed-point channel trick (an artifact, not essential).

const NUM_SHOTS = 8
const DECAY = 0.8          // trail keeps ~4/5 of its brightness per frame
const SPEED_SCALE = 0.0008 // pixels per ms per unit velocity (× pixelCount)
const AMBIENT_R = 0.04     // faint deep-red wash on "empty" pixels
const SHOT_LEVEL = 0.35    // projectiles are dim so additive overlap
                           // brightens without hue-shifting badly

// trail canvas, one set of channels per pixel
var trailR = array(pixelCount)
var trailG = array(pixelCount)
var trailB = array(pixelCount)

var pos = array(NUM_SHOTS)   // fractional pixel position
var vel = array(NUM_SHOTS)   // random 1..4, several-fold speed spread
var red = array(NUM_SHOTS)   // 1 = red shot, 0 = green shot

// round-robin colors weighted toward green: 5 green, 3 red out of 8
var i
for (i = 0; i < NUM_SHOTS; i++) {
  red[i] = (i == 2 || i == 5 || i == 7) ? 1 : 0
  pos[i] = random(pixelCount)         // stagger the opening volley
  vel[i] = 1 + random(3)
}

export function beforeRender(delta) {
  // fade pass: trails decay exponentially toward black
  feedback(trailR, DECAY)
  feedback(trailG, DECAY)
  feedback(trailB, DECAY)

  var s
  for (s = 0; s < NUM_SHOTS; s++) {
    var newPos = pos[s] + delta * SPEED_SCALE * pixelCount * vel[s]

    // stamp every integer pixel passed over since last frame so fast
    // shots draw continuous streaks, not dotted lines
    var p
    for (p = floor(pos[s]) + 1; p <= floor(newPos); p++) {
      if (p >= 0 && p < pixelCount) {
        if (red[s]) {
          trailR[p] = min(1, trailR[p] + SHOT_LEVEL)
        } else {
          trailG[p] = min(1, trailG[p] + SHOT_LEVEL)
        }
      }
    }

    if (newPos >= pixelCount) {
      // respawn at the start with a fresh random speed; color is for life
      pos[s] = 0
      vel[s] = 1 + random(3)
    } else {
      pos[s] = newPos
    }
  }
}

export function render(index) {
  rgb(
    min(1, trailR[index] + AMBIENT_R),
    trailG[index],
    trailB[index]
  )
}
